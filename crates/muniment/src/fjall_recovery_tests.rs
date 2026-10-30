// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
use fjall::bounded_recovery::{prepare_journal, RecoveryBudget, RecoveryError, RecoveryLimitKind, RecoveryLimits};
use std::path::PathBuf;
use tempfile::TempDir;

fn limits() -> RecoveryLimits {
    RecoveryLimits { max_value_bytes: 1024, max_key_bytes: 1024,
        max_replay_bytes: 65_536, max_replay_items: 1000, max_metadata_bytes: 65_536,
        max_input_bytes: 134_217_728, max_alloc_bytes: 65_536 }
}

fn journal(items: &[(&str, &[u8])]) -> (TempDir, PathBuf) {
    let dir = TempDir::new().unwrap();
    {
        let keyspace = Config::new(dir.path()).open().unwrap();
        let partition = keyspace.open_partition("p", PartitionCreateOptions::default()).unwrap();
        let mut batch = keyspace.batch();
        for (key, value) in items {
            batch.insert(&partition, *key, *value);
        }
        batch.commit().unwrap();
        keyspace.persist(fjall::PersistMode::SyncAll).unwrap();
    }
    let files: Vec<_> = std::fs::read_dir(dir.path().join("journals")).unwrap()
        .map(|entry| entry.unwrap().path()).collect();
    assert_eq!(files.len(), 1);
    (dir, files[0].clone())
}

#[test]
fn journal_allocation_limit_includes_checksum_scratch() {
    let (_dir, path) = journal(&[("k", b"v")]);
    let before = std::fs::read(&path).unwrap();
    let limits = RecoveryLimits { max_alloc_bytes: 200, ..limits() };
    let result = prepare_journal(&path, &mut RecoveryBudget::new(limits));
    assert!(matches!(result, Err(RecoveryError::Limit { kind: RecoveryLimitKind::AllocationBytes, .. })));
    assert_eq!(std::fs::read(path).unwrap(), before);
}

#[test]
fn journal_rejects_input_limit_before_requesting_value_buffer() {
    let value = vec![7; 8192];
    let (_dir, path) = journal(&[("k", &value)]);
    let before = std::fs::read(&path).unwrap();
    let limits = RecoveryLimits { max_value_bytes: 8192, max_input_bytes: 26, ..limits() };
    let mut budget = RecoveryBudget::new(limits);
    assert!(matches!(prepare_journal(&path, &mut budget), Err(RecoveryError::Limit { kind: RecoveryLimitKind::InputBytes, .. })));
    assert!(budget.allocated_bytes() < 8192, "value buffer was requested before input refusal");
    assert_eq!(std::fs::read(path).unwrap(), before);
}

#[test]
fn truncated_large_value_does_not_request_declared_buffer() {
    let value = vec![7; 8192];
    let (_dir, path) = journal(&[("k", &value)]);
    let mut bytes = std::fs::read(&path).unwrap();
    // The length field is present, but the declared body is incomplete.
    bytes.truncate(38);
    std::fs::write(&path, &bytes).unwrap();
    let mut budget = RecoveryBudget::new(RecoveryLimits { max_value_bytes: 8192, ..limits() });
    let result = prepare_journal(&path, &mut budget);
    assert!(matches!(result, Err(RecoveryError::Corruption(_))));
    assert!(budget.allocated_bytes() < 8192, "truncated value was allocated");
    assert_eq!(std::fs::read(path).unwrap(), bytes);
}

#[test]
fn journal_accepts_exact_key_value_boundaries_and_large_batch() {
    let items: Vec<_> = (0..100).map(|_| ("key", b"123456".as_slice())).collect();
    let (_dir, path) = journal(&items);
    let exact = RecoveryLimits { max_key_bytes: 4, max_value_bytes: 6, ..limits() };
    assert_eq!(prepare_journal(path, &mut RecoveryBudget::new(exact)).unwrap().item_count(), 100);
}

#[test]
fn journal_replay_item_limit_accepts_exact_batch_boundary() {
    let (_dir, path) = journal(&[("k", b"v")]);
    let exact = RecoveryLimits { max_replay_items: 3, ..limits() };
    assert_eq!(prepare_journal(&path, &mut RecoveryBudget::new(exact)).unwrap().item_count(), 1);
    let small = RecoveryLimits { max_replay_items: 2, ..limits() };
    assert!(matches!(prepare_journal(path, &mut RecoveryBudget::new(small)), Err(RecoveryError::Limit { kind: RecoveryLimitKind::ReplayItems, .. })));
}

