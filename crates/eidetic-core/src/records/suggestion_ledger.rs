// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. SPDX-License-Identifier: MPL-2.0
//! Durable, append-only ledger of tag suggestions and their reviews.
//!
//! Storage is one Muniment [`Journal`] of [`LedgerEntry`] values persisted one
//! entry per key under [`LEDGER_PREFIX`] (`entries::append_entries`), so an
//! append is one small write and the log reloads in order. Suggestions and
//! reviews share the log; a review always follows the suggestion it names, and
//! the ledger refuses a review whose suggestion it does not hold. Nothing is
//! ever rewritten: regeneration is a new suggestion id, rejection is a new
//! review, and personal tag assertions are never touched here.
//!
//! # Keeping tiers
//!
//! A suggestion is queryable at its declared [`KeepingTier`]:
//!
//! - `Runtime`: held in memory for this ledger handle only. It and its reviews
//!   are never written, so they are gone after a reopen.
//! - `Rebuildable` and `Retained`: journaled, so they and their reviews survive
//!   reopening. `Rebuildable` entries stay flagged by their own tier so a host
//!   may purge and regenerate them; the ledger never deletes anything.
//!
//! A write that fails in the store leaves the entry in memory and returns the
//! error; the next ledger write (or [`SuggestionLedger::flush`]) retries it.
//! One writer per prefix is assumed.
use super::ResearchError;
use super::suggestions::{
    KeepingTier, PersonalTagAssertionDraftV1, SuggestionFilter, SuggestionView,
    TagSuggestionReviewV1, TagSuggestionV1, accept, reject, suggestions_for, validate_review,
    validate_suggestion,
};
use crate::Hash;
use crate::models::recall::RecallSourceId;
use muniment::{Backend, Journal, JsonSlots, Seq, StoreError};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Key prefix of the ledger's journal entries inside the backend.
pub const LEDGER_PREFIX: &str = "eidetic/tag-suggestions";

/// One journaled ledger entry.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LedgerEntry {
    Suggestion(Box<TagSuggestionV1>),
    Review(TagSuggestionReviewV1),
}

#[derive(Debug)]
pub enum LedgerError {
    Store(StoreError),
    Invalid(ResearchError),
    /// A review or decision named a suggestion this ledger does not hold.
    UnknownSuggestion(Hash),
    /// The stored journal does not replay into a consistent ledger.
    Corrupt(String),
}
impl From<StoreError> for LedgerError {
    fn from(e: StoreError) -> Self {
        Self::Store(e)
    }
}
impl From<ResearchError> for LedgerError {
    fn from(e: ResearchError) -> Self {
        Self::Invalid(e)
    }
}

/// Whether a recorded suggestion was new.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Recorded {
    New,
    Existing,
}

pub struct SuggestionLedger<B: Backend> {
    slots: JsonSlots<B>,
    journal: Journal<LedgerEntry>,
    /// Journal entries below this are in the store.
    flushed: Seq,
    suggestions: Vec<TagSuggestionV1>,
    reviews: Vec<TagSuggestionReviewV1>,
    tiers: HashMap<Hash, KeepingTier>,
}

impl<B: Backend> SuggestionLedger<B> {
    /// Open over `backend`, replaying whatever the journal holds. An empty
    /// backend yields an empty ledger.
    ///
    /// # Errors
    /// A store failure, or a journal that does not replay consistently.
    pub async fn open(backend: B) -> Result<Self, LedgerError> {
        let slots = JsonSlots::new(backend);
        let journal =
            Journal::<LedgerEntry>::load_entries(&slots, LEDGER_PREFIX, None, None).await?;
        let mut ledger = Self {
            flushed: journal.next_seq(),
            slots,
            journal: Journal::new(),
            suggestions: Vec::new(),
            reviews: Vec::new(),
            tiers: HashMap::new(),
        };
        for entry in journal.entries() {
            match entry {
                LedgerEntry::Suggestion(s) => {
                    validate_suggestion(s).map_err(|e| LedgerError::Corrupt(format!("{e:?}")))?;
                    ledger.tiers.insert(s.id(), s.keeping);
                    ledger.suggestions.push((**s).clone());
                }
                LedgerEntry::Review(r) => {
                    if !ledger.tiers.contains_key(&r.suggestion) {
                        return Err(LedgerError::Corrupt(format!(
                            "review names unknown suggestion {}",
                            r.suggestion
                        )));
                    }
                    ledger.reviews.push(r.clone());
                }
            }
        }
        ledger.journal = journal;
        Ok(ledger)
    }

