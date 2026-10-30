// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
use crate::{FjallBoundError, FjallReadBudget, FjallReadLimits};
use tempfile::TempDir;

fn limits(max_value_bytes: usize) -> FjallReadLimits {
    FjallReadLimits {
        max_value_bytes,
        max_block_encoded_bytes: 1_048_576,
        max_block_decoded_bytes: 1_048_576,
        max_input_bytes: 4_194_304,
        max_decode_bytes: 4_194_304,
        max_metadata_bytes: 4_194_304,
        max_alloc_bytes: 4_194_304,
        max_visited_items: 1000,
        max_key_bytes: 4096,
        max_output_items: 100,
        max_output_key_bytes: 65_536,
    }
}

#[test]
fn bounded_read_accepts_exact_size_and_refuses_oversize() {
    let dir = TempDir::new().unwrap();
    let store = FjallStore::open(dir.path()).unwrap();
    pollster::block_on(store.put("key", b"hello")).unwrap();
    let mut exact = FjallReadBudget::new(limits(5));
    assert_eq!(store.get_bounded("key", &limits(5), &mut exact).unwrap(), Some(b"hello".to_vec()));
    let mut small = FjallReadBudget::new(limits(4));
    assert!(matches!(store.get_bounded("key", &limits(4), &mut small), Err(FjallBoundError::Limit { .. })));
}

#[test]
fn bounded_read_missing_at_zero_value_limit_is_none() {
    let dir = TempDir::new().unwrap();
    let store = FjallStore::open(dir.path()).unwrap();
    let mut budget = FjallReadBudget::new(limits(0));
    assert_eq!(store.get_bounded("absent", &limits(0), &mut budget).unwrap(), None);
}

#[test]
fn prefix_probe_is_distinct_from_a_complete_bounded_list() {
    let dir = TempDir::new().unwrap();
    let store = FjallStore::open(dir.path()).unwrap();
    pollster::block_on(store.put("prefix-a", b"a")).unwrap();
    pollster::block_on(store.put("prefix-b", b"b")).unwrap();
    let mut call = limits(5);
    call.max_output_items = 1;
    let mut budget = FjallReadBudget::new(limits(5));
    assert!(matches!(store.list_bounded("prefix-", &call, &mut budget), Err(FjallBoundError::Limit { kind: crate::StoreLimitKind::OutputItems, .. })));
    let mut call = limits(5);
    call.max_output_items = 0;
    call.max_output_key_bytes = 0;
    let mut budget = FjallReadBudget::new(limits(5));
    assert!(store.exists_bounded("prefix-", &call, &mut budget).unwrap());
}

#[test]
fn bounded_owner_rejects_oversize_write_and_preflights_batches() {
    let dir = TempDir::new().unwrap();
    let mut store = FjallStore::open(dir.path()).unwrap();
    store.owner_limits = Some(crate::FjallOwnerLimits {
        read: limits(4),
        recovery: crate::FjallRecoveryLimits { max_value_bytes: 4, max_key_bytes: 4096,
            max_replay_bytes: 100_000, max_replay_items: 100, max_metadata_bytes: 1_000_000,
            max_input_bytes: 100_000, max_alloc_bytes: 1_000_000 },
    });
    pollster::block_on(store.put("good", b"four")).unwrap();
    assert!(matches!(pollster::block_on(store.put("bad", b"five!")), Err(crate::StoreError::LimitExceeded { kind: crate::StoreLimitKind::ValueBytes, limit: 4 })));
    let batch = [crate::WriteOp::Put { key: "first".into(), value: b"ok".to_vec() }, crate::WriteOp::Put { key: "second".into(), value: b"large".to_vec() }];
    assert!(matches!(pollster::block_on(store.apply(&batch)), Err(crate::StoreError::LimitExceeded { kind: crate::StoreLimitKind::ValueBytes, limit: 4 })));
    assert_eq!(pollster::block_on(store.get("first")).unwrap(), None);
}

