// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Model-free retrieval over a borrowed, owner-authorized retained corpus.
//! Passage selectors count Unicode scalar values, not UTF-8 bytes.

use crate::{DocumentIndex, DocumentIndexConfig, SearchDocument};
use eidetic::models::recall::{
    InferencePassage, PassageSelector, RecallSourceId, RetainedSourceText,
};
use std::{
    cmp::Ordering,
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

pub const CHUNKER_VERSION: &str = "mere.unicode-chunks/v1";

/// The host owns storage and decides which sources are admitted. The revision
/// must change on any session or admission change, including removal and
/// readmission of the same version. Source head changes are observed separately.
pub trait RetainedSourceOwner: Send + Sync {
    fn snapshot(&self) -> Result<Vec<RetainedSourceText>, String>;
    fn current_version(&self, source: &RecallSourceId) -> Result<Option<String>, String>;
    fn scope_revision(&self) -> Result<String, String>;
}

#[derive(Clone, Copy, Debug)]
pub struct RecallIndexConfig {
    pub chunk_chars: usize,
    pub max_corpus_chars: usize,
}

pub type SourceHeads = BTreeMap<RecallSourceId, Result<Option<String>, String>>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StaleSource {
    pub source: RecallSourceId,
    pub indexed_version: String,
    pub current_version: Option<String>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct SearchResult {
    pub passages: Vec<InferencePassage>,
    pub stale: Vec<StaleSource>,
    /// Selected identities absent from the indexed snapshot; absence is not a
    /// claim that the source has no matching text.
    pub unavailable_sources: Vec<RecallSourceId>,
}

pub struct RecallIndex {
    owner: Arc<dyn RetainedSourceOwner>,
    scope_revision: String,
    sources: BTreeMap<RecallSourceId, String>,
    index: DocumentIndex<String>,
    passages: BTreeMap<String, InferencePassage>,
}

impl RecallIndex {
    pub fn build(
        owner: Arc<dyn RetainedSourceOwner>,
        config: RecallIndexConfig,
    ) -> Result<Self, String> {
        if config.chunk_chars == 0 || config.max_corpus_chars == 0 {
            return Err("recall limits must be greater than zero".into());
        }
        let revision = owner.scope_revision()?;
        let mut input = owner.snapshot()?;
        if owner.scope_revision()? != revision {
            return Err("recall scope changed during snapshot".into());
        }
        input.sort_by(|a, b| (&a.source, &a.version).cmp(&(&b.source, &b.version)));
        let mut total = 0usize;
        let mut sources = BTreeMap::new();
        let mut index = DocumentIndex::new(DocumentIndexConfig::default());
        let mut passages = BTreeMap::new();
        for source in &input {
            if source.source.source.trim().is_empty() || source.version.trim().is_empty() {
                return Err("recall requires a source identity and exact version".into());
            }
            if sources
                .insert(source.source.clone(), source.version.clone())
                .is_some()
            {
                return Err("recall corpus repeats a source identity".into());
            }
            let char_count = source.text.chars().count();
            total = total
                .checked_add(char_count)
                .ok_or("recall corpus size overflow")?;
            if total > config.max_corpus_chars {
                return Err("recall corpus exceeds configured character limit".into());
            }
            let chars: Vec<char> = source.text.chars().collect();
            let text_hash = eidetic::Hash::of(source.text.as_bytes());
            for start in (0..chars.len()).step_by(config.chunk_chars) {
                let end = start.saturating_add(config.chunk_chars).min(chars.len());
                let text: String = chars[start..end].iter().collect();
                if text.trim().is_empty() {
                    continue;
                }
                let passage = InferencePassage {
                    source: source.source.clone(),
                    version: source.version.clone(),
                    text_hash,
                    selector: PassageSelector { start, end },
                    chunker: format!("{CHUNKER_VERSION}/{}", config.chunk_chars),
                    text,
                };
                let key = passage.id().to_string();
                index.add(SearchDocument {
                    key: key.clone(),
                    primary_address: source.source.source.clone(),
                    aliases: vec![],
                    title: source.title.clone(),
                    body: Some(passage.text.clone()),
                });
                passages.insert(key, passage);
            }
        }
        Ok(Self {
            owner,
            scope_revision: revision,
            sources,
            index,
            passages,
        })
    }

    pub fn source_ids(&self) -> Vec<RecallSourceId> {
        self.sources.keys().cloned().collect()
    }
    pub fn passages(&self) -> &BTreeMap<String, InferencePassage> {
        &self.passages
    }

    /// Observes each indexed selected source once. Empty scope selects nothing.
    pub fn observe_heads(&self, scope: &[RecallSourceId]) -> SourceHeads {
        scope
            .iter()
            .filter(|source| self.sources.contains_key(*source))
            .map(|source| source.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .map(|source| {
                let head = self.owner.current_version(&source);
                (source, head)
            })
            .collect()
    }

    /// Observe heads again, checking the snapshot session/admission revision.
    /// This does not compare the new heads with an earlier observation.
    pub fn reobserve_heads(&self, scope: &[RecallSourceId]) -> Result<SourceHeads, String> {
        if self.owner.scope_revision()? != self.scope_revision {
            return Err("recall scope changed during operation".into());
        }
        let current = self.observe_heads(scope);
        if self.owner.scope_revision()? != self.scope_revision {
            return Err("recall scope changed during operation".into());
        }
        Ok(current)
    }

    /// Rankings against supplied observations only, without a current-authority
    /// guarantee. Callers emitting current-authorized results must recheck
    /// admission and compare the observations with `reobserve_heads`.
    pub fn rankings_observed(
        &self,
        question: &str,
        scope: &[RecallSourceId],
        heads: &SourceHeads,
    ) -> Vec<String> {
        if question.trim().is_empty() || scope.is_empty() {
            return vec![];
        }
        let selected: BTreeSet<_> = scope.iter().collect();
        let mut hits: Vec<_> = self
            .index
            .search(question, self.index.len())
            .into_iter()
            .filter(|hit| {
                let p = &self.passages[&hit.key];
                selected.contains(&p.source)
                    && heads
                        .get(&p.source)
                        .and_then(|h| h.as_ref().ok())
                        .and_then(|h| h.as_ref())
                        == Some(&p.version)
            })
            .collect();
        hits.sort_by(|a, b| {
            b.score
                .total_cmp(&a.score)
                .then_with(|| self.content_tie_order(&a.key, &b.key))
        });
        hits.into_iter().map(|hit| hit.key).collect()
    }

    pub fn content_tie_order(&self, a: &str, b: &str) -> Ordering {
        self.passages[b]
            .text
            .cmp(&self.passages[a].text)
            .then_with(|| a.cmp(b))
    }

    /// Search an explicit selection and revalidate the owner before returning.
    pub fn search(&self, question: &str, scope: &[RecallSourceId]) -> Result<SearchResult, String> {
        if self.owner.scope_revision()? != self.scope_revision {
            return Err("recall scope changed during operation".into());
        }
        let observed = self.observe_heads(scope);
        let keys = self.rankings_observed(question, scope, &observed);
        let final_heads = self.reobserve_heads(scope)?;
        let mut result = SearchResult::default();
        let selected: BTreeSet<_> = scope.iter().cloned().collect();
        for source in selected {
            let Some(version) = self.sources.get(&source) else {
                result.unavailable_sources.push(source);
                continue;
            };
            let initial = &observed[&source];
            let final_head = &final_heads[&source];
            if initial.as_ref().ok().and_then(|v| v.as_ref()) != Some(version)
                || final_head.as_ref().ok().and_then(|v| v.as_ref()) != Some(version)
            {
                let invalid = if final_head.as_ref().ok().and_then(|v| v.as_ref()) != Some(version)
                {
                    final_head
                } else {
                    initial
                };
                result.stale.push(StaleSource {
                    source,
                    indexed_version: version.clone(),
                    current_version: invalid.as_ref().ok().cloned().flatten(),
                    error: invalid.as_ref().err().cloned(),
                });
            }
        }
        result.passages = keys
            .into_iter()
            .filter_map(|key| {
                let p = &self.passages[&key];
                (final_heads
                    .get(&p.source)
                    .and_then(|v| v.as_ref().ok())
                    .and_then(|v| v.as_ref())
                    == Some(&p.version))
                .then(|| p.clone())
            })
            .collect();
        if self.owner.scope_revision()? != self.scope_revision {
            return Err("recall scope changed during operation".into());
        }
        Ok(result)
    }
}
