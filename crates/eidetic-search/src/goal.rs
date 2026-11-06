// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Model-free goal assessment for a bounded, goal-directed traversal.
//!
//! A person states a goal; a traversal host (Fleece's `run_traversal`) asks
//! two questions as it walks supplied, already-retained sources: *which pending
//! candidate next?* and *does this observed source serve the goal?* This
//! module answers both without a model, from goal-term coverage under the
//! crate's one tokenizer, and keeps every answer as an inspectable record:
//! the exact retained version and text hash assessed, the goal terms matched
//! and missed, the selectors (Unicode scalar offsets) of each match, and the
//! rule that chose each candidate. A model assessor can replace or rerank
//! these answers; the record shape is what a caller cites.
//!
//! Nothing here fetches, admits or writes. The host owns observation,
//! authorization and any graph change, which stays a proposal.

use crate::query::{QueryError, QueryMode, matches_for};
use crate::tokenize::{SEGMENTATION_PROFILE, TOKENIZER_NAME, Tokenizer};
use eidetic::models::recall::{PassageSelector, RecallSourceId, RetainedSourceText};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Operation identity recorded with every [`GoalRecord`].
pub const GOAL_OPERATION_VERSION: &str = "eidetic.goal-assessment/v2";

/// Identity of [`STOP_WORDS`], recorded in [`GoalRecord::stop_words`] when the
/// list was applied.
pub const STOP_WORDS_ID: &str = "eidetic.stop-words/en-v1";

/// Small fixed English stop list: articles, conjunctions, prepositions,
/// pronouns and auxiliaries. Removed from goal terms only, never from source
/// text. Sorted for binary search; changing it requires a new id.
pub const STOP_WORDS: &[&str] = &[
    "a", "about", "after", "all", "am", "an", "and", "any", "are", "as", "at", "be", "been", "but",
    "by", "can", "could", "did", "do", "does", "for", "from", "had", "has", "have", "he", "her",
    "him", "his", "i", "if", "in", "into", "is", "it", "its", "me", "my", "nor", "of", "on", "or",
    "our", "she", "should", "so", "that", "the", "their", "them", "then", "they", "this", "to",
    "us", "was", "we", "were", "while", "who", "will", "with", "would", "you", "your",
];

fn is_stop_word(term: &str) -> bool {
    STOP_WORDS.binary_search(&term).is_ok()
}

fn default_true() -> bool {
    true
}
fn default_gap() -> usize {
    40
}
fn default_context() -> usize {
    60
}

/// Thresholds and input bounds for one goal run.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoalSettings {
    /// Minimum share of distinct goal terms, in thousandths, a source must
    /// contain to count as a match. 1000 requires every term.
    pub min_coverage_permille: u16,
    /// Matched sources after which the goal is reported satisfied.
    pub satisfy_after: usize,
    /// Maximum characters of goal text.
    pub max_goal_chars: usize,
    /// Maximum characters of one assessed source text or candidate label.
    pub max_text_chars: usize,
    /// Maximum match selectors kept per assessment.
    pub max_selectors: usize,
    /// Drop [`STOP_WORDS`] from goal terms (unless that would leave none).
    #[serde(default = "default_true")]
    pub ignore_stop_words: bool,
    /// Matches overlapping or within this many scalars merge into one passage.
    #[serde(default = "default_gap")]
    pub passage_gap_chars: usize,
    /// Scalars of context added each side of a passage, clamped to the text
    /// and snapped to whitespace where possible.
    #[serde(default = "default_context")]
    pub passage_context_chars: usize,
}

impl Default for GoalSettings {
    fn default() -> Self {
        Self {
            min_coverage_permille: 500,
            satisfy_after: 3,
            max_goal_chars: 10_000,
            max_text_chars: 10_000_000,
            max_selectors: 64,
            ignore_stop_words: true,
            passage_gap_chars: default_gap(),
            passage_context_chars: default_context(),
        }
    }
}

/// The answer to "does this source serve the goal?", in the vocabulary of a
/// traversal host: keep walking, record a match, or stop satisfied.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum GoalVerdict {
    Continue,
    Match,
    Satisfied,
}

/// One assessed source, citing the exact retained version it read.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GoalAssessment {
    pub source: RecallSourceId,
    pub version: String,
    pub text_hash: eidetic::Hash,
    pub matched_terms: Vec<String>,
    pub missing_terms: Vec<String>,
    pub coverage_permille: u16,
    /// Body matches in Unicode scalar offsets of the retained text, capped at
    /// [`GoalSettings::max_selectors`].
    pub matches: Vec<PassageSelector>,
    /// Whether matches beyond the cap were left out.
    pub matches_truncated: bool,
    /// `matches` merged into passages (gap-joined, context-expanded), in
    /// scalar offsets, capped at [`GoalSettings::max_selectors`].
    #[serde(default)]
    pub passages: Vec<PassageSelector>,
    /// Whether passages beyond the cap were left out.
    #[serde(default)]
    pub passages_truncated: bool,
    pub verdict: GoalVerdict,
}

