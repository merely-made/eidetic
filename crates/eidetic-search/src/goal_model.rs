// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The model path of goal assessment (P4 S3).
//!
//! [`ModelGoalAssessor`] wraps S1's [`GoalAssessor`]. S1 always runs and its
//! record is kept verbatim; the model adds embedding similarity beside it.
//! For selection, candidate labels are ranked by cosine similarity to the
//! goal. For assessment, the retained text is cut by an explicit, bounded
//! chunker, the best chunk's similarity is combined with S1's coverage by an
//! explicit rule, and the result is the verdict. Every step records both
//! answers, which won, and the embedding envelope in force.
//!
//! The core is feature-free: it needs only a [`GoalEmbedder`]. When the
//! embedder errors, returns a malformed answer or has nothing to embed, that
//! step falls back to S1's answer and the record says the model was
//! unavailable there; with no embedder at all, use [`GoalAssessor`] directly.
//! Scores are integer thousandths of cosine so records are `Eq` and stable.

use crate::goal::{
    CandidateLabel, GoalAssessment, GoalAssessor, GoalRecord, GoalSettings, GoalVerdict,
    SelectionChoice,
};
use crate::query::QueryError;
use eidetic::models::embedding::EmbeddingEnvelopeV1;
use eidetic::models::recall::{RecallSourceId, RetainedSourceText};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Operation identity recorded with every [`ModelGoalRecord`].
pub const GOAL_MODEL_OPERATION_VERSION: &str = "eidetic.goal-model/v1";

/// Which side of the comparison a text is on. Asymmetric models declare
/// different literal prefixes for the goal and for the text it is held
/// against (`EmbeddingSpaceV1::query_prefix` / `document_prefix`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EmbedRole {
    /// The goal.
    Query,
    /// Retained text chunks and candidate labels.
    Document,
}

/// A text embedder as the goal path needs it. Implementations own batching,
/// device and weights; the envelope's space must describe exactly the vectors
/// `embed` returns (its corpus is replaced per run by the assessor).
pub trait GoalEmbedder {
    /// The loaded model's identity, as an envelope. Only `space` is used.
    fn envelope(&self) -> EmbeddingEnvelopeV1;
    /// One vector per text, in order, each `envelope().space.dimensions` long.
    ///
    /// # Errors
    /// A message when the model could not answer this batch.
    fn embed(&mut self, texts: &[&str], role: EmbedRole) -> Result<Vec<Vec<f32>>, String>;
}

/// Who picks among pending candidates when both S1 and the model answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModelSelectMode {
    /// The model's top-scoring label wins (if it clears the floor); S1's
    /// choice is recorded beside it.
    ModelRanks,
    /// S1 wins when its term-overlap rule fired; the model decides only when
    /// S1 fell back to the oldest candidate.
    S1Overlap,
}

/// How S1 coverage and model similarity combine into a match.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModelAssessRule {
    /// Match if S1 matches OR similarity reaches the threshold.
    Or,
    /// Similarity alone decides; S1's assessment is recorded, not used.
    ModelReplaces,
    /// S1 alone decides; the model's similarity is recorded for study.
    RecordOnly,
}

/// Model-path settings; the S1 settings stay in the wrapped record.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelGoalSettings {
    pub select_mode: ModelSelectMode,
    pub assess_rule: ModelAssessRule,
    /// Cosine, in thousandths, at or above which text counts as serving the
    /// goal. Model-dependent: calibrate per embedding space.
    pub match_similarity_milli: i16,
    /// Lowest cosine, in thousandths, at which the model may pick a candidate.
    pub min_select_similarity_milli: i16,
    /// Characters per chunk.
    pub max_chunk_chars: usize,
    /// Characters shared by neighbouring chunks; less than `max_chunk_chars`.
    pub chunk_overlap_chars: usize,
    /// Chunks embedded per source (a present title takes one); text beyond
    /// them is left unscored and the assessment says so.
    pub max_chunks: usize,
}

impl Default for ModelGoalSettings {
    fn default() -> Self {
        Self {
            select_mode: ModelSelectMode::ModelRanks,
            assess_rule: ModelAssessRule::Or,
            match_similarity_milli: 500,
            min_select_similarity_milli: -1000,
            max_chunk_chars: 1000,
            chunk_overlap_chars: 100,
            max_chunks: 16,
        }
    }
}

