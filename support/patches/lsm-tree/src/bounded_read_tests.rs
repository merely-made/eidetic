// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::{AbstractTree, Config};
use crate::bounded_budget::{Budget, ReadLimits};
use crate::bounded_read::{get, scan, ReadError};

fn limits(value: usize) -> ReadLimits {
    ReadLimits { max_value_bytes: value, max_key_bytes: 1024, max_block_encoded_bytes: 65536,
        max_block_decoded_bytes: 65536, max_input_bytes: 1_000_000, max_decode_bytes: 1_000_000,
        max_alloc_bytes: 1_000_000, max_metadata_bytes: 1_000_000, max_visited_items: 10000,
        max_output_items: 100, max_output_key_bytes: 10000 }
}

#[test]
fn bounded_get_checks_memory_and_cold_segment_values() {
    let dir = tempfile::tempdir().unwrap();
    let tree = Config::new(dir.path()).open().unwrap();
    tree.insert("key", "12345", 1);
    let mut call = Budget::new(limits(4));
    let mut operation = Budget::new(limits(4));
    assert!(matches!(get(&tree, b"key", None, &mut call, &mut operation), Err(ReadError::ValueLimit { limit: 4 })));
    tree.flush_active_memtable(0).unwrap().unwrap();
    let mut call = Budget::new(limits(4));
    let mut operation = Budget::new(limits(4));
    assert!(matches!(get(&tree, b"key", None, &mut call, &mut operation), Err(ReadError::ValueLimit { limit: 4 })));
    let mut call = Budget::new(limits(5));
    let mut operation = Budget::new(limits(5));
    assert_eq!(get(&tree, b"key", None, &mut call, &mut operation).unwrap().unwrap().as_ref(), b"12345");
}

#[test]
fn bounded_scan_refuses_truncation_and_respects_deletes() {
    let dir = tempfile::tempdir().unwrap();
    let tree = Config::new(dir.path()).open().unwrap();
    tree.insert("prefix-a", "a", 1);
    tree.insert("prefix-b", "b", 2);
    tree.insert("prefix-c", "c", 3);
    tree.remove("prefix-b", 4);
    tree.flush_active_memtable(0).unwrap().unwrap();
    let mut one = limits(5);
    one.max_output_items = 1;
    assert!(scan(&tree, b"prefix-", Some(b"prefix."), None, &mut Budget::new(one.clone()), &mut Budget::new(one)).is_err());
    let good = limits(5);
    let keys = scan(&tree, b"prefix-", Some(b"prefix."), None, &mut Budget::new(good.clone()), &mut Budget::new(good)).unwrap();
    assert_eq!(keys.iter().map(AsRef::as_ref).collect::<Vec<&[u8]>>(), vec![b"prefix-a", b"prefix-c"]);
}

#[test]
fn bounded_get_ignores_unrelated_larger_memory_value() {
    let dir = tempfile::tempdir().unwrap();
    let tree = Config::new(dir.path()).open().unwrap();
    tree.insert("a", "ok", 1);
    tree.insert("z", "oversized", 2);
    let mut call = Budget::new(limits(2));
    let mut operation = Budget::new(limits(2));
    assert_eq!(get(&tree, b"a", None, &mut call, &mut operation).unwrap().unwrap().as_ref(), b"ok");
}

#[test]
fn unconstrained_call_limits_never_relax_operation_limits() {
    let dir = tempfile::tempdir().unwrap();
    let tree = Config::new(dir.path()).open().unwrap();
    tree.insert("key", "12345", 1);
    let mut call = Budget::new(ReadLimits::unlimited());
    let mut operation = Budget::new(limits(4));
    assert!(matches!(get(&tree, b"key", None, &mut call, &mut operation), Err(ReadError::ValueLimit { limit: 4 })));
}

