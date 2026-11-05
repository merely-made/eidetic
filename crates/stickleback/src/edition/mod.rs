// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Edition records: one signed log per key naming what a site, share or clip
//! is at each point in time.
//!
//! An edition is a p2panda operation whose body names content by digest. The
//! header's sequence number and backlink order editions and expose rollback
//! and equivocation; no schema version doubles as a counter. A reader holding
//! only the log key verifies a chain with [`verify_edition_chain`]. Any
//! Ed25519 key can root a log. Provider hints are advisory and never decide.
//! Carriage, content storage and the site profile belong to other crates.

mod body;
mod chain;
mod policy;
#[cfg(test)]
mod tests;
mod wire;

pub use body::{
    ContentRef, EditionBody, EditionId, EditionKind, EditionLimits, Provenance, SealedMarker,
};
pub use chain::{Edition, EditionChain, EditionWriter, verify_edition_chain};
pub use p2panda_core::{SigningKey, VerifyingKey};
pub use policy::{EditionPolicy, check_edition_operation, edition_topic};
pub use wire::{decode_evidence, encode_evidence, merge_evidence};

use serde::{Deserialize, Serialize};

/// Header extension marking an operation as an edition (format version 1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditionExt {
    /// Edition record format, not an edition counter.
    pub version: u8,
}

impl Default for EditionExt {
    fn default() -> Self {
        Self { version: 1 }
    }
}

/// An edition operation as it travels.
pub type EditionOperation = p2panda_core::Operation<EditionExt>;

/// Why an edition, or a chain of them, is refused.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum EditionError {
    /// Signature, header or body hash does not verify.
    #[error("invalid edition operation: {0}")]
    Invalid(String),
    /// The operation is signed by a key other than the log key.
    #[error("edition is signed by a key other than the log key")]
    WrongAuthor,
    /// The body is missing, malformed, oversized or breaks the record rules.
    #[error("edition body refused: {0}")]
    Body(String),
    /// Two different editions claim one sequence number.
    #[error("log key equivocated at sequence {seq_num}")]
    Equivocation { seq_num: u32 },
    /// The chain does not run from sequence 0 without gaps or wrong backlinks.
    #[error("edition chain is not contiguous at sequence {seq_num}")]
    Discontinuous { seq_num: u32 },
    /// No editions were supplied.
    #[error("no editions supplied")]
    Empty,
    /// Evidence bytes are malformed or exceed a limit.
    #[error("edition evidence refused: {0}")]
    Evidence(String),
    /// A fork's first edition names an edition absent from the supplied source.
    #[error("fork source edition is not in the supplied chain")]
    ForkSourceMissing,
    /// The chain has no fork source to resolve.
    #[error("first edition names no fork source")]
    NotAFork,
    /// The log's sequence space is exhausted.
    #[error("edition sequence is exhausted")]
    SequenceExhausted,
}