/// What is known of a pending candidate before it is observed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CandidateLabel {
    pub source: RecallSourceId,
    /// Link text, title or other label the host already holds; never fetched.
    pub label: Option<String>,
}

/// Why a candidate was chosen.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SelectionRule {
    /// The label sharing the most distinct goal terms; ties go to the oldest.
    GoalTermOverlap,
    /// No label shared a goal term, so the oldest candidate was taken.
    OldestFallback,
}

/// One selection among pending candidates.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectionChoice {
    pub index: usize,
    pub pending: usize,
    pub source: RecallSourceId,
    pub matched_terms: Vec<String>,
    pub rule: SelectionRule,
}

/// The complete, citable account of a goal run's choices.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GoalRecord {
    pub operation_version: String,
    pub tokenizer: String,
    pub goal: String,
    pub terms: Vec<String>,
    /// [`STOP_WORDS_ID`] when the stop list was applied to the goal terms.
    #[serde(default)]
    pub stop_words: Option<String>,
    pub settings: GoalSettings,
    pub selections: Vec<SelectionChoice>,
    pub assessments: Vec<GoalAssessment>,
    pub matched: usize,
}

/// Stateful assessor and selector for one goal run.
#[derive(Debug)]
pub struct GoalAssessor {
    tokenizer: Tokenizer,
    goal: String,
    terms: Vec<String>,
    stop_words: Option<String>,
    settings: GoalSettings,
    selections: Vec<SelectionChoice>,
    assessments: Vec<GoalAssessment>,
    matched: usize,
}

impl GoalAssessor {
    /// Refuses empty or oversized goals, a goal with no searchable term, and
    /// unusable settings.
    pub fn new(goal: &str, settings: GoalSettings) -> Result<Self, QueryError> {
        if settings.min_coverage_permille == 0
            || settings.min_coverage_permille > 1000
            || settings.satisfy_after == 0
            || settings.max_goal_chars == 0
            || settings.max_text_chars == 0
        {
            return Err("invalid goal settings".into());
        }
        if goal.chars().count() > settings.max_goal_chars {
            return Err("goal exceeds max_goal_chars".into());
        }
        let tokenizer = Tokenizer::new();
        let all: Vec<String> = tokenizer
            .tokens(goal)
            .into_iter()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        if all.is_empty() {
            return Err("goal has no searchable terms".into());
        }
        let mut terms = all.clone();
        let mut stop_words = None;
        if settings.ignore_stop_words {
            let kept: Vec<String> = all.iter().filter(|t| !is_stop_word(t)).cloned().collect();
            // A goal made only of stop words keeps its terms.
            if !kept.is_empty() {
                terms = kept;
                stop_words = Some(STOP_WORDS_ID.to_owned());
            }
        }
        Ok(Self {
            tokenizer,
            goal: goal.to_owned(),
            terms,
            stop_words,
            settings,
            selections: Vec::new(),
            assessments: Vec::new(),
            matched: 0,
        })
    }

    /// The distinct goal terms, sorted.
    pub fn terms(&self) -> &[String] {
        &self.terms
    }

    /// The goal text as given.
    pub fn goal(&self) -> &str {
        &self.goal
    }

    /// The settings this run was built with.
    pub fn settings(&self) -> &GoalSettings {
        &self.settings
    }

    /// The most recent assessment, without cloning the whole record.
    pub fn last_assessment(&self) -> Option<&GoalAssessment> {
        self.assessments.last()
    }

    /// The most recent selection, without cloning the whole record.
    pub fn last_selection(&self) -> Option<&SelectionChoice> {
        self.selections.last()
    }

