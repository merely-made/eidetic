// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The v2 journal digest: a chain over the canonical entries, so each
//! prefix's digest follows from the one before it and every one can be kept.
//!
//! v1 ([`Journal::prefix_digest`]) hashes the entry count before the entries,
//! so naming a longer prefix means hashing the whole prefix again. v2 drops the
//! count and chains instead: an append hashes one entry and the previous
//! digest, and a [`DigestChain`] keeps every prefix's digest, 32 bytes an
//! entry, so naming any cursor is a lookup (mere:engine-split D154).
//!
//! # Definition
//!
//! Write `D` for the domain string `"muniment journal prefix digest v2"`,
//! `H(x)` for BLAKE3 in key-derivation mode with context `D` over input `x`
//! (`blake3::Hasher::new_derive_key(D)`, the mode v1 uses under its own
//! string), `c_i` for entry `i`'s canonical encoding (the bytes v1 hashes; see
//! the `digest` module), `le64(n)` for `n` as eight little-endian bytes and `‖`
//! for concatenation. The digest of the first `k` entries is `d_k`, where
//!
//! ```text
//! d_0     = H(empty input)
//! d_(i+1) = H(d_i ‖ le64(len(c_i)) ‖ c_i)
//! ```
//!
//! Each step hashes the previous 32-byte digest, then one entry framed as v1
//! frames it: its encoded length, then its encoding. No count is hashed; a
//! prefix's length is the number of steps. Like v1, the digest covers the
//! entries and their order and nothing else: not the log's id, its provenance
//! or the causes recorded beside the entries. So a prefix keeps its digest as
//! the log grows, across saves and loads, and in a fork that copied it.

use serde::Serialize;

use crate::blob::Hash;
use crate::error::StoreError;

use super::digest::canonical_into;
use super::log::Journal;
use super::seq::Seq;

/// Domain separation, and the version of this definition: a change to the
/// encoding above changes this string.
const DOMAIN: &str = "muniment journal prefix digest v2";

/// `d_0`, the empty prefix's digest.
fn empty() -> Hash {
    Hash::from_bytes(*blake3::Hasher::new_derive_key(DOMAIN).finalize().as_bytes())
}

/// One step: the digest of a prefix one entry longer than `previous`'s.
fn step(previous: &Hash, encoded: &[u8]) -> Hash {
    let mut hasher = blake3::Hasher::new_derive_key(DOMAIN);
    hasher.update(previous.as_bytes());
    hasher.update(&(encoded.len() as u64).to_le_bytes());
    hasher.update(encoded);
    Hash::from_bytes(*hasher.finalize().as_bytes())
}

impl<T: Serialize> Journal<T> {
    /// The v2 digest of the first `len` entries, or `None` if the log holds
    /// fewer, computed from scratch: linear in `len`, like v1. A holder that
    /// names prefixes often keeps a [`DigestChain`] instead.
    ///
    /// # Errors
    ///
    /// [`StoreError::Codec`] if an entry's `Serialize` implementation fails.
    pub fn chain_digest(&self, len: usize) -> Result<Option<Hash>, StoreError> {
        let Some(prefix) = self.entries().get(..len) else {
            return Ok(None);
        };
        let mut digest = empty();
        let mut encoded = Vec::new();
        for entry in prefix {
            canonical_into(entry, &mut encoded)?;
            digest = step(&digest, &encoded);
        }
        Ok(Some(digest))
    }
}

/// Every prefix's v2 digest for one journal, kept beside it: `at(k)` names the
/// first `k` entries.
///
/// It holds 32 bytes an entry plus the empty prefix's. Appending an entry
/// encodes and hashes that entry only. It is not stored with the journal: a
/// holder builds it with [`of`](Self::of) when it loads one, then keeps it
/// current with [`push`](Self::push) or [`catch_up`](Self::catch_up).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DigestChain {
    /// `digests[k]` is `d_k`; never empty.
    digests: Vec<Hash>,
}

impl Default for DigestChain {
    fn default() -> Self {
        Self::new()
    }
}

impl DigestChain {
    /// The chain of an empty journal.
    pub fn new() -> Self {
        Self {
            digests: vec![empty()],
        }
    }

    /// The chain of every prefix of `journal`.
    ///
    /// # Errors
    ///
    /// [`StoreError::Codec`] if an entry's `Serialize` implementation fails.
    pub fn of<T: Serialize>(journal: &Journal<T>) -> Result<Self, StoreError> {
        let mut chain = Self::new();
        chain.digests.reserve(journal.len());
        chain.catch_up(journal)?;
        Ok(chain)
    }

    /// Extend the chain by the entry appended after the ones it covers, and
    /// return the new newest digest.
    ///
    /// # Errors
    ///
    /// [`StoreError::Codec`] if the entry's `Serialize` implementation fails;
    /// the chain is then unchanged.
    pub fn push<T: ?Sized + Serialize>(&mut self, entry: &T) -> Result<Hash, StoreError> {
        let mut encoded = Vec::new();
        canonical_into(entry, &mut encoded)?;
        let next = step(&self.newest(), &encoded);
        self.digests.push(next);
        Ok(next)
    }

    /// Extend the chain by the entries `journal` holds past the ones it covers,
    /// and return the newest digest.
    ///
    /// # Errors
    ///
    /// [`StoreError::Corruption`] if the chain covers more entries than the
    /// journal holds, so it was built for another; [`StoreError::Codec`] if an
    /// entry's `Serialize` implementation fails, with the entries before it
    /// added.
    pub fn catch_up<T: Serialize>(&mut self, journal: &Journal<T>) -> Result<Hash, StoreError> {
        if self.len() > journal.len() {
            return Err(StoreError::Corruption(format!(
                "a digest chain over {} entries cannot follow a journal of {}",
                self.len(),
                journal.len()
            )));
        }
        let mut encoded = Vec::new();
        for entry in journal.from(Seq(self.len() as u64)) {
            canonical_into(entry, &mut encoded)?;
            let next = step(&self.newest(), &encoded);
            self.digests.push(next);
        }
        Ok(self.newest())
    }

    /// The digest of the first `len` entries, or `None` past the chain.
    pub fn at(&self, len: usize) -> Option<Hash> {
        self.digests.get(len).copied()
    }

    /// The digest of every entry the chain covers.
    pub fn newest(&self) -> Hash {
        self.digests.last().copied().unwrap_or_else(empty)
    }

    /// The number of entries the chain covers.
    pub fn len(&self) -> usize {
        self.digests.len() - 1
    }

    /// Whether the chain covers no entries.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
#[path = "chain_tests.rs"]
mod tests;
