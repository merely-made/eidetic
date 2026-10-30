// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::bounded_prepare::{prepare, PrepareError, RecoveryBudget, RecoveryResource};
use crate::{AbstractTree, Config};

struct Ledger { remaining: u64 }
impl RecoveryBudget for Ledger {
    fn charge(&mut self, resource: RecoveryResource, amount: u64) -> Result<(), PrepareError> {
        self.remaining = self.remaining.checked_sub(amount).ok_or(PrepareError::Limit { resource, limit: self.remaining })?;
        Ok(())
    }
    fn check_key_bytes(&mut self, amount: u64) -> Result<(), PrepareError> {
        self.charge(RecoveryResource::KeyBytes, amount)
    }
}

#[test]
fn admitted_existing_empty_tree_reads_after_commit() {
    let dir = tempfile::tempdir().unwrap();
    drop(Config::new(dir.path()).open().unwrap());
    let mut budget = Ledger { remaining: 1_000_000 };
    let prepared = prepare(Config::new(dir.path()), &mut budget).unwrap();
    let tree = prepared.commit().unwrap();
    assert_eq!(tree.get("missing", None).unwrap(), None);
}

#[test]
fn recovery_budget_refusal_preserves_existing_manifest() {
    let dir = tempfile::tempdir().unwrap();
    drop(Config::new(dir.path()).open().unwrap());
    let manifest = dir.path().join(crate::file::MANIFEST_FILE);
    let before = std::fs::read(&manifest).unwrap();
    let mut budget = Ledger { remaining: 0 };
    assert!(matches!(prepare(Config::new(dir.path()), &mut budget), Err(PrepareError::Limit { .. })));
    assert_eq!(std::fs::read(manifest).unwrap(), before);
}

#[test]
fn populated_tree_is_retained_and_orphan_cleanup_is_deferred() {
    let dir = tempfile::tempdir().unwrap();
    let tree = Config::new(dir.path()).open().unwrap();
    tree.insert("alpha", "value", 0);
    tree.flush_active_memtable(0).unwrap().unwrap();
    drop(tree);
    let orphan = dir.path().join(crate::file::SEGMENTS_FOLDER).join("999999");
    std::fs::write(&orphan, b"orphan").unwrap();
    let mut budget = Ledger { remaining: 2_000_000 };
    let prepared = prepare(Config::new(dir.path()), &mut budget).unwrap();
    assert!(orphan.exists());
    let tree = prepared.commit().unwrap();
    assert!(!orphan.exists());
    assert_eq!(tree.get("alpha", None).unwrap().unwrap().as_ref(), b"value");
}

#[test]
fn cloned_prepared_tree_keeps_descriptors_after_plan_drop() {
    let dir = tempfile::tempdir().unwrap();
    let tree = Config::new(dir.path()).open().unwrap();
    tree.insert("alpha", "value", 0);
    tree.flush_active_memtable(0).unwrap().unwrap();
    drop(tree);
    let prepared = prepare(Config::new(dir.path()), &mut Ledger { remaining: 2_000_000 }).unwrap();
    let retained = prepared.tree().clone();
    drop(prepared);
    assert_eq!(retained.get("alpha", None).unwrap().unwrap().as_ref(), b"value");
}

#[test]
fn missing_empty_segment_directory_is_created_only_at_commit() {
    let dir = tempfile::tempdir().unwrap();
    drop(Config::new(dir.path()).open().unwrap());
    let segments = dir.path().join(crate::file::SEGMENTS_FOLDER);
    std::fs::remove_dir(&segments).unwrap();
    let prepared = prepare(Config::new(dir.path()), &mut Ledger { remaining: 1_000_000 }).unwrap();
    assert!(!segments.exists());
    let tree = prepared.commit().unwrap();
    assert!(segments.is_dir());
    assert_eq!(tree.get("missing", None).unwrap(), None);
}

#[test]
fn disjoint_comparison_work_is_charged_before_recovery() {
    struct Visits(Vec<u64>);
    impl RecoveryBudget for Visits {
        fn charge(&mut self, resource: RecoveryResource, amount: u64) -> Result<(), PrepareError> {
            if resource == RecoveryResource::VisitedItems { self.0.push(amount); }
            Ok(())
        }
        fn check_key_bytes(&mut self, _: u64) -> Result<(), PrepareError> { Ok(()) }
    }
    let dir = tempfile::tempdir().unwrap();
    let tree = Config::new(dir.path()).open().unwrap();
    for i in 0..4 {
        tree.insert(format!("key-{i}"), "value", i);
        tree.flush_active_memtable(0).unwrap().unwrap();
    }
    drop(tree);
    let mut visits = Visits(Vec::new());
    let plan = prepare(Config::new(dir.path()), &mut visits).unwrap();
    assert!(visits.0.iter().any(|&amount| amount >= 16));
    drop(plan);
}