    /// Record a suggestion; idempotent by id. Returns its id.
    ///
    /// # Errors
    /// An invalid suggestion or a store failure.
    pub async fn record_suggestion(
        &mut self,
        suggestion: TagSuggestionV1,
    ) -> Result<(Hash, Recorded), LedgerError> {
        validate_suggestion(&suggestion)?;
        let id = suggestion.id();
        if self.tiers.contains_key(&id) {
            self.flush().await?;
            return Ok((id, Recorded::Existing));
        }
        self.tiers.insert(id, suggestion.keeping);
        self.suggestions.push(suggestion.clone());
        if suggestion.keeping != KeepingTier::Runtime {
            self.journal
                .append(LedgerEntry::Suggestion(Box::new(suggestion)));
        }
        self.flush().await?;
        Ok((id, Recorded::New))
    }

    /// Append a review. Refused when its suggestion is unknown. Reviews of
    /// `Runtime` suggestions are kept in memory with them.
    ///
    /// # Errors
    /// An invalid review, an unknown suggestion, or a store failure.
    pub async fn record_review(
        &mut self,
        review: TagSuggestionReviewV1,
    ) -> Result<(), LedgerError> {
        validate_review(&review)?;
        let Some(tier) = self.tiers.get(&review.suggestion).copied() else {
            return Err(LedgerError::UnknownSuggestion(review.suggestion));
        };
        self.reviews.push(review.clone());
        if tier != KeepingTier::Runtime {
            self.journal.append(LedgerEntry::Review(review));
        }
        self.flush().await
    }

    /// Accept a held suggestion: records the review and returns the draft
    /// assertion citing it, for the owning application to admit.
    ///
    /// # Errors
    /// An unknown suggestion or a failure recording the review.
    pub async fn accept_suggestion(
        &mut self,
        id: &Hash,
        reviewer: &str,
        at_unix_micros: i64,
    ) -> Result<PersonalTagAssertionDraftV1, LedgerError> {
        let s = self.get(id).ok_or(LedgerError::UnknownSuggestion(*id))?;
        let (review, draft) = accept(s, reviewer, at_unix_micros);
        self.record_review(review).await?;
        Ok(draft)
    }

    /// Reject a held suggestion: a review only.
    ///
    /// # Errors
    /// An unknown suggestion or a failure recording the review.
    pub async fn reject_suggestion(
        &mut self,
        id: &Hash,
        reviewer: &str,
        at_unix_micros: i64,
        reason: Option<String>,
    ) -> Result<(), LedgerError> {
        let s = self.get(id).ok_or(LedgerError::UnknownSuggestion(*id))?;
        let review = reject(s, reviewer, at_unix_micros, reason);
        self.record_review(review).await
    }

    /// Write any journal entries the store has not accepted yet.
    ///
    /// # Errors
    /// A store failure; the entries stay pending for the next flush.
    pub async fn flush(&mut self) -> Result<(), LedgerError> {
        self.journal
            .append_entries(&self.slots, LEDGER_PREFIX, self.flushed)
            .await?;
        self.flushed = self.journal.next_seq();
        Ok(())
    }

    pub fn get(&self, id: &Hash) -> Option<&TagSuggestionV1> {
        self.tiers
            .contains_key(id)
            .then(|| self.suggestions.iter().find(|s| &s.id() == id))
            .flatten()
    }
    /// Every suggestion held, in recording order (persisted first on reopen).
    pub fn suggestions(&self) -> &[TagSuggestionV1] {
        &self.suggestions
    }
    /// Every review held, in recording order.
    pub fn reviews(&self) -> &[TagSuggestionReviewV1] {
        &self.reviews
    }
    /// Suggestions for `subject` with review state; see [`suggestions_for`].
    pub fn suggestions_for<'a>(
        &'a self,
        subject: &RecallSourceId,
        filter: &SuggestionFilter,
    ) -> Vec<SuggestionView<'a>> {
        suggestions_for(subject, &self.suggestions, &self.reviews, filter)
    }
}

#[cfg(test)]
#[path = "suggestion_ledger_tests.rs"]
mod tests;