    /// Assess one observed, retained source. A refused input (oversized text)
    /// records nothing and leaves the run's state unchanged.
    pub fn assess(&mut self, retained: &RetainedSourceText) -> Result<GoalVerdict, QueryError> {
        let body_chars = retained.text.chars().count();
        let title_chars = retained.title.as_ref().map_or(0, |t| t.chars().count());
        if body_chars.saturating_add(title_chars) > self.settings.max_text_chars {
            return Err("source text exceeds max_text_chars".into());
        }
        let mut present = self.present_terms(&retained.text);
        if let Some(title) = &retained.title {
            present.extend(self.present_terms(title));
        }
        let (matched_terms, missing_terms): (Vec<_>, Vec<_>) = self
            .terms
            .iter()
            .cloned()
            .partition(|t| present.contains(t));
        let coverage_permille = permille(matched_terms.len(), self.terms.len());
        let mut matches = matches_for(
            &retained.text,
            &QueryMode::Terms(String::new()),
            &matched_terms,
        );
        matches.sort_by_key(|m| (m.start, m.end));
        let (passages, passages_truncated) = self.passages(&retained.text, &matches);
        let matches_truncated = matches.len() > self.settings.max_selectors;
        matches.truncate(self.settings.max_selectors);
        let verdict = if coverage_permille < self.settings.min_coverage_permille {
            GoalVerdict::Continue
        } else {
            self.matched += 1;
            if self.matched >= self.settings.satisfy_after {
                GoalVerdict::Satisfied
            } else {
                GoalVerdict::Match
            }
        };
        self.assessments.push(GoalAssessment {
            source: retained.source.clone(),
            version: retained.version.clone(),
            text_hash: eidetic::Hash::of(retained.text.as_bytes()),
            matched_terms,
            missing_terms,
            coverage_permille,
            matches,
            matches_truncated,
            passages,
            passages_truncated,
            verdict,
        });
        Ok(verdict)
    }

    /// Choose the pending candidate whose label shares the most distinct goal
    /// terms, oldest first among equals; with no overlap anywhere, the oldest.
    /// Oversized labels count as unlabeled. Returns `None` only when empty.
    pub fn select(&mut self, candidates: &[CandidateLabel]) -> Option<usize> {
        let mut best: Option<(usize, Vec<String>)> = None;
        for (index, candidate) in candidates.iter().enumerate() {
            let Some(label) = candidate
                .label
                .as_deref()
                .filter(|l| l.chars().count() <= self.settings.max_text_chars)
            else {
                continue;
            };
            let present = self.present_terms(label);
            let shared: Vec<String> = self
                .terms
                .iter()
                .filter(|t| present.contains(*t))
                .cloned()
                .collect();
            if !shared.is_empty() && best.as_ref().is_none_or(|(_, b)| shared.len() > b.len()) {
                best = Some((index, shared));
            }
        }
        let (index, matched_terms, rule) = match best {
            Some((index, shared)) => (index, shared, SelectionRule::GoalTermOverlap),
            None if !candidates.is_empty() => (0, Vec::new(), SelectionRule::OldestFallback),
            None => return None,
        };
        self.selections.push(SelectionChoice {
            index,
            pending: candidates.len(),
            source: candidates[index].source.clone(),
            matched_terms,
            rule,
        });
        Some(index)
    }

    /// The citable account of every choice so far.
    pub fn record(&self) -> GoalRecord {
        GoalRecord {
            operation_version: GOAL_OPERATION_VERSION.into(),
            tokenizer: format!("{TOKENIZER_NAME}/{SEGMENTATION_PROFILE}"),
            goal: self.goal.clone(),
            terms: self.terms.clone(),
            stop_words: self.stop_words.clone(),
            settings: self.settings.clone(),
            selections: self.selections.clone(),
            assessments: self.assessments.clone(),
            matched: self.matched,
        }
    }

    /// Merge sorted matches into passages, then expand by context.
    fn passages(&self, text: &str, matches: &[PassageSelector]) -> (Vec<PassageSelector>, bool) {
        let mut merged: Vec<PassageSelector> = Vec::new();
        for m in matches {
            match merged.last_mut() {
                Some(last)
                    if m.start <= last.end.saturating_add(self.settings.passage_gap_chars) =>
                {
                    last.end = last.end.max(m.end);
                }
                _ => merged.push(m.clone()),
            }
        }
        let truncated = merged.len() > self.settings.max_selectors;
        merged.truncate(self.settings.max_selectors);
        let context = self.settings.passage_context_chars;
        if context > 0 && !merged.is_empty() {
            let chars: Vec<char> = text.chars().collect();
            let len = chars.len();
            let mut out: Vec<PassageSelector> = Vec::new();
            for p in &merged {
                let mut start = p.start.saturating_sub(context);
                while start < p.start && start > 0 && !chars[start - 1].is_whitespace() {
                    start += 1;
                }
                let mut end = p.end.saturating_add(context).min(len);
                while end > p.end && end < len && !chars[end].is_whitespace() {
                    end -= 1;
                }
                match out.last_mut() {
                    Some(last) if start <= last.end => last.end = last.end.max(end),
                    _ => out.push(PassageSelector { start, end }),
                }
            }
            merged = out;
        }
        (merged, truncated)
    }

    fn present_terms(&self, text: &str) -> BTreeSet<String> {
        self.tokenizer.tokens(text).into_iter().collect()
    }
}

fn permille(part: usize, whole: usize) -> u16 {
    if whole == 0 {
        return 0;
    }
    // part <= whole, so the quotient is at most 1000.
    u16::try_from(part.saturating_mul(1000) / whole).unwrap_or(1000)
}

#[cfg(test)]
#[path = "goal_tests.rs"]
mod tests;