#[test]
fn cold_cached_blocks_and_snapshot_versions_obey_limits() {
    let dir = tempfile::tempdir().unwrap();
    {
        let tree = Config::new(dir.path()).open().unwrap();
        tree.insert("key", "old", 1);
        tree.flush_active_memtable(0).unwrap().unwrap();
    }
    let tree = Config::new(dir.path()).open().unwrap();
    assert_eq!(tree.get("key", None).unwrap().unwrap().as_ref(), b"old");
    let mut small = limits(2);
    assert!(matches!(get(&tree, b"key", None, &mut Budget::new(small.clone()), &mut Budget::new(small.clone())), Err(ReadError::ValueLimit { limit: 2 })));
    tree.insert("key", "new", 3);
    small.max_value_bytes = 3;
    assert_eq!(get(&tree, b"key", Some(3), &mut Budget::new(small.clone()), &mut Budget::new(small)).unwrap().unwrap().as_ref(), b"old");
}

#[test]
fn repeated_calls_cannot_reset_the_shared_item_budget() {
    use crate::bounded_budget::BudgetKind;
    let dir = tempfile::tempdir().unwrap();
    let tree = Config::new(dir.path()).open().unwrap();
    tree.insert("key", "ok", 1);
    let mut constrained = limits(2);
    constrained.max_visited_items = 1;
    let mut operation = Budget::new(constrained.clone());
    get(&tree, b"key", None, &mut Budget::new(constrained.clone()), &mut operation).unwrap();
    let used = operation.used(BudgetKind::VisitedItems);
    assert_eq!(used, 1);
    assert!(matches!(get(&tree, b"key", None, &mut Budget::new(constrained), &mut operation), Err(ReadError::Budget(_))));
    assert_eq!(operation.used(BudgetKind::VisitedItems), used);
}

#[test]
fn prefix_presence_does_not_require_output_list_capacity() {
    use crate::bounded_read::exists;
    let dir = tempfile::tempdir().unwrap();
    let tree = Config::new(dir.path()).open().unwrap();
    tree.insert("prefix-1", "ok", 1);
    let mut constrained = limits(2);
    constrained.max_output_items = 0;
    constrained.max_output_key_bytes = 0;
    assert!(exists(&tree, b"prefix-", Some(b"prefix."), None, &mut Budget::new(constrained.clone()), &mut Budget::new(constrained)).unwrap());
}

#[test]
fn bounded_reads_fetch_nonresident_index_under_the_same_ledger() {
    let dir = tempfile::tempdir().unwrap();
    {
        let tree = Config::new(dir.path()).open().unwrap();
        tree.insert("key", "value", 1);
        let segment = tree.flush_active_memtable(0).unwrap().unwrap();
        tree.levels.write().unwrap().atomic_swap(|levels| {
            let moved = levels[0].remove(segment.id()).unwrap();
            levels[2].insert(moved);
        }).unwrap();
    }
    let tree = Config::new(dir.path()).open().unwrap();
    let good = limits(5);
    assert_eq!(get(&tree, b"key", None, &mut Budget::new(good.clone()), &mut Budget::new(good.clone())).unwrap().unwrap().as_ref(), b"value");
    let mut small = good;
    small.max_block_encoded_bytes = 0;
    assert!(matches!(get(&tree, b"key", None, &mut Budget::new(small.clone()), &mut Budget::new(small)), Err(ReadError::EncodedLimit { limit: 0 })));
}

#[test]
fn corrupt_data_block_checksum_is_not_returned_as_a_value() {
    use std::io::{Read, Seek, SeekFrom, Write};
    use crate::coding::Decode;
    use crate::segment::block::header::Header;
    let dir = tempfile::tempdir().unwrap();
    let path;
    {
        let tree = Config::new(dir.path()).open().unwrap();
        tree.insert("key", "value", 1);
        path = tree.flush_active_memtable(0).unwrap().unwrap().path.clone();
    }
    {
        let mut file = std::fs::OpenOptions::new().read(true).write(true).open(path).unwrap();
        let header = Header::decode_from(&mut file).unwrap();
        let position = Header::serialized_len() as u64 + u64::from(header.data_length) - 1;
        file.seek(SeekFrom::Start(position)).unwrap();
        let mut byte = [0];
        file.read_exact(&mut byte).unwrap();
        file.seek(SeekFrom::Start(position)).unwrap();
        file.write_all(&[byte[0] ^ 1]).unwrap();
    }
    let tree = Config::new(dir.path()).open().unwrap();
    let limits = limits(5);
    assert!(matches!(get(&tree, b"key", None, &mut Budget::new(limits.clone()), &mut Budget::new(limits)), Err(ReadError::Corruption)));
}

