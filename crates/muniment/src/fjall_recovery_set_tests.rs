// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::recovery_fixture::{inventory, limits, two_journals_with_repairable_tail};
use fjall::bounded_recovery::{prepare_journal_set, RecoveryBudget, RecoveryError, RecoveryLimitKind};
use fjall::Config;

#[test]
fn journal_set_refusal_preserves_earlier_tail_and_later_valid_batch() {
    let dir = two_journals_with_repairable_tail();
    let before = inventory(dir.path());
    let result = prepare_journal_set(dir.path(), &mut RecoveryBudget::new(limits(5)));
    assert!(matches!(result, Err(RecoveryError::Limit { kind: RecoveryLimitKind::ValueBytes, .. })));
    assert_eq!(inventory(dir.path()), before);
    let plan = prepare_journal_set(dir.path(), &mut RecoveryBudget::new(limits(32))).unwrap();
    assert_eq!(plan.journal_count(), 2);
    assert_eq!(plan.item_count(), 2);
    drop(plan);
    assert_eq!(inventory(dir.path()), before);
}

#[test]
fn journal_set_plan_holds_lease_until_drop() {
    let dir = two_journals_with_repairable_tail();
    let plan = prepare_journal_set(dir.path(), &mut RecoveryBudget::new(limits(32))).unwrap();
    assert!(Config::new(dir.path()).open().is_err());
    drop(plan);
    assert!(Config::new(dir.path()).open().is_ok());
}

#[test]
fn journal_set_metadata_limit_refusal_preserves_inventory() {
    let dir = two_journals_with_repairable_tail();
    let before = inventory(dir.path());
    let mut restricted = limits(32);
    restricted.max_metadata_bytes = 1;
    assert!(matches!(prepare_journal_set(dir.path(), &mut RecoveryBudget::new(restricted)), Err(RecoveryError::Limit { kind: RecoveryLimitKind::MetadataBytes, .. })));
    assert_eq!(inventory(dir.path()), before);
}

fn replay_fixture() -> tempfile::TempDir {
    let dir = tempfile::TempDir::new().unwrap();
    {
        let keyspace = Config::new(dir.path()).flush_workers(0).compaction_workers(0).open().unwrap();
        let primary = keyspace.open_partition("primary", fjall::PartitionCreateOptions::default()).unwrap();
        let unknown = keyspace.open_partition("unknown", fjall::PartitionCreateOptions::default()).unwrap();
        primary.insert("old", "prior").unwrap();
        primary.insert("gone", "old").unwrap();
        unknown.insert("alien", "hidden").unwrap();
        keyspace.persist(fjall::PersistMode::SyncAll).unwrap();
        assert!(primary.rotate_memtable().unwrap());
        primary.insert("old", "new").unwrap();
        primary.remove("gone").unwrap();
        primary.insert("new", "value").unwrap();
        keyspace.persist(fjall::PersistMode::SyncAll).unwrap();
    }
    dir
}

#[test]
fn prepared_replay_retains_latest_values_without_mutation() {
    use fjall::bounded_recovery::ReplayPartition;
    let dir = replay_fixture();
    let before = inventory(dir.path());
    let mut budget = RecoveryBudget::new(limits(128));
    let journals = prepare_journal_set(dir.path(), &mut budget).unwrap();
    let replay = journals.prepare_replay(&[ReplayPartition { name: "primary", highest_persisted_seqno: None }], &mut budget).unwrap();
    assert_eq!(replay.get("primary", b"old").as_deref(), Some(b"new".as_slice()));
    assert_eq!(replay.get("primary", b"gone").as_deref(), None);
    assert_eq!(replay.get("primary", b"new").as_deref(), Some(b"value".as_slice()));
    assert_eq!(replay.get("unknown", b"alien").as_deref(), None);
    drop(replay);
    drop(journals);
    assert_eq!(inventory(dir.path()), before);
}

#[test]
fn prepared_replay_allocation_limit_preserves_inventory() {
    use fjall::bounded_recovery::ReplayPartition;
    let dir = replay_fixture();
    let before = inventory(dir.path());
    let mut budget = RecoveryBudget::new(limits(128));
    let journals = prepare_journal_set(dir.path(), &mut budget).unwrap();
    budget.tighten_allocation_limit(budget.allocated_bytes());
    assert!(matches!(journals.prepare_replay(&[ReplayPartition { name: "primary", highest_persisted_seqno: None }], &mut budget), Err(RecoveryError::Limit { kind: RecoveryLimitKind::AllocationBytes, .. })));
    drop(journals);
    assert_eq!(inventory(dir.path()), before);
}