#[test]
fn journal_replay_budget_is_shared_between_files() {
    let (_dir1, first) = journal(&[("k", b"v")]);
    let (_dir2, second) = journal(&[("k", b"v")]);
    let mut budget = RecoveryBudget::new(RecoveryLimits { max_replay_bytes: 5, ..limits() });
    assert_eq!(prepare_journal(&first, &mut budget).unwrap().item_count(), 1);
    assert!(matches!(prepare_journal(&second, &mut budget), Err(RecoveryError::Limit { kind: RecoveryLimitKind::ReplayBytes, .. })));
}

#[test]
fn journal_reports_count_mismatch_without_mutation() {
    let (_dir, path) = journal(&[("a", b"v"), ("b", b"v")]);
    let mut bytes = std::fs::read(&path).unwrap();
    bytes[1..5].copy_from_slice(&1_u32.to_be_bytes());
    std::fs::write(&path, &bytes).unwrap();
    assert!(matches!(prepare_journal(&path, &mut RecoveryBudget::new(limits())), Err(RecoveryError::Corruption(_))));
    assert_eq!(std::fs::read(path).unwrap(), bytes);
}

#[test]
fn journal_reports_checksum_mismatch_without_mutation() {
    let (_dir, path) = journal(&[("k", b"v")]);
    let mut bytes = std::fs::read(&path).unwrap();
    // One-item batch: 15-byte start, 12-byte item, end tag then checksum.
    bytes[29] ^= 1;
    std::fs::write(&path, &bytes).unwrap();
    assert!(matches!(prepare_journal(&path, &mut RecoveryBudget::new(limits())), Err(RecoveryError::Corruption(_))));
    assert_eq!(std::fs::read(path).unwrap(), bytes);
}

#[test]
fn journal_incomplete_tail_is_not_truncated_by_prepare() {
    let (_dir, path) = journal(&[("k", b"v")]);
    let mut bytes = std::fs::read(&path).unwrap();
    // The journal is preallocated, so trim within the first batch trailer.
    bytes.truncate(39);
    std::fs::write(&path, &bytes).unwrap();
    assert_eq!(prepare_journal(&path, &mut RecoveryBudget::new(limits())).unwrap().item_count(), 0);
    assert_eq!(std::fs::read(path).unwrap(), bytes);
}

#[test]
fn partial_flush_across_partitions_preserves_acknowledged_values_on_reopen() {
    let dir = TempDir::new().unwrap();
    {
        let keyspace = Config::new(dir.path()).flush_workers(1).compaction_workers(0).open().unwrap();
        let first = keyspace.open_partition("first", PartitionCreateOptions::default()).unwrap();
        let second = keyspace.open_partition("second", PartitionCreateOptions::default()).unwrap();
        first.insert("earlier", "first-old").unwrap();
        second.insert("earlier", "second-old").unwrap();
        keyspace.persist(fjall::PersistMode::SyncAll).unwrap();
        first.rotate_memtable_and_wait().unwrap();
        assert!(first.flushes_completed() > 0);
        first.insert("later", "first-new").unwrap();
        second.insert("later", "second-new").unwrap();
        keyspace.persist(fjall::PersistMode::SyncAll).unwrap();
    }
    let keyspace = Config::new(dir.path()).flush_workers(0).compaction_workers(0).open().unwrap();
    let first = keyspace.open_partition("first", PartitionCreateOptions::default()).unwrap();
    let second = keyspace.open_partition("second", PartitionCreateOptions::default()).unwrap();
    for (partition, earlier, later) in [(&first, b"first-old".as_slice(), b"first-new".as_slice()), (&second, b"second-old".as_slice(), b"second-new".as_slice())] {
        assert_eq!(partition.get("earlier").unwrap().unwrap().as_ref(), earlier);
        assert_eq!(partition.get("later").unwrap().unwrap().as_ref(), later);
    }
}

