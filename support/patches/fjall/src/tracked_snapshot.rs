// Copyright (c) 2024-present, fjall-rs
// This source code is licensed under both the Apache 2.0 and MIT License
// (found in the LICENSE-* files in the repository)

use crate::snapshot_nonce::SnapshotNonce;
use lsm_tree::{KvPair, UserKey, UserValue};
use std::ops::RangeBounds;

/// A snapshot captures a read-only point-in-time view of the tree at the time the snapshot was created
///
/// As long as the snapshot is open, old versions of objects will not be evicted as to
/// keep the snapshot consistent. Thus, snapshots should only be kept around for as little as possible.
///
/// Snapshots do not persist across restarts.
pub struct TrackedSnapshot {
    inner: lsm_tree::Snapshot,

    /// Sequence number captured by this snapshot.
    pub seqno: u64,

    #[allow(unused)]
    nonce: SnapshotNonce,
}

impl TrackedSnapshot {
    pub(crate) fn new(snapshot: lsm_tree::Snapshot, nonce: SnapshotNonce) -> Self {
        Self {
            seqno: snapshot.seqno,
            inner: snapshot,
            nonce,
        }
    }

    /// Read a value at this snapshot.
    pub fn get<K: AsRef<[u8]>>(&self, key: K) -> lsm_tree::Result<Option<UserValue>> {
        self.inner.get(key)
    }

    /// Read a value's length at this snapshot.
    pub fn size_of<K: AsRef<[u8]>>(&self, key: K) -> lsm_tree::Result<Option<u32>> {
        self.inner.size_of(key)
    }

    /// Test whether a key is visible at this snapshot.
    pub fn contains_key<K: AsRef<[u8]>>(&self, key: K) -> lsm_tree::Result<bool> {
        self.inner.contains_key(key)
    }

    /// Return the first visible entry.
    pub fn first_key_value(&self) -> lsm_tree::Result<Option<KvPair>> {
        self.inner.first_key_value()
    }

    /// Return the last visible entry.
    pub fn last_key_value(&self) -> lsm_tree::Result<Option<KvPair>> {
        self.inner.last_key_value()
    }

    /// Test whether this snapshot contains any entries.
    pub fn is_empty(&self) -> lsm_tree::Result<bool> {
        self.inner.is_empty()
    }

    /// Count visible entries, scanning the snapshot.
    pub fn len(&self) -> lsm_tree::Result<usize> {
        self.inner.len()
    }

    /// Iterate over the snapshot while retaining its owner lease.
    pub fn iter(&self) -> impl DoubleEndedIterator<Item = lsm_tree::Result<KvPair>> + 'static {
        crate::iter::LeasedIter::new(self.inner.iter(), self.nonce.clone())
    }

    /// Iterate over snapshot keys while retaining its owner lease.
    pub fn keys(&self) -> impl DoubleEndedIterator<Item = lsm_tree::Result<UserKey>> + 'static {
        crate::iter::LeasedIter::new(self.inner.keys(), self.nonce.clone())
    }

    /// Iterate over snapshot values while retaining its owner lease.
    pub fn values(&self) -> impl DoubleEndedIterator<Item = lsm_tree::Result<UserValue>> + 'static {
        crate::iter::LeasedIter::new(self.inner.values(), self.nonce.clone())
    }

    /// Iterate over a snapshot range while retaining its owner lease.
    pub fn range<K: AsRef<[u8]>, R: RangeBounds<K>>(
        &self,
        range: R,
    ) -> impl DoubleEndedIterator<Item = lsm_tree::Result<KvPair>> + 'static {
        crate::iter::LeasedIter::new(self.inner.range(range), self.nonce.clone())
    }

    /// Iterate over a snapshot prefix while retaining its owner lease.
    pub fn prefix<K: AsRef<[u8]>>(
        &self,
        prefix: K,
    ) -> impl DoubleEndedIterator<Item = lsm_tree::Result<KvPair>> + 'static {
        crate::iter::LeasedIter::new(self.inner.prefix(prefix), self.nonce.clone())
    }
}