#[test]
fn prepared_replay_skips_persisted_sealed_items_but_keeps_active_items() {
    use fjall::bounded_recovery::ReplayPartition;
    let dir = tempfile::TempDir::new().unwrap();
    let watermark;
    {
        let keyspace = Config::new(dir.path()).flush_workers(0).compaction_workers(0).open().unwrap();
        let primary = keyspace.open_partition("primary", fjall::PartitionCreateOptions::default()).unwrap();
        primary.insert("sealed", "older").unwrap();
        watermark = keyspace.instant().checked_sub(1).unwrap();
        keyspace.persist(fjall::PersistMode::SyncAll).unwrap();
        assert!(primary.rotate_memtable().unwrap());
        primary.insert("active", "newer").unwrap();
        keyspace.persist(fjall::PersistMode::SyncAll).unwrap();
    }
    let before = inventory(dir.path());
    let mut budget = RecoveryBudget::new(limits(128));
    let journals = prepare_journal_set(dir.path(), &mut budget).unwrap();
    let replay = journals.prepare_replay(&[ReplayPartition { name: "primary", highest_persisted_seqno: Some(watermark) }], &mut budget).unwrap();
    assert_eq!(replay.get("primary", b"sealed").as_deref(), None);
    assert_eq!(replay.get("primary", b"active").as_deref(), Some(b"newer".as_slice()));
    let replay_without_watermark = journals.prepare_replay(&[ReplayPartition { name: "primary", highest_persisted_seqno: None }], &mut budget).unwrap();
    assert_eq!(replay_without_watermark.get("primary", b"sealed").as_deref(), Some(b"older".as_slice()));
    drop(replay_without_watermark);
    drop(replay);
    drop(journals);
    assert_eq!(inventory(dir.path()), before);
}

#[test]
fn prepared_replay_keeps_owner_lease_after_journal_plan_drops() {
    use fjall::bounded_recovery::ReplayPartition;
    let dir = replay_fixture();
    let mut budget = RecoveryBudget::new(limits(128));
    let journals = prepare_journal_set(dir.path(), &mut budget).unwrap();
    let replay = journals.prepare_replay(&[ReplayPartition { name: "primary", highest_persisted_seqno: None }], &mut budget).unwrap();
    drop(journals);
    assert!(Config::new(dir.path()).open().is_err());
    drop(replay);
    assert!(Config::new(dir.path()).open().is_ok());
}

#[test]
fn prepared_replay_rejects_a_fresh_budget() {
    use fjall::bounded_recovery::ReplayPartition;
    let dir = replay_fixture();
    let before = inventory(dir.path());
    let mut admission_budget = RecoveryBudget::new(limits(128));
    let journals = prepare_journal_set(dir.path(), &mut admission_budget).unwrap();
    let mut fresh_budget = RecoveryBudget::new(limits(128));
    assert!(matches!(journals.prepare_replay(&[ReplayPartition { name: "primary", highest_persisted_seqno: None }], &mut fresh_budget), Err(RecoveryError::Unsupported(_))));
    drop(journals);
    assert_eq!(inventory(dir.path()), before);
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn journal_set_metadata_budget_covers_native_directory_name_bounds() {
    let dir = two_journals_with_repairable_tail();
    let before = inventory(dir.path());
    let mut ceilings = limits(128);
    ceilings.max_metadata_bytes = 1_000_000;
    let plan = prepare_journal_set(dir.path(), &mut RecoveryBudget::new(ceilings)).unwrap();
    assert_eq!(plan.journal_count(), 2);
    drop(plan);
    assert_eq!(inventory(dir.path()), before);
}

#[test]
fn partition_options_admission_enforces_input_and_metadata_without_mutation() {
    use fjall::bounded_recovery::prepare_partition_options;
    let dir = tempfile::tempdir().unwrap();
    {
        let keyspace = Config::new(dir.path()).flush_workers(0).compaction_workers(0).open().unwrap();
        keyspace.open_partition("plain", fjall::PartitionCreateOptions::default()).unwrap();
    }
    let path = dir.path().join("partitions/plain");
    let size = std::fs::metadata(path.join("config")).unwrap().len();
    let before = inventory(dir.path());
    let mut exact = limits(128);
    exact.max_input_bytes = size;
    assert!(prepare_partition_options(&path, &mut RecoveryBudget::new(exact.clone())).is_ok());
    exact.max_input_bytes = size - 1;
    assert!(matches!(prepare_partition_options(&path, &mut RecoveryBudget::new(exact)), Err(RecoveryError::Limit { kind: RecoveryLimitKind::InputBytes, .. })));
    let mut no_metadata = limits(128);
    no_metadata.max_metadata_bytes = 0;
    assert!(matches!(prepare_partition_options(&path, &mut RecoveryBudget::new(no_metadata)), Err(RecoveryError::Limit { kind: RecoveryLimitKind::MetadataBytes, .. })));
    assert_eq!(inventory(dir.path()), before);
}

#[test]
fn partition_options_reject_blob_and_malformed_compression_before_mutation() {
    use fjall::bounded_recovery::prepare_partition_options;
    let dir = tempfile::tempdir().unwrap();
    {
        let keyspace = Config::new(dir.path()).flush_workers(0).compaction_workers(0).open().unwrap();
        keyspace.open_partition("plain", fjall::PartitionCreateOptions::default()).unwrap();
        keyspace.open_partition("blob", fjall::PartitionCreateOptions::default().with_kv_separation(Default::default())).unwrap();
    }
    let plain = dir.path().join("partitions/plain");
    let blob = dir.path().join("partitions/blob");
    let mut malformed = std::fs::read(plain.join("config")).unwrap();
    // Header(4), level/type(2), three u32 fields(12), then compression tag/padding.
    malformed[19] = 255;
    std::fs::write(plain.join("config"), malformed).unwrap();
    let before = inventory(dir.path());
    assert!(matches!(prepare_partition_options(blob, &mut RecoveryBudget::new(limits(128))), Err(RecoveryError::Unsupported(_))));
    assert!(matches!(prepare_partition_options(plain, &mut RecoveryBudget::new(limits(128))), Err(RecoveryError::Corruption(_))));
    assert_eq!(inventory(dir.path()), before);
}
