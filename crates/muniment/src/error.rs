// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The store error type.

use std::fmt;

/// A resource whose configured storage limit was exceeded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreLimitKind {
    ValueBytes,
    KeyBytes,
    BlockEncodedBytes,
    BlockDecodedBytes,
    InputBytes,
    DecodeBytes,
    MetadataBytes,
    AllocationBytes,
    VisitedItems,
    OutputItems,
    OutputKeyBytes,
    ReplayBytes,
    ReplayItems,
}

/// A storage failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreError {
    /// The host backend (filesystem, OPFS, redb, fjall, ...) failed. The string
    /// is the backend's own error, stringified so the seam stays backend-agnostic.
    Backend(String),
    /// Encoding or decoding a typed value through a [`Codec`](crate::Codec) failed.
    Codec(String),
    /// The backend refused work exceeding an explicit resource limit.
    LimitExceeded { kind: StoreLimitKind, limit: u64 },
    /// A persisted format or tree cannot be safely handled by this backend.
    Unsupported(String),
    /// Persisted data is damaged or malformed.
    Corruption(String),
    /// This backend cannot honor [`Backend::transact`](crate::Backend::transact):
    /// it has no way to run a caller's read-then-write closure against one
    /// consistent snapshot and commit atomically. Distinct from `Backend` so a
    /// caller can match on it and fall back deliberately (e.g. refuse custody
    /// operations rather than risk a stale read) instead of treating it as an
    /// ordinary I/O failure.
    NotTransactional,
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StoreError::Backend(msg) => write!(f, "backend: {msg}"),
            StoreError::Codec(msg) => write!(f, "codec: {msg}"),
            StoreError::LimitExceeded { kind, limit } => write!(f, "storage {kind:?} limit exceeded ({limit})"),
            StoreError::Unsupported(msg) => write!(f, "unsupported storage: {msg}"),
            StoreError::Corruption(msg) => write!(f, "storage corruption: {msg}"),
            StoreError::NotTransactional => {
                write!(f, "backend does not support transactional read-then-write")
            },
        }
    }
}

impl std::error::Error for StoreError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limit_error_retains_its_kind_and_limit() {
        let error = StoreError::LimitExceeded {
            kind: StoreLimitKind::ValueBytes,
            limit: 4,
        };
        assert_eq!(error.clone(), error);
        assert!(matches!(error, StoreError::LimitExceeded { kind: StoreLimitKind::ValueBytes, limit: 4 }));
        assert!(error.to_string().contains('4'));
    }
}