#[test]
fn moderate_scan_uses_bounded_n_log_n_merge_work() {
    let dir = tempfile::tempdir().unwrap();
    let tree = Config::new(dir.path()).open().unwrap();
    for n in 0..1100 { tree.insert(format!("k-{n:04}"), "v", n); }
    let mut cap = limits(1);
    cap.max_visited_items = 50_000;
    cap.max_output_items = 1200;
    cap.max_decode_bytes = 5_000_000;
    cap.max_alloc_bytes = 5_000_000;
    cap.max_metadata_bytes = 5_000_000;
    let keys = scan(&tree, b"k-", Some(b"k."), None, &mut Budget::new(cap.clone()), &mut Budget::new(cap)).unwrap();
    assert_eq!(keys.len(), 1100);
    assert_eq!(keys[0].as_ref(), b"k-0000");
    assert_eq!(keys[1099].as_ref(), b"k-1099");
}

#[cfg(feature = "bytes")]
#[test]
fn bytes_backed_read_is_explicitly_unsupported() {
    let dir = tempfile::tempdir().unwrap();
    let tree = Config::new(dir.path()).open().unwrap();
    tree.insert("key", "value", 0);
    let cap = limits(5);
    assert!(matches!(get(&tree, b"key", None, &mut Budget::new(cap.clone()), &mut Budget::new(cap)), Err(ReadError::Unsupported(_))));
}

#[test]
fn resident_work_charges_every_entry_before_its_bytes_are_read() {
    use crate::bounded_budget::BudgetKind;
    let dir = tempfile::tempdir().unwrap();
    let tree = Config::new(dir.path()).open().unwrap();
    tree.insert("a", "old", 1);
    tree.insert("a", "new", 2);
    tree.insert("z", "extra", 3);
    let mut cap = limits(8);
    cap.max_decode_bytes = 0;
    let mut budget = Budget::new(cap.clone());
    assert!(matches!(get(&tree, b"a", None, &mut Budget::new(cap.clone()), &mut budget), Err(ReadError::Budget(e)) if e.kind == BudgetKind::DecodeBytes));
    assert_eq!(budget.used(BudgetKind::DecodeBytes), 0);
    // All three physical entries cost 1+3, 1+3, and 1+5 bytes, including
    // the shadowed and nonmatching records. Sorting two matches costs four.
    cap.max_decode_bytes = 18;
    let mut budget = Budget::new(cap.clone());
    assert_eq!(get(&tree, b"a", None, &mut Budget::new(cap.clone()), &mut budget).unwrap().unwrap().as_ref(), b"new");
    assert_eq!(budget.used(BudgetKind::DecodeBytes), 18);
    assert!(matches!(get(&tree, b"missing", None, &mut Budget::new(cap), &mut budget), Err(ReadError::Budget(e)) if e.kind == BudgetKind::DecodeBytes));
    assert_eq!(budget.used(BudgetKind::DecodeBytes), 18);
}

#[test]
fn prefix_presence_refuses_if_later_range_work_exceeds_the_budget() {
    use crate::bounded_budget::BudgetKind;
    use crate::bounded_read::exists;
    let dir = tempfile::tempdir().unwrap();
    let tree = Config::new(dir.path()).open().unwrap();
    tree.insert("p-a", "v", 1);
    tree.insert("p-b", "v", 2);
    let mut cap = limits(1); cap.max_visited_items = 1;
    assert!(matches!(exists(&tree, b"p-", Some(b"p."), None, &mut Budget::new(cap.clone()), &mut Budget::new(cap)), Err(ReadError::Budget(e)) if e.kind == BudgetKind::VisitedItems));
}
