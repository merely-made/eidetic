// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Verified edition chains and the writer that extends one.

use std::collections::BTreeMap;

use p2panda_core::{Body, Hash, Header, SigningKey};

use super::{
    EditionBody, EditionError, EditionExt, EditionId, EditionLimits, EditionOperation,
    check_edition_operation,
};
use crate::causal::{CausalEntry, CausalError, author_head, causal_projection};

/// One verified edition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edition {
    /// Operation hash.
    pub id: EditionId,
    /// Position in the log, from 0.
    pub seq_num: u32,
    /// Decoded body.
    pub body: EditionBody,
}

/// A log that verified end to end under one key: signatures, body hashes,
/// contiguous sequence numbers and backlinks, no two editions at one position.
#[derive(Clone, Debug)]
pub struct EditionChain {
    key: [u8; 32],
    editions: Vec<Edition>,
    operations: Vec<EditionOperation>,
}

impl EditionChain {
    /// The log key the chain verified under.
    pub fn key(&self) -> &[u8; 32] {
        &self.key
    }

    /// Editions in order, oldest first.
    pub fn editions(&self) -> &[Edition] {
        &self.editions
    }

    /// The newest edition this chain shows. It is the newest known, not
    /// necessarily the newest that exists.
    pub fn head(&self) -> &Edition {
        self.editions.last().expect("verified chains are non-empty")
    }

    /// The edition with this id, if the chain holds it.
    pub fn get(&self, id: &EditionId) -> Option<&Edition> {
        self.editions.iter().find(|edition| edition.id == *id)
    }

    /// The source named by the first edition, when this log is a fork.
    pub fn fork_source(&self) -> Option<EditionId> {
        self.editions.first().and_then(|e| e.body.fork_of)
    }

    /// Resolve this fork's source edition inside the source log's chain.
    ///
    /// # Errors
    /// Fails when this log is not a fork or the source chain lacks the edition.
    pub fn resolve_fork<'a>(&self, source: &'a Self) -> Result<&'a Edition, EditionError> {
        let id = self.fork_source().ok_or(EditionError::NotAFork)?;
        source.get(&id).ok_or(EditionError::ForkSourceMissing)
    }

    /// The signed operations, oldest first.
    pub fn operations(&self) -> &[EditionOperation] {
        &self.operations
    }
}

/// Verify operations as one log under `key`. Repeats of an operation are
/// ignored; order of input is irrelevant.
///
/// # Errors
/// Refuses the first bad operation, an equivocation (two editions at one
/// sequence number), or a chain that does not run from 0 by backlink.
pub fn verify_edition_chain(
    key: &[u8; 32],
    operations: &[EditionOperation],
    limits: EditionLimits,
) -> Result<EditionChain, EditionError> {
    let mut unique = BTreeMap::new();
    for operation in operations {
        let body = check_edition_operation(operation, Some(key), limits)?;
        if unique.len() >= limits.max_editions {
            return Err(EditionError::Evidence("too many editions".into()));
        }
        unique.insert(operation.hash, (operation, body));
    }
    if unique.is_empty() {
        return Err(EditionError::Empty);
    }
    let entries: Vec<CausalEntry<u64>> = unique
        .values()
        .map(|(operation, _)| CausalEntry::from_operation(*operation, 0, Vec::new()))
        .collect();
    causal_projection(&entries).map_err(|error| match error {
        CausalError::ForkedSequence { seq_num } => EditionError::Equivocation { seq_num },
        other => EditionError::Invalid(other.to_string()),
    })?;
    let mut ordered: Vec<_> = unique.into_values().collect();
    ordered.sort_by_key(|(operation, _)| operation.header.seq_num);
    let mut previous: Option<Hash> = None;
    let mut editions = Vec::with_capacity(ordered.len());
    for (index, (operation, body)) in ordered.iter().enumerate() {
        let seq_num = operation.header.seq_num;
        if usize::try_from(seq_num).ok() != Some(index) || operation.header.backlink != previous {
            return Err(EditionError::Discontinuous { seq_num });
        }
        previous = Some(operation.hash);
        editions.push(Edition {
            id: EditionId(*operation.hash.as_bytes()),
            seq_num,
            body: body.clone(),
        });
    }
    Ok(EditionChain {
        key: *key,
        editions,
        operations: ordered.into_iter().map(|(op, _)| op.clone()).collect(),
    })
}

/// Signs the next edition of one log.
pub struct EditionWriter {
    key: SigningKey,
    next: u32,
    backlink: Option<Hash>,
    limits: EditionLimits,
}

impl EditionWriter {
    /// Start a new log. Its first edition may name a fork source.
    pub fn new(key: SigningKey) -> Self {
        Self {
            key,
            next: 0,
            backlink: None,
            limits: EditionLimits::default(),
        }
    }

    /// Continue a verified log.
    ///
    /// # Errors
    /// Fails if the chain belongs to another key or its sequence is exhausted.
    pub fn resume(key: SigningKey, chain: &EditionChain) -> Result<Self, EditionError> {
        let public = *key.verifying_key().as_bytes();
        if public != chain.key {
            return Err(EditionError::WrongAuthor);
        }
        let entries: Vec<_> = chain
            .operations
            .iter()
            .map(|op| CausalEntry::from_operation(op, 0u64, Vec::new()))
            .collect();
        let (next, backlink) =
            author_head(&entries, public, &0u64).map_err(|_| EditionError::SequenceExhausted)?;
        Ok(Self {
            key,
            next,
            backlink: backlink.map(Hash::from_bytes),
            limits: EditionLimits::default(),
        })
    }

    /// Sign and return the next edition.
    ///
    /// # Errors
    /// Refuses a body that breaks the record rules for its position.
    pub fn append(&mut self, body: &EditionBody) -> Result<EditionOperation, EditionError> {
        let bytes = body.encode(self.limits)?;
        let payload = Body::from_bytes(&bytes);
        let header = Header::builder()
            .body(payload.as_bytes())
            .seq_num(self.next)
            .backlink(self.backlink)
            .build(&self.key, EditionExt::default());
        let operation = EditionOperation {
            hash: header.hash(),
            header,
            body: Some(payload),
        };
        check_edition_operation(&operation, None, self.limits)?;
        self.next = self
            .next
            .checked_add(1)
            .ok_or(EditionError::SequenceExhausted)?;
        self.backlink = Some(operation.hash);
        Ok(operation)
    }
}