impl ModelGoalSettings {
    fn validate(&self) -> Result<(), QueryError> {
        let ok = |m: i16| (-1000..=1000).contains(&m);
        if self.max_chunk_chars == 0
            || self.chunk_overlap_chars >= self.max_chunk_chars
            || self.max_chunks == 0
            || !ok(self.match_similarity_milli)
            || !ok(self.min_select_similarity_milli)
        {
            return Err("invalid model goal settings".into());
        }
        Ok(())
    }

    /// The chunker's identity, bound into the envelope's corpus.
    pub fn chunker_id(&self) -> String {
        format!(
            "eidetic.goal-chunk/v1/chars={}/overlap={}/max={}",
            self.max_chunk_chars, self.chunk_overlap_chars, self.max_chunks
        )
    }
}

/// Whether the model answered a step.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModelStatus {
    Used,
    /// The step ran on S1's answer alone.
    Unavailable {
        reason: String,
    },
}

/// Whose pick was taken for a selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SelectionWinner {
    S1,
    Model,
}

/// One selection: S1's choice, the model's scores, and the pick taken.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelSelection {
    pub s1: SelectionChoice,
    /// Per candidate, cosine to the goal in thousandths; `None` where the
    /// candidate had no usable label or the model was unavailable.
    pub scores_milli: Vec<Option<i16>>,
    pub model_pick: Option<usize>,
    pub chosen: usize,
    pub winner: SelectionWinner,
    pub status: ModelStatus,
}

/// One assessment: S1's answer, the model's score, and the combined verdict.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelAssessment {
    pub source: RecallSourceId,
    pub version: String,
    pub text_hash: eidetic::Hash,
    pub s1_verdict: GoalVerdict,
    pub s1_coverage_permille: u16,
    /// Best chunk's cosine to the goal, in thousandths.
    pub similarity_milli: Option<i16>,
    /// Index of that chunk among those embedded (a title, if any, is first).
    pub best_chunk: Option<usize>,
    pub chunks_embedded: usize,
    /// Whether text beyond the chunk bound was left unscored.
    pub text_truncated: bool,
    pub s1_match: bool,
    pub model_match: Option<bool>,
    pub rule: ModelAssessRule,
    pub status: ModelStatus,
    pub verdict: GoalVerdict,
}

/// The citable account of a model-assisted goal run.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ModelGoalRecord {
    pub operation_version: String,
    /// S1's complete record, unchanged by the model.
    pub s1: GoalRecord,
    /// The embedder's space with the run's chunker and the retained versions
    /// assessed (sorted by source, the latest version per source).
    pub envelope: EmbeddingEnvelopeV1,
    pub settings: ModelGoalSettings,
    pub selections: Vec<ModelSelection>,
    pub assessments: Vec<ModelAssessment>,
    /// Matches under the combined rule.
    pub matched: usize,
}

impl ModelGoalRecord {
    /// Refuse an `expected` envelope from a foreign space or chunker, or one
    /// whose corpus differs; delegates to `ensure_compatible`.
    ///
    /// # Errors
    /// The refusal reason from the envelope check.
    pub fn ensure_compatible(&self, expected: &EmbeddingEnvelopeV1) -> Result<(), String> {
        self.envelope.ensure_compatible(expected)
    }

    /// As [`Self::ensure_compatible`] but ignoring which sources each run saw,
    /// the check for "same model and chunker across runs".
    ///
    /// # Errors
    /// The refusal reason from the envelope check.
    pub fn ensure_compatible_space(&self, other: &EmbeddingEnvelopeV1) -> Result<(), String> {
        let mut other = other.clone();
        other.corpus.sources = self.envelope.corpus.sources.clone();
        self.envelope.ensure_compatible(&other)
    }
}

/// S1 plus a model; see the module docs.
pub struct ModelGoalAssessor<E: GoalEmbedder> {
    inner: GoalAssessor,
    embedder: E,
    settings: ModelGoalSettings,
    space: EmbeddingEnvelopeV1,
    goal_vector: Option<Vec<f32>>,
    selections: Vec<ModelSelection>,
    assessments: Vec<ModelAssessment>,
    matched: usize,
}

impl<E: GoalEmbedder> ModelGoalAssessor<E> {
    /// Refuses what [`GoalAssessor::new`] refuses, unusable model settings and
    /// an embedder whose envelope does not validate.
    ///
    /// # Errors
    /// The refusal reason.
    pub fn new(
        goal: &str,
        s1: GoalSettings,
        settings: ModelGoalSettings,
        embedder: E,
    ) -> Result<Self, QueryError> {
        settings.validate()?;
        let inner = GoalAssessor::new(goal, s1)?;
        let space = embedder.envelope();
        space.validate()?;
        Ok(Self {
            inner,
            embedder,
            settings,
            space,
            goal_vector: None,
            selections: Vec::new(),
            assessments: Vec::new(),
            matched: 0,
        })
    }

