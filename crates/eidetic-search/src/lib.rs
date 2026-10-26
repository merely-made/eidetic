// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! eidetic-search — lexical recall over derived document projections.
//!
//! [`DocumentIndex`] is the neutral transient seam: callers provide already
//! grouped [`SearchDocument`] values and receive their opaque keys back. The
//! [`TrailIndex`] adapter is an in-tree BM25 index minted **from** `BrowsingTrace`
//! codicils: derived state, never the source of truth. The trace corpus in
//! the eidetic store is the authority; the index can always be re-minted
//! from it ([`TrailIndex::rebuild`]), which is exactly what happens when an
//! on-disk projection's format no longer matches this build
//! ([`SearchError::FormatMismatch`] — the format version travels with the
//! index and recipients reject-or-re-mint, the workspace-pins doctrine
//! applied to a wire format).
//!
//! The engine is a postings map and a weighted per-field BM25 ([`bm25`]) over
//! one tokenizer ([`tokenize`]): no mmap, no threads, no C, so the crate
//! follows muniment to wasm/OPFS. tantivy was retired for that reason and for
//! its fixed cost per re-mint, which is the only thing the consumer does.
//!
//! Three surfaces:
//!
//! - **Recall** — [`TrailIndex::search`]: BM25 over tokenized titles, page
//!   text, and URL components, weighted by [`index::FieldWeights`]; a single
//!   absolute URL takes the exact canonical-URL lane. "Where did I read
//!   about X?"
//! - **Reports** — [`TrailIndex::top_domains`] /
//!   [`TrailIndex::visits_histogram`] over the stored columns (domain / owner
//!   / time / transition, so reports need no re-index).
//! - **Candidates** — [`CandidateIndex`]: a token-prefix lookup over the same
//!   tokenizer, for a lane that must narrow the corpus before it ranks
//!   (the behavioural one) without scanning every record per keystroke.
//! - **Fusion** — [`fuse`] / [`fuse_many`]: the engine-agnostic seam that
//!   merges this crate's lexical ranking with a vector ranking (`intel/esp`
//!   or any other) and any further lane — the behavioural one in
//!   `eidetic::browsing::frecency` — by reciprocal-rank fusion. This crate
//!   deliberately does not depend on an embedding engine; the caller brings
//!   the rankings.
//!
//! The [`SearchIndexSpec`] is the persisted [`TrailIndex`] projection's local
//! compatibility contract. Community consumers exchange admitted capture
//! records and remint [`DocumentIndex`] locally rather than treating a shared
//! index or its scores as authority.

pub mod bm25;
pub mod candidates;
pub mod document;
pub mod fusion;
pub mod index;
pub mod spec;
pub mod tokenize;

pub use bm25::{Bm25Config, Bm25Index};
pub use candidates::CandidateIndex;
pub use document::{DocumentHit, DocumentIndex, DocumentIndexConfig, SearchDocument};
pub use fusion::{FusedHit, Ranking, fuse, fuse_many};
pub use index::{FieldWeights, Hit, IndexConfig, TrailIndex};
pub use spec::{SEARCH_INDEX_SCHEMA_REF, SPEC_SIDECAR, SearchIndexSpec, bootstrap_search_schema};
pub use tokenize::{Stemmer, Tokenizer};

/// Search-lane failures.
#[derive(Debug, thiserror::Error)]
pub enum SearchError {
    /// The on-disk projection was written by a different engine or field set
    /// than this build. Re-mint from traces ([`TrailIndex::rebuild`]) — the
    /// corpus is the source of truth, the index is derived.
    #[error("index format mismatch: on disk {found}, this build {current} — re-mint from traces")]
    FormatMismatch { found: String, current: String },
    /// No index (or no spec sidecar) at the path. Build one from traces.
    #[error("no trail index at {0} — build one from traces first")]
    Missing(String),
    #[error("search engine: {0}")]
    Engine(String),
    #[error("search io: {0}")]
    Io(String),
}

impl From<std::io::Error> for SearchError {
    fn from(e: std::io::Error) -> Self {
        SearchError::Io(e.to_string())
    }
}

/// Crate result alias.
pub type Result<T> = std::result::Result<T, SearchError>;
