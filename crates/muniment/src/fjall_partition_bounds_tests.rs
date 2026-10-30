// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use fjall::{bounded_budget::{Budget, ReadLimits}, bounded_read::ReadError, Config, PartitionCreateOptions};

#[test]
fn partition_bounded_read_uses_snapshot_and_caller_ledger() {
    let dir = tempfile::tempdir().unwrap();
    let keyspace = Config::new(dir.path()).open().unwrap();
    let partition = keyspace.open_partition("test", PartitionCreateOptions::default()).unwrap();
    partition.insert("key", "hello").unwrap();
    let mut limits = ReadLimits::unlimited();
    limits.max_value_bytes = 4;
    let mut operation = Budget::new(limits.clone());
    assert!(matches!(partition.get_bounded(b"key", &mut Budget::new(limits.clone()), &mut operation), Err(ReadError::ValueLimit { limit: 4 })));
    limits.max_value_bytes = 5;
    assert_eq!(partition.get_bounded(b"key", &mut Budget::new(limits.clone()), &mut Budget::new(limits)).unwrap().unwrap().as_ref(), b"hello");
}