    /// The wrapped S1 assessor.
    pub fn s1(&self) -> &GoalAssessor {
        &self.inner
    }

    /// Choose among pending candidates; see [`ModelSelectMode`]. S1 always
    /// runs, so the choice exists even when the model is unavailable.
    pub fn select(&mut self, candidates: &[CandidateLabel]) -> Option<usize> {
        self.inner.select(candidates)?;
        let s1 = self.inner.last_selection()?.clone();
        let max = self.inner.settings().max_text_chars;
        let usable: Vec<(usize, &str)> = candidates
            .iter()
            .enumerate()
            .filter_map(|(i, c)| Some((i, c.label.as_deref()?)))
            .filter(|(_, l)| l.chars().count() <= max)
            .collect();
        let mut scores_milli = vec![None; candidates.len()];
        let texts: Vec<&str> = usable.iter().map(|(_, l)| *l).collect();
        let status = match self.score(&texts) {
            Ok(scores) => {
                for ((i, _), s) in usable.iter().zip(scores) {
                    scores_milli[*i] = Some(s);
                }
                ModelStatus::Used
            }
            Err(reason) => ModelStatus::Unavailable { reason },
        };
        // First maximum wins, so ties go to the oldest candidate.
        let model_pick = scores_milli
            .iter()
            .enumerate()
            .filter_map(|(i, s)| Some((i, (*s)?)))
            .fold(None::<(usize, i16)>, |best, cur| match best {
                Some(b) if b.1 >= cur.1 => Some(b),
                _ => Some(cur),
            })
            .filter(|(_, s)| *s >= self.settings.min_select_similarity_milli)
            .map(|(i, _)| i);
        let s1_fired = s1.rule == crate::goal::SelectionRule::GoalTermOverlap;
        let take_model = model_pick.is_some()
            && match self.settings.select_mode {
                ModelSelectMode::ModelRanks => true,
                ModelSelectMode::S1Overlap => !s1_fired,
            };
        let (chosen, winner) = match model_pick {
            Some(i) if take_model => (i, SelectionWinner::Model),
            _ => (s1.index, SelectionWinner::S1),
        };
        self.selections.push(ModelSelection {
            s1,
            scores_milli,
            model_pick,
            chosen,
            winner,
            status,
        });
        Some(chosen)
    }

    /// Assess one retained source under the configured rule. S1's assessment
    /// is always made and recorded; an S1-refused input records nothing.
    ///
    /// # Errors
    /// S1's refusal of the input (oversized text).
    pub fn assess(&mut self, retained: &RetainedSourceText) -> Result<GoalVerdict, QueryError> {
        let s1_verdict = self.inner.assess(retained)?;
        let s1: GoalAssessment = self
            .inner
            .last_assessment()
            .ok_or("S1 recorded no assessment")?
            .clone();
        let s1_match = s1_verdict != GoalVerdict::Continue;
        let (chunks, text_truncated) =
            chunk_text(retained.title.as_deref(), &retained.text, &self.settings);
        let scored = if chunks.is_empty() {
            Err("nothing to embed".to_string())
        } else {
            self.score(&chunks)
        };
        let (status, similarity, best_chunk) = match scored {
            Ok(scores) => {
                let (i, s) = scores
                    .iter()
                    .copied()
                    .enumerate()
                    .fold((0, i16::MIN), |b, c| if c.1 > b.1 { c } else { b });
                (ModelStatus::Used, Some(s), Some(i))
            }
            Err(reason) => (ModelStatus::Unavailable { reason }, None, None),
        };
        let model_match = similarity.map(|s| s >= self.settings.match_similarity_milli);
        let rule = self.settings.assess_rule;
        let is_match = match (model_match, rule) {
            (None, _) | (_, ModelAssessRule::RecordOnly) => s1_match,
            (Some(m), ModelAssessRule::Or) => s1_match || m,
            (Some(m), ModelAssessRule::ModelReplaces) => m,
        };
        let verdict = if !is_match {
            GoalVerdict::Continue
        } else {
            self.matched += 1;
            if self.matched >= self.inner.settings().satisfy_after {
                GoalVerdict::Satisfied
            } else {
                GoalVerdict::Match
            }
        };
        self.assessments.push(ModelAssessment {
            source: s1.source,
            version: s1.version,
            text_hash: s1.text_hash,
            s1_verdict,
            s1_coverage_permille: s1.coverage_permille,
            similarity_milli: similarity,
            best_chunk,
            chunks_embedded: chunks.len(),
            text_truncated,
            s1_match,
            model_match,
            rule,
            status,
            verdict,
        });
        Ok(verdict)
    }