#[test]
fn journal_malformed_marker_before_later_batch_is_corruption_without_mutation() {
    let dir = TempDir::new().unwrap();
    {
        let keyspace = Config::new(dir.path()).flush_workers(0).compaction_workers(0).open().unwrap();
        let partition = keyspace.open_partition("p", PartitionCreateOptions::default()).unwrap();
        partition.insert("a", "v").unwrap();
        partition.insert("b", "w").unwrap();
        keyspace.persist(fjall::PersistMode::SyncAll).unwrap();
    }
    let path = std::fs::read_dir(dir.path().join("journals")).unwrap().next().unwrap().unwrap().path();
    let original = std::fs::read(&path).unwrap();
    // Each first batch is 15-byte start + 12-byte item + 13-byte end.
    for damaged in [0xff_u8, 0] {
        let mut bytes = original.clone();
        bytes[40] = damaged;
        std::fs::write(&path, &bytes).unwrap();
        assert!(matches!(prepare_journal(&path, &mut RecoveryBudget::new(limits())), Err(RecoveryError::Corruption(_))));
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
    }
}

#[test]
fn journal_malformed_item_and_trailer_are_corruption_without_mutation() {
    let (_dir, path) = journal(&[("k", b"v")]);
    let original = std::fs::read(&path).unwrap();
    // One item: value type at 16, partition at 18, end trailer at 36.
    for index in [16_usize, 18, 36] {
        let mut bytes = original.clone();
        bytes[index] = 0xff;
        std::fs::write(&path, &bytes).unwrap();
        assert!(matches!(prepare_journal(&path, &mut RecoveryBudget::new(limits())), Err(RecoveryError::Corruption(_))));
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
    }
}

#[test]
fn journal_many_small_batches_use_bounded_geometric_metadata_growth() {
    let dir = TempDir::new().unwrap();
    {
        let keyspace = Config::new(dir.path()).flush_workers(0).compaction_workers(0).open().unwrap();
        let partition = keyspace.open_partition("p", PartitionCreateOptions::default()).unwrap();
        for index in 0..256 {
            partition.insert(format!("key-{index}"), b"v").unwrap();
        }
        keyspace.persist(fjall::PersistMode::SyncAll).unwrap();
    }
    let path = std::fs::read_dir(dir.path().join("journals")).unwrap().next().unwrap().unwrap().path();
    let before = std::fs::read(&path).unwrap();
    let ceilings = RecoveryLimits { max_metadata_bytes: 200_000, max_alloc_bytes: 1_000_000, ..limits() };
    let plan = prepare_journal(&path, &mut RecoveryBudget::new(ceilings)).unwrap();
    assert_eq!(plan.item_count(), 256);
    assert_eq!(std::fs::read(path).unwrap(), before);
}

#[test]
fn journal_declared_count_exceeding_remaining_input_does_not_allocate_batch() {
    let (_dir, path) = journal(&[("k", b"v")]);
    let mut bytes = std::fs::read(&path).unwrap();
    bytes[1..5].copy_from_slice(&u32::MAX.to_be_bytes());
    std::fs::write(&path, &bytes).unwrap();
    let mut ceilings = limits();
    ceilings.max_replay_items = u64::MAX;
    ceilings.max_metadata_bytes = u64::MAX;
    ceilings.max_alloc_bytes = u64::MAX;
    let mut budget = RecoveryBudget::new(ceilings);
    assert!(matches!(prepare_journal(&path, &mut budget), Err(RecoveryError::Corruption(_))));
    assert!(budget.allocated_bytes() < 1024);
    assert_eq!(std::fs::read(path).unwrap(), bytes);
}

#[test]
fn journal_padding_input_budget_counts_all_physical_bytes() {
    let (_dir, path) = journal(&[("k", b"v")]);
    let before = std::fs::read(&path).unwrap();
    let mut exact = limits();
    exact.max_input_bytes = before.len() as u64;
    assert_eq!(prepare_journal(&path, &mut RecoveryBudget::new(exact.clone())).unwrap().item_count(), 1);
    exact.max_input_bytes -= 1;
    assert!(matches!(prepare_journal(&path, &mut RecoveryBudget::new(exact)), Err(RecoveryError::Limit { kind: RecoveryLimitKind::InputBytes, .. })));
    assert_eq!(std::fs::read(path).unwrap(), before);
}
