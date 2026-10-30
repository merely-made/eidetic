// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Synchronous read forwarding with an owned snapshot nonce.

use super::PartitionHandle;
use crate::snapshot_nonce::SnapshotNonce;
use lsm_tree::{
    AnyTree, Slice,
    bounded_budget::Budget,
    bounded_read::{self, ReadError},
};

impl PartitionHandle {
    /// Get a value while applying both the call and operation ledgers.
    ///
    /// # Errors
    /// Returns a typed limit, unsupported representation, corruption or I/O error.
    #[doc(hidden)]
    pub fn get_bounded(
        &self,
        key: &[u8],
        call: &mut Budget,
        operation: &mut Budget,
    ) -> Result<Option<Slice>, ReadError> {
        let seqno = self.seqno.get();
        let _nonce = SnapshotNonce::new(seqno, self.snapshot_tracker.clone());
        match &self.tree {
            AnyTree::Standard(tree) => bounded_read::get(tree, key, Some(seqno), call, operation),
            AnyTree::Blob(_) => Err(ReadError::Unsupported("blob tree")),
        }
    }

    /// Probe a key while applying both ledgers.
    ///
    /// # Errors
    /// Returns a typed limit, unsupported representation, corruption or I/O error.
    #[doc(hidden)]
    pub fn exists_bounded(
        &self,
        start: &[u8],
        end: Option<&[u8]>,
        call: &mut Budget,
        operation: &mut Budget,
    ) -> Result<bool, ReadError> {
        let seqno = self.seqno.get();
        let _nonce = SnapshotNonce::new(seqno, self.snapshot_tracker.clone());
        match &self.tree {
            AnyTree::Standard(tree) => {
                bounded_read::exists(tree, start, end, Some(seqno), call, operation)
            }
            AnyTree::Blob(_) => Err(ReadError::Unsupported("blob tree")),
        }
    }

    /// Read every visible key in a half-open range, or return an explicit refusal.
    ///
    /// # Errors
    /// Returns a typed limit, unsupported representation, corruption or I/O error.
    #[doc(hidden)]
    pub fn scan_bounded(
        &self,
        start: &[u8],
        end: Option<&[u8]>,
        call: &mut Budget,
        operation: &mut Budget,
    ) -> Result<Vec<Slice>, ReadError> {
        let seqno = self.seqno.get();
        let _nonce = SnapshotNonce::new(seqno, self.snapshot_tracker.clone());
        match &self.tree {
            AnyTree::Standard(tree) => {
                bounded_read::scan(tree, start, end, Some(seqno), call, operation)
            }
            AnyTree::Blob(_) => Err(ReadError::Unsupported("blob tree")),
        }
    }
}
