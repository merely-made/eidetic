// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! A dictionary and a quad index over dense ids, generic over the term type.
//!
//! This is the store half of a query engine's dataset: every term a quad uses
//! is interned once in a [`Dictionary`] and named by a [`TermId`], and the
//! quads are held as id tuples in a [`QuadIndex`] sorted six ways (SPO, POS,
//! OSP and their graph-first twins), so any pattern that binds some of
//! subject, predicate, object and graph reads one contiguous range. The module
//! never learns what a term is: a consumer supplies the terms (Mere's engine
//! interns `oxrdf` terms and implements its SPARQL evaluator's dataset seam
//! over this index), and [`crate::rdf::Term`] works as well.
//!
//! The shape was ruled in Mere's indexed dataset brief
//! (`mere/design_docs/mere_docs/technical_architecture/2026-11-09_indexed_dataset_brief.md`,
//! D1 to D5) and its quad index plan
//! (`mere/design_docs/mere_docs/implementation_strategy/2026-11-09_quad_index_plan.md`,
//! D1 to D7):
//!
//! - **Dense ids per rebuild.** A [`TermId`] is minted when a dictionary is
//!   built and lives as long as it. Nothing of it is persisted; durable
//!   identity stays with whatever the consumer's terms name. The dictionary
//!   and the index are append-only and rebuilt per revision, never mutated in
//!   place.
//! - **One index, two views.** Held and withdrawn rows sit in one index;
//!   [`View::Live`] reads only the held rows and [`View::History`] reads them
//!   all.
//! - **Ordinals into one base table.** The rows are stored once, sorted SPO
//!   then graph, and the five other orderings are `u32` positions into them.
//! - **Every (quad, meta) row is kept, each quad yielded once.** The same quad
//!   can arrive with several metas (two asserters, or one held and one
//!   withdrawn row); a pattern yields it once per view, its held row first.

mod dictionary;
mod index;
#[cfg(test)]
mod tests;

use std::num::NonZeroU32;

pub use dictionary::Dictionary;
pub use index::QuadIndex;

/// A dense term id minted by a [`Dictionary`].
///
/// Ids start at 1 and are never zero, so `Option<TermId>` is four bytes. The
/// value `u32::MAX` is reserved as [`TermId::DEFAULT_GRAPH`] and is never
/// minted.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TermId(NonZeroU32);

impl TermId {
    /// The default graph's id. No term stands behind it: a quad in the default
    /// graph carries it as its graph, and no dictionary mints or resolves it.
    pub const DEFAULT_GRAPH: TermId = TermId(NonZeroU32::MAX);

    /// The id with this raw value, or `None` for zero. Any value is accepted,
    /// so a consumer can name an id no dictionary minted (one that matches no
    /// row, for a term the dictionary lacks); resolving it panics.
    #[must_use]
    pub const fn from_u32(raw: u32) -> Option<TermId> {
        match NonZeroU32::new(raw) {
            Some(raw) => Some(TermId(raw)),
            None => None,
        }
    }

    /// The raw value.
    #[must_use]
    pub const fn as_u32(self) -> u32 {
        self.0.get()
    }

    /// Whether this is [`TermId::DEFAULT_GRAPH`].
    #[must_use]
    pub const fn is_default_graph(self) -> bool {
        self.0.get() == u32::MAX
    }
}

/// A quad as four term ids. The default graph is [`TermId::DEFAULT_GRAPH`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Quad {
    /// The subject.
    pub subject: TermId,
    /// The predicate.
    pub predicate: TermId,
    /// The object.
    pub object: TermId,
    /// The graph, or [`TermId::DEFAULT_GRAPH`].
    pub graph: TermId,
}

impl Quad {
    /// A quad in the default graph.
    #[must_use]
    pub const fn in_default_graph(subject: TermId, predicate: TermId, object: TermId) -> Quad {
        Quad {
            subject,
            predicate,
            object,
            graph: TermId::DEFAULT_GRAPH,
        }
    }
}

/// What a filter needs beside the quad itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct QuadMeta {
    /// Whether the quad is held; a withdrawn row is `false` and only
    /// [`View::History`] reads it.
    pub held: bool,
    /// The asserter, when the consumer records one.
    pub asserter: Option<TermId>,
    /// The statement the quad belongs to, for reifier lookups.
    pub statement: Option<TermId>,
}

impl QuadMeta {
    /// A held row with no asserter or statement.
    pub const HELD: QuadMeta = QuadMeta {
        held: true,
        asserter: None,
        statement: None,
    };

    /// The order rows of one quad are kept in: held first, then by asserter,
    /// then by statement (`None` before `Some`).
    pub(crate) fn rank(&self) -> (bool, Option<TermId>, Option<TermId>) {
        (!self.held, self.asserter, self.statement)
    }
}

/// Which rows a read sees.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum View {
    /// Held rows only.
    Live,
    /// Held and withdrawn rows.
    History,
}

impl View {
    /// Whether a row with this meta is visible in the view.
    #[must_use]
    pub const fn admits(self, meta: &QuadMeta) -> bool {
        match self {
            View::Live => meta.held,
            View::History => true,
        }
    }
}
