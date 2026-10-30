// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::bounded_prepare::{PrepareError, RecoveryBudget, RecoveryResource};
use crate::bounded_trailer::read_segment_metadata;
use crate::{AbstractTree, Config};

struct Ledger { remaining: u64, max_key: u64 }
impl RecoveryBudget for Ledger {
    fn charge(&mut self, resource: RecoveryResource, amount: u64) -> Result<(), PrepareError> {
        self.remaining = self.remaining.checked_sub(amount).ok_or(PrepareError::Limit { resource, limit: self.remaining })?;
        Ok(())
    }
    fn check_key_bytes(&mut self, amount: u64) -> Result<(), PrepareError> {
        if amount > self.max_key { return Err(PrepareError::Limit { resource: RecoveryResource::KeyBytes, limit: self.max_key }); }
        Ok(())
    }
}

#[test]
fn reads_real_segment_metadata_without_tree_recovery() {
    let dir = tempfile::tempdir().unwrap();
    let tree = Config::new(dir.path()).open().unwrap();
    tree.insert("alpha", "value", 0);
    let segment = tree.flush_active_memtable(0).unwrap().unwrap();
    let mut file = std::fs::File::open(&segment.path).unwrap();
    let (meta, _) = read_segment_metadata(&mut file, &mut Ledger { remaining: 4096, max_key: 5 }).unwrap();
    assert_eq!(meta.key_range.min().as_ref(), b"alpha");
    assert_eq!(meta.id, segment.id());
}

#[test]
fn refuses_segment_metadata_key_over_limit() {
    let dir = tempfile::tempdir().unwrap();
    let tree = Config::new(dir.path()).open().unwrap();
    tree.insert("alpha", "value", 0);
    let segment = tree.flush_active_memtable(0).unwrap().unwrap();
    let mut file = std::fs::File::open(&segment.path).unwrap();
    assert!(matches!(read_segment_metadata(&mut file, &mut Ledger { remaining: 4096, max_key: 4 }), Err(PrepareError::Limit { resource: RecoveryResource::KeyBytes, .. })));
}