#[test]
fn replay_install_retains_active_and_sealed_before_commit() {
    use crate::{InternalValue, Memtable, ValueType};
    let dir = tempfile::tempdir().unwrap();
    drop(Config::new(dir.path()).open().unwrap());
    let mut prepared = prepare(Config::new(dir.path()), &mut Ledger { remaining: 1_000_000 }).unwrap();
    let sealed = Memtable::default();
    sealed.insert(InternalValue::from_components("sealed", "earlier", 1, ValueType::Value));
    let active = Memtable::default();
    active.insert(InternalValue::from_components("active", "later", 2, ValueType::Value));
    let retained = prepared.install_replay(Some(active), vec![sealed], &mut Ledger { remaining: 1_000_000 }).unwrap();
    assert_eq!(retained.len(), 1);
    assert_eq!(prepared.tree().get("sealed", None).unwrap().unwrap().as_ref(), b"earlier");
    assert_eq!(prepared.tree().get("active", None).unwrap().unwrap().as_ref(), b"later");
    let tree = prepared.commit().unwrap();
    assert_eq!(tree.get("sealed", None).unwrap().unwrap().as_ref(), b"earlier");
}

#[test]
fn refused_replay_install_leaves_tree_unchanged() {
    use crate::{InternalValue, Memtable, ValueType};
    let dir = tempfile::tempdir().unwrap();
    drop(Config::new(dir.path()).open().unwrap());
    let mut prepared = prepare(Config::new(dir.path()), &mut Ledger { remaining: 1_000_000 }).unwrap();
    let active = Memtable::default();
    active.insert(InternalValue::from_components("active", "later", 2, ValueType::Value));
    assert!(matches!(prepared.install_replay(Some(active), vec![], &mut Ledger { remaining: 0 }), Err(PrepareError::Limit { .. })));
    assert_eq!(prepared.tree().get("active", None).unwrap(), None);
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn deferred_cleanup_paths_are_precharged() {
    struct Allocation(u64);
    impl RecoveryBudget for Allocation {
        fn charge(&mut self, resource: RecoveryResource, amount: u64) -> Result<(), PrepareError> {
            if resource == RecoveryResource::AllocationBytes { self.0 += amount; }
            Ok(())
        }
        fn check_key_bytes(&mut self, _: u64) -> Result<(), PrepareError> { Ok(()) }
    }
    let long = std::path::PathBuf::from(format!("/{}", "x".repeat(400)));
    let mut budget = Allocation(0);
    crate::bounded_tree::precharge_commit_paths(Some(&long), &[long.clone()], &mut budget).unwrap();
    assert_eq!(budget.0, 3 * (long.as_os_str().as_encoded_bytes().len() as u64 + 1));
}

#[test]
fn prepared_tree_iterator_keeps_descriptors_after_tree_drop() {
    let dir = tempfile::tempdir().unwrap();
    let tree = Config::new(dir.path()).open().unwrap();
    tree.insert("alpha", "value", 0);
    tree.flush_active_memtable(0).unwrap().unwrap();
    drop(tree);
    let prepared = prepare(Config::new(dir.path()), &mut Ledger { remaining: 1_000_000 }).unwrap();
    let tree = prepared.tree().clone();
    let mut iter = tree.iter(None, None);
    drop(prepared);
    drop(tree);
    let (key, value) = iter.next().unwrap().unwrap();
    assert_eq!(key.as_ref(), b"alpha");
    assert_eq!(value.as_ref(), b"value");
}

#[cfg(feature = "bytes")]
#[test]
fn bytes_backed_prepare_refuses_before_mutation() {
    let dir = tempfile::tempdir().unwrap();
    drop(Config::new(dir.path()).open().unwrap());
    let manifest = dir.path().join(crate::file::MANIFEST_FILE);
    let before = std::fs::read(&manifest).unwrap();
    assert!(matches!(prepare(Config::new(dir.path()), &mut Ledger { remaining: 1_000_000 }), Err(PrepareError::Unsupported(_))));
    assert_eq!(std::fs::read(manifest).unwrap(), before);
}