    /// The citable account: S1's record unchanged, plus the model's.
    pub fn record(&self) -> ModelGoalRecord {
        let mut latest: BTreeMap<&RecallSourceId, (&str, eidetic::Hash)> = BTreeMap::new();
        for a in &self.assessments {
            latest.insert(&a.source, (&a.version, a.text_hash));
        }
        let mut envelope = self.space.clone();
        envelope.corpus.chunker = self.settings.chunker_id();
        envelope.corpus.sources = latest
            .into_iter()
            .map(|(source, (version, text_hash))| {
                eidetic::models::embedding::EmbeddingSourceRevisionV1 {
                    source: source.clone(),
                    version: version.to_owned(),
                    text_hash,
                }
            })
            .collect();
        ModelGoalRecord {
            operation_version: GOAL_MODEL_OPERATION_VERSION.into(),
            s1: self.inner.record(),
            envelope,
            settings: self.settings.clone(),
            selections: self.selections.clone(),
            assessments: self.assessments.clone(),
            matched: self.matched,
        }
    }

    /// Cosine of each text to the goal, in thousandths. Any malformed answer
    /// fails the whole step so a partial score never mixes with S1.
    fn score(&mut self, texts: &[&str]) -> Result<Vec<i16>, String> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        if self.goal_vector.is_none() {
            let goal = self.inner.goal().to_owned();
            let mut v = self.embed_checked(&[goal.as_str()], EmbedRole::Query)?;
            self.goal_vector = v.pop();
        }
        let vectors = self.embed_checked(texts, EmbedRole::Document)?;
        let goal = self.goal_vector.as_deref().ok_or("goal vector missing")?;
        Ok(vectors.iter().map(|v| cosine_milli(goal, v)).collect())
    }

    fn embed_checked(&mut self, texts: &[&str], role: EmbedRole) -> Result<Vec<Vec<f32>>, String> {
        let dims = self.space.space.dimensions;
        let out = self.embedder.embed(texts, role)?;
        let sound = out.len() == texts.len()
            && out.iter().all(|v| {
                v.len() == dims && v.iter().all(|x| x.is_finite()) && v.iter().any(|x| *x != 0.0)
            });
        if sound {
            Ok(out)
        } else {
            Err("embedder returned a malformed batch".into())
        }
    }
}

fn cosine_milli(a: &[f32], b: &[f32]) -> i16 {
    let dot: f64 = a
        .iter()
        .zip(b)
        .map(|(x, y)| f64::from(*x) * f64::from(*y))
        .sum();
    let norm = |v: &[f32]| v.iter().map(|x| f64::from(*x).powi(2)).sum::<f64>().sqrt();
    let c = (dot / (norm(a) * norm(b))).clamp(-1.0, 1.0);
    // Clamped to [-1, 1], so the product fits i16.
    (c * 1000.0).round() as i16
}

/// Byte offset `n` chars past `from`, or the end.
fn advance(s: &str, from: usize, n: usize) -> usize {
    s[from..]
        .char_indices()
        .nth(n)
        .map_or(s.len(), |(i, _)| from + i)
}

/// The goal path's chunker: the title (cut to one chunk) first, then
/// fixed-size character windows with overlap, at most `max_chunks` in all.
/// Returns the chunks and whether text beyond the bound was left out.
/// Identified by [`ModelGoalSettings::chunker_id`].
pub fn chunk_text<'a>(
    title: Option<&'a str>,
    text: &'a str,
    settings: &ModelGoalSettings,
) -> (Vec<&'a str>, bool) {
    let mut chunks = Vec::new();
    if let Some(t) = title.filter(|t| !t.trim().is_empty()) {
        chunks.push(&t[..advance(t, 0, settings.max_chunk_chars)]);
    }
    let step = settings.max_chunk_chars - settings.chunk_overlap_chars;
    let mut start = 0;
    while start < text.len() {
        if chunks.len() >= settings.max_chunks {
            return (chunks, true);
        }
        let end = advance(text, start, settings.max_chunk_chars);
        if !text[start..end].trim().is_empty() {
            chunks.push(&text[start..end]);
        }
        if end == text.len() {
            break;
        }
        start = advance(text, start, step);
    }
    (chunks, false)
}

#[cfg(test)]
#[path = "goal_model_tests.rs"]
mod tests;