fn owner_limits(read_value: usize, recovery_value: usize) -> crate::FjallOwnerLimits {
    let mut read = limits(read_value);
    read.max_input_bytes = 128 * 1024 * 1024;
    read.max_decode_bytes = 128 * 1024 * 1024;
    read.max_alloc_bytes = 128 * 1024 * 1024;
    read.max_metadata_bytes = 128 * 1024 * 1024;
    read.max_visited_items = 100_000;
    crate::FjallOwnerLimits { read, recovery: crate::FjallRecoveryLimits { max_value_bytes: recovery_value,
        max_key_bytes: 4096, max_replay_bytes: 64 * 1024 * 1024, max_replay_items: 100_000,
        max_metadata_bytes: 32 * 1024 * 1024, max_input_bytes: 128 * 1024 * 1024, max_alloc_bytes: 64 * 1024 * 1024 } }
}

#[test]
fn bounded_owner_reopens_and_ordinary_reads_obey_installed_limits() {
    let dir = TempDir::new().unwrap();
    {
        let store = FjallStore::open(dir.path()).unwrap();
        pollster::block_on(store.put("key", b"hello")).unwrap();
        store.sync_all().unwrap();
    }
    let store = FjallStore::open_with_limits(dir.path(), owner_limits(4, 5)).unwrap();
    assert!(matches!(pollster::block_on(store.get("key")), Err(crate::StoreError::LimitExceeded { kind: crate::StoreLimitKind::ValueBytes, limit: 4 })));
    assert!(matches!(pollster::block_on(store.list("k")), Err(crate::StoreError::LimitExceeded { kind: crate::StoreLimitKind::ValueBytes, limit: 4 })));
}

#[test]
fn bounded_owner_new_store_survives_sync_and_reopen() {
    let dir = TempDir::new().unwrap();
    {
        let store = FjallStore::open_with_limits(dir.path(), owner_limits(5, 5)).unwrap();
        pollster::block_on(store.put("key", b"hello")).unwrap();
        store.sync_all().unwrap();
    }
    let store = FjallStore::open_with_limits(dir.path(), owner_limits(5, 5)).unwrap();
    assert_eq!(pollster::block_on(store.get("key")).unwrap().unwrap(), b"hello");
}

#[test]
fn delete_and_batch_delete_preflight_recovery_keys_before_reopen() {
    let dir = TempDir::new().unwrap();
    let mut owner = owner_limits(5, 5);
    owner.recovery.max_key_bytes = DEFAULT_PARTITION.len() + 5;
    {
        let store = FjallStore::open_with_limits(dir.path(), owner.clone()).unwrap();
        pollster::block_on(store.put("good", b"ok")).unwrap();
        assert!(matches!(pollster::block_on(store.delete("longer")), Err(crate::StoreError::LimitExceeded { kind: crate::StoreLimitKind::KeyBytes, .. })));
        let batch = [crate::WriteOp::Put { key: "other".into(), value: b"ok".to_vec() }, crate::WriteOp::Delete { key: "longer".into() }];
        assert!(matches!(pollster::block_on(store.apply(&batch)), Err(crate::StoreError::LimitExceeded { kind: crate::StoreLimitKind::KeyBytes, .. })));
        assert_eq!(pollster::block_on(store.get("other")).unwrap(), None);
        store.sync_all().unwrap();
    }
    let store = FjallStore::open_with_limits(dir.path(), owner).unwrap();
    assert_eq!(pollster::block_on(store.get("good")).unwrap().unwrap(), b"ok");
}

#[test]
fn empty_prefix_includes_binary_boundary_keys_in_memory_and_on_disk() {
    let dir = TempDir::new().unwrap();
    let store = FjallStore::open(dir.path()).unwrap();
    store.partition.insert([0xff, 0x00], "v").unwrap();
    let cap = limits(5);
    for cold in [false, true] {
        if cold {
            store.partition.rotate_memtable_and_wait().unwrap();
        }
        assert!(store.exists_bounded("", &cap, &mut FjallReadBudget::new(cap.clone())).unwrap());
        assert!(matches!(store.list_bounded("", &cap, &mut FjallReadBudget::new(cap.clone())), Err(FjallBoundError::Store(crate::StoreError::Corruption(_)))));
    }
}
