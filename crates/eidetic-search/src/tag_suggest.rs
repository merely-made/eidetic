// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Model-free tag suggestions from concept labels.
//!
//! Given retained source text and a set of known tag concepts (identity plus
//! a human label), suggest each concept whose label terms all occur in the
//! source body under the crate's one tokenizer. Every suggestion is an
//! [`eidetic::records::TagSuggestionV1`] citing the retained version, the text
//! hash, the producer, the caller's run id, a digest of the settings and the
//! scalar-offset selectors of the label words found. It is a proposal only:
//! it is stored through the suggestion ledger and reviewed there, and never
//! becomes a personal tag by itself.
//!
//! Matching is exact on lowercased tokens: there is no diacritic folding, so
//! `cafe` does not match `café`. Only the body is read, because selectors
//! address the body text stream; a title hit is not evidence.

use crate::query::{QueryError, QueryMode, matches_for};
use crate::tokenize::{SEGMENTATION_PROFILE, TOKENIZER_NAME, Tokenizer};
use eidetic::Hash;
use eidetic::models::recall::{PassageSelector, RecallSourceId, RetainedSourceText};
use eidetic::records::{
    KeepingTier, SuggestionProducer, SuggestionProducerKind, TagIdentity, TagSuggestionV1,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// The rule name recorded as the producer.
pub const TAG_SUGGEST_RULE: &str = "eidetic.tag-suggest/label-terms-v1";
/// The operation version recorded with every suggestion.
pub const TAG_SUGGEST_OPERATION_VERSION: &str = "label-terms/v1";

/// A known tag concept and the label its words are searched for.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TagConcept {
    pub tag: TagIdentity,
    pub label: String,
}

/// Thresholds and bounds; serialized into the settings digest.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TagSuggestSettings {
    /// Minimum share of distinct label terms present, in thousandths. The
    /// default 1000 emits full matches only.
    pub min_coverage_permille: u16,
    /// Maximum evidence selectors per suggestion. The first occurrence of each
    /// present term is always kept first, so this must be at least the label's
    /// term count to cover every word; shorter caps drop later terms.
    pub max_selectors: usize,
    /// Maximum characters of one source body.
    pub max_text_chars: usize,
    /// Lifecycle the suggestions declare.
    pub keeping: KeepingTier,
}

impl Default for TagSuggestSettings {
    fn default() -> Self {
        Self {
            min_coverage_permille: 1000,
            max_selectors: 16,
            max_text_chars: 10_000_000,
            keeping: KeepingTier::Retained,
        }
    }
}

#[derive(Serialize)]
struct DigestInput<'a> {
    operation_version: &'a str,
    tokenizer: String,
    settings: &'a TagSuggestSettings,
}

/// BLAKE3 of the canonical settings JSON, with operation and tokenizer.
///
/// # Panics
/// Never in practice: plain settings always serialize.
pub fn settings_digest(settings: &TagSuggestSettings) -> Hash {
    let input = DigestInput {
        operation_version: TAG_SUGGEST_OPERATION_VERSION,
        tokenizer: format!("{TOKENIZER_NAME}/{SEGMENTATION_PROFILE}"),
        settings,
    };
    Hash::of(&serde_json::to_vec(&input).expect("settings serialize"))
}

/// Suggest concept tags for `sources`.
///
/// `run` is the caller's run id. `existing` lists (source, concept) pairs
/// already tagged; those concepts are skipped for that source. Output order
/// is source order, then concept identity order, independent of the order
/// `concepts` is given in. Concepts with no label terms, and duplicate
/// concept identities after the first, are ignored.
///
/// # Errors
/// An empty run id, unusable settings, or a source body over
/// `max_text_chars`.
pub fn suggest_tags(
    sources: &[RetainedSourceText],
    concepts: &[TagConcept],
    existing: &[(RecallSourceId, TagIdentity)],
    run: &str,
    settings: &TagSuggestSettings,
) -> Result<Vec<TagSuggestionV1>, QueryError> {
    if run.trim().is_empty() {
        return Err("run id empty".into());
    }
    if settings.min_coverage_permille == 0
        || settings.min_coverage_permille > 1000
        || settings.max_selectors == 0
        || settings.max_text_chars == 0
    {
        return Err("invalid tag suggestion settings".into());
    }
    let tokenizer = Tokenizer::new();
    let mut labelled: Vec<(&TagIdentity, Vec<String>)> = Vec::new();
    for concept in concepts {
        let terms: Vec<String> = tokenizer
            .tokens(&concept.label)
            .into_iter()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        if !terms.is_empty() && !labelled.iter().any(|(t, _)| **t == concept.tag) {
            labelled.push((&concept.tag, terms));
        }
    }
    labelled.sort_by(|a, b| a.0.cmp(b.0));
    let digest = settings_digest(settings);
    let mut out = Vec::new();
    for source in sources {
        if source.text.chars().count() > settings.max_text_chars {
            return Err("source text exceeds max_text_chars".into());
        }
        let present: BTreeSet<String> = tokenizer.tokens(&source.text).into_iter().collect();
        let text_hash = Hash::of(source.text.as_bytes());
        for (tag, terms) in &labelled {
            if existing
                .iter()
                .any(|(s, t)| *s == source.source && t == *tag)
            {
                continue;
            }
            let found: Vec<String> = terms
                .iter()
                .filter(|t| present.contains(*t))
                .cloned()
                .collect();
            let coverage = found.len() * 1000 / terms.len();
            if found.is_empty() || coverage < usize::from(settings.min_coverage_permille) {
                continue;
            }
            out.push(TagSuggestionV1 {
                subject: source.source.clone(),
                subject_version: source.version.clone(),
                input_text_hash: text_hash,
                concept: (*tag).clone(),
                producer: SuggestionProducer {
                    kind: SuggestionProducerKind::Rule {
                        name: TAG_SUGGEST_RULE.into(),
                    },
                    operation_version: TAG_SUGGEST_OPERATION_VERSION.into(),
                },
                run: run.into(),
                settings_digest: digest,
                evidence: evidence(&source.text, &found, settings.max_selectors),
                score: Some(found.len() as f32 / terms.len() as f32),
                keeping: settings.keeping,
            });
        }
    }
    Ok(out)
}

/// First occurrence of each term, then the rest, capped, in text order.
fn evidence(text: &str, terms: &[String], cap: usize) -> Vec<PassageSelector> {
    let per_term: Vec<Vec<PassageSelector>> = terms
        .iter()
        .map(|t| {
            matches_for(
                text,
                &QueryMode::Terms(String::new()),
                std::slice::from_ref(t),
            )
        })
        .collect();
    let mut chosen: Vec<PassageSelector> = per_term
        .iter()
        .filter_map(|m| m.first().cloned())
        .take(cap)
        .collect();
    for m in &per_term {
        for sel in m.iter().skip(1) {
            if chosen.len() >= cap {
                break;
            }
            chosen.push(sel.clone());
        }
    }
    chosen.sort_by_key(|s| (s.start, s.end));
    chosen
}

#[cfg(test)]
#[path = "tag_suggest_tests.rs"]
mod tests;
