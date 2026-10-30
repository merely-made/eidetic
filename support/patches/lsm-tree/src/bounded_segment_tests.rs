// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::{AbstractTree, Config};
use crate::bounded_prepare::{PrepareError, RecoveryBudget, RecoveryResource};
use crate::bounded_segment::prepare_segment;

struct Ledger { remaining: u64 }
impl RecoveryBudget for Ledger {
    fn charge(&mut self, resource: RecoveryResource, amount: u64) -> Result<(), PrepareError> {
        self.remaining = self.remaining.checked_sub(amount).ok_or(PrepareError::Limit { resource, limit: self.remaining })?;
        Ok(())
    }
    fn check_key_bytes(&mut self, _: u64) -> Result<(), PrepareError> { Ok(()) }
}

#[test]
fn prepares_real_segment_without_mutating_it() {
    let dir = tempfile::tempdir().unwrap();
    let config = Config::new(dir.path());
    let tree = config.clone().open().unwrap();
    tree.insert("alpha", "value", 0);
    let original = tree.flush_active_memtable(0).unwrap().unwrap();
    let before = std::fs::read(&original.path).unwrap();
    let segment = prepare_segment(&original.path, 91, config.cache.clone(), config.descriptor_table.clone(), &mut Ledger { remaining: 1_000_000 }).unwrap();
    assert_eq!(segment.id(), original.id());
    assert_eq!(segment.metadata.key_range.min().as_ref(), b"alpha");
    drop(segment);
    assert_eq!(std::fs::read(&original.path).unwrap(), before);
}

#[test]
fn segment_limit_refusal_does_not_mutate_segment() {
    let dir = tempfile::tempdir().unwrap();
    let config = Config::new(dir.path());
    let tree = config.clone().open().unwrap();
    tree.insert("alpha", "value", 0);
    let original = tree.flush_active_memtable(0).unwrap().unwrap();
    let before = std::fs::read(&original.path).unwrap();
    assert!(matches!(prepare_segment(&original.path, 91, config.cache.clone(), config.descriptor_table.clone(), &mut Ledger { remaining: 0 }), Err(PrepareError::Limit { .. })));
    assert_eq!(std::fs::read(&original.path).unwrap(), before);
}

fn invalid_index_pointer_is_refused(items: usize) {
    use crate::{coding::{Decode, Encode}, segment::{block::offset::BlockOffset, block_index::IndexBlock}};
    use std::io::{Seek, SeekFrom, Write};
    let dir = tempfile::tempdir().unwrap();
    let config = Config::new(dir.path()).data_block_size(1024);
    let tree = config.clone().open().unwrap();
    for n in 0..items { tree.insert(format!("key-{n:04}"), vec![b'v'; if items == 1 { 5 } else { 700 }], n as u64); }
    let original = tree.flush_active_memtable(0).unwrap().unwrap();
    let path = original.path.clone();
    let index_start = *original.offsets.index_block_ptr;
    let mut file = std::fs::OpenOptions::new().read(true).write(true).open(&path).unwrap();
    file.seek(SeekFrom::Start(index_start)).unwrap();
    let block = IndexBlock::from_reader(&mut file).unwrap();
    let mut entries = block.items.into_vec();
    let last = entries.last_mut().unwrap();
    last.offset = BlockOffset(index_start);
    let (header, packed) = IndexBlock::to_bytes_compressed(&entries, block.header.previous_block_offset, block.header.compression).unwrap();
    assert_eq!(header.data_length, block.header.data_length);
    file.seek(SeekFrom::Start(index_start)).unwrap();
    header.encode_into(&mut file).unwrap();
    file.write_all(&packed).unwrap();
    drop(file);
    let before = std::fs::read(&path).unwrap();
    assert!(matches!(prepare_segment(&path, 91, config.cache.clone(), config.descriptor_table.clone(), &mut Ledger { remaining: 1_000_000 }), Err(PrepareError::Corruption(_))));
    assert_eq!(std::fs::read(path).unwrap(), before);
}

#[test]
fn single_and_last_index_pointers_are_validated_during_prepare() {
    invalid_index_pointer_is_refused(1);
    invalid_index_pointer_is_refused(4);
}
