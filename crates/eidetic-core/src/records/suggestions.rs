// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. SPDX-License-Identifier: MPL-2.0
//! Generated tag suggestions, kept outside asserted personal tags.
//!
//! A suggestion is an immutable, content-addressed record of what a producer
//! proposed over one retained input revision. Review is a separate append-only
//! record, never a mutation of the suggestion. Accepting yields a draft that the
//! owning application may admit through its own attributed funnel; Eidetic does
//! not write personal tags, and nothing here grants access or touches storage.
use super::{ResearchError, TagIdentity};
use crate::Hash;
use crate::models::recall::{PassageSelector, RecallSourceId};
use serde::{Deserialize, Serialize};

/// Identity-owned tag concept reference: owner plus concept id. Two concepts
/// with equal concept strings but different owners are distinct.
pub type TagRef = TagIdentity;

/// Declared lifecycle at which a suggestion stays queryable. Separate from
/// personal tag assertion and from clip promotion.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeepingTier {
    /// A runtime observation that need not survive the session.
    Runtime,
    /// A rebuildable index entry; may be regenerated from its inputs.
    Rebuildable,
    /// Kept with retained clips and notes; survives reopening.
    Retained,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub enum SuggestionProducerKind {
    Lexical,
    Model { model: String, revision: String },
    Rule { name: String },
}
/// Payload-free producer class, for filtering.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProducerClass {
    Lexical,
    Model,
    Rule,
}
impl SuggestionProducerKind {
    pub fn class(&self) -> ProducerClass {
        match self {
            Self::Lexical => ProducerClass::Lexical,
            Self::Model { .. } => ProducerClass::Model,
            Self::Rule { .. } => ProducerClass::Rule,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SuggestionProducer {
    pub kind: SuggestionProducerKind,
    pub operation_version: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TagSuggestionV1 {
    pub subject: RecallSourceId,
    /// The retained input revision the producer read.
    pub subject_version: String,
    pub input_text_hash: Hash,
    pub concept: TagRef,
    pub producer: SuggestionProducer,
    /// Run id; a regeneration carries a new one.
    pub run: String,
    /// BLAKE3 of the canonical settings bytes, supplied by the caller.
    pub settings_digest: Hash,
    pub evidence: Vec<PassageSelector>,
    pub score: Option<f32>,
    pub keeping: KeepingTier,
}
impl TagSuggestionV1 {
    /// Deterministic id over the canonical serialization of every field, so a
    /// regeneration with a new run is a new suggestion and overwrites nothing.
    pub fn id(&self) -> Hash {
        let mut bytes = b"eidetic.TagSuggestion/v1\0".to_vec();
        bytes.extend(serde_json::to_vec(self).expect("suggestion serializes"));
        Hash::of(&bytes)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub enum ReviewDecision {
    Accepted,
    Rejected { reason: Option<String> },
}
/// Append-only review of one suggestion.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TagSuggestionReviewV1 {
    pub suggestion: Hash,
    pub decision: ReviewDecision,
    pub reviewer: String,
    pub at_unix_micros: i64,
}
/// What the owning application admits through its own attributed funnel.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PersonalTagAssertionDraftV1 {
    pub subject: RecallSourceId,
    pub subject_version: String,
    pub concept: TagRef,
    pub asserter: String,
    pub cites_suggestion: Hash,
}

/// Accept: a review record plus the draft assertion citing the suggestion.
pub fn accept(
    suggestion: &TagSuggestionV1,
    reviewer: &str,
    at_unix_micros: i64,
) -> (TagSuggestionReviewV1, PersonalTagAssertionDraftV1) {
    let id = suggestion.id();
    (
        TagSuggestionReviewV1 {
            suggestion: id,
            decision: ReviewDecision::Accepted,
            reviewer: reviewer.into(),
            at_unix_micros,
        },
        PersonalTagAssertionDraftV1 {
            subject: suggestion.subject.clone(),
            subject_version: suggestion.subject_version.clone(),
            concept: suggestion.concept.clone(),
            asserter: reviewer.into(),
            cites_suggestion: id,
        },
    )
}
/// Reject: a review record only. Never produces or retracts an assertion.
pub fn reject(
    suggestion: &TagSuggestionV1,
    reviewer: &str,
    at_unix_micros: i64,
    reason: Option<String>,
) -> TagSuggestionReviewV1 {
    TagSuggestionReviewV1 {
        suggestion: suggestion.id(),
        decision: ReviewDecision::Rejected { reason },
        reviewer: reviewer.into(),
        at_unix_micros,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReviewState {
    Unreviewed,
    Accepted {
        reviewer: String,
        at_unix_micros: i64,
    },
    Rejected {
        reviewer: String,
        at_unix_micros: i64,
        reason: Option<String>,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewStateKind {
    Unreviewed,
    Accepted,
    Rejected,
}
impl ReviewState {
    pub fn kind(&self) -> ReviewStateKind {
        match self {
            Self::Unreviewed => ReviewStateKind::Unreviewed,
            Self::Accepted { .. } => ReviewStateKind::Accepted,
            Self::Rejected { .. } => ReviewStateKind::Rejected,
        }
    }
}
/// Current review state of one suggestion among arbitrary reviews.
///
/// Reviews for other suggestions are ignored. The latest review by
/// `at_unix_micros` wins. Ties break deterministically, independent of input
/// order: a rejection outranks an acceptance, then the greater reviewer string,
/// then the greater rejection reason (`None` < `Some`). A later rejection after
/// an acceptance changes only this state; it does not retract a personal
/// assertion already admitted from the accepted draft.
pub fn review_state(suggestion_id: &Hash, reviews: &[TagSuggestionReviewV1]) -> ReviewState {
    let key = |r: &TagSuggestionReviewV1| {
        let (rank, reason) = match &r.decision {
            ReviewDecision::Accepted => (0u8, None),
            ReviewDecision::Rejected { reason } => (1, reason.as_ref()),
        };
        (r.at_unix_micros, rank, r.reviewer.clone(), reason.cloned())
    };
    let Some(r) = reviews
        .iter()
        .filter(|r| &r.suggestion == suggestion_id)
        .max_by_key(|r| key(r))
    else {
        return ReviewState::Unreviewed;
    };
    match &r.decision {
        ReviewDecision::Accepted => ReviewState::Accepted {
            reviewer: r.reviewer.clone(),
            at_unix_micros: r.at_unix_micros,
        },
        ReviewDecision::Rejected { reason } => ReviewState::Rejected {
            reviewer: r.reviewer.clone(),
            at_unix_micros: r.at_unix_micros,
            reason: reason.clone(),
        },
    }
}

/// Query filter; every `Some` field must match.
#[derive(Clone, Debug, Default)]
pub struct SuggestionFilter {
    pub subject_version: Option<String>,
    pub concept: Option<TagRef>,
    pub producer: Option<ProducerClass>,
    pub run: Option<String>,
    pub review: Option<ReviewStateKind>,
    pub keeping: Option<KeepingTier>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct SuggestionView<'a> {
    pub id: Hash,
    pub suggestion: &'a TagSuggestionV1,
    pub review: ReviewState,
}
/// Suggestions for `subject` with their review state, in input order. Reads
/// suggestion and review records only; personal tags are never consulted.
pub fn suggestions_for<'a>(
    subject: &RecallSourceId,
    suggestions: &'a [TagSuggestionV1],
    reviews: &[TagSuggestionReviewV1],
    filter: &SuggestionFilter,
) -> Vec<SuggestionView<'a>> {
    suggestions
        .iter()
        .filter(|s| &s.subject == subject)
        .filter(|s| {
            filter
                .subject_version
                .as_ref()
                .is_none_or(|v| v == &s.subject_version)
        })
        .filter(|s| filter.concept.as_ref().is_none_or(|c| c == &s.concept))
        .filter(|s| filter.producer.is_none_or(|p| p == s.producer.kind.class()))
        .filter(|s| filter.run.as_ref().is_none_or(|r| r == &s.run))
        .filter(|s| filter.keeping.is_none_or(|k| k == s.keeping))
        .filter_map(|s| {
            let id = s.id();
            let review = review_state(&id, reviews);
            filter
                .review
                .is_none_or(|k| k == review.kind())
                .then_some(SuggestionView {
                    id,
                    suggestion: s,
                    review,
                })
        })
        .collect()
}

fn nonempty(s: &str) -> bool {
    !s.trim().is_empty()
}
fn invalid(s: &str) -> ResearchError {
    ResearchError::Invalid(s.into())
}
/// Intrinsic checks; hosts also validate the outer schema.
pub fn validate_suggestion(s: &TagSuggestionV1) -> Result<(), ResearchError> {
    if !nonempty(&s.subject.source) || !nonempty(&s.subject_version) {
        return Err(invalid("suggestion subject or input revision empty"));
    }
    if !nonempty(&s.concept.owner) || !nonempty(&s.concept.concept) {
        return Err(invalid("suggestion concept owner or id empty"));
    }
    if !nonempty(&s.run) || !nonempty(&s.producer.operation_version) {
        return Err(invalid("suggestion run or operation version empty"));
    }
    let producer_ok = match &s.producer.kind {
        SuggestionProducerKind::Lexical => true,
        SuggestionProducerKind::Model { model, revision } => nonempty(model) && nonempty(revision),
        SuggestionProducerKind::Rule { name } => nonempty(name),
    };
    if !producer_ok {
        return Err(invalid("suggestion producer identity empty"));
    }
    if s.score
        .is_some_and(|x| !x.is_finite() || !(0.0..=1.0).contains(&x))
    {
        return Err(invalid("suggestion score outside [0,1]"));
    }
    if s.evidence.iter().any(|e| e.start >= e.end) {
        return Err(invalid("suggestion evidence selector empty or inverted"));
    }
    Ok(())
}
/// Validate a review: non-empty reviewer and suggestion-independent shape.
pub fn validate_review(r: &TagSuggestionReviewV1) -> Result<(), ResearchError> {
    if !nonempty(&r.reviewer) {
        return Err(invalid("review reviewer empty"));
    }
    Ok(())
}
/// Validate an assertion draft.
pub fn validate_assertion_draft(d: &PersonalTagAssertionDraftV1) -> Result<(), ResearchError> {
    if !nonempty(&d.subject.source)
        || !nonempty(&d.subject_version)
        || !nonempty(&d.concept.owner)
        || !nonempty(&d.concept.concept)
        || !nonempty(&d.asserter)
    {
        return Err(invalid("assertion draft has an empty identity field"));
    }
    Ok(())
}

#[cfg(test)]
#[path = "suggestions_tests.rs"]
mod tests;
