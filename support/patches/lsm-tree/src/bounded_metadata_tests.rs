// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::bounded_metadata::parse_metadata;
use crate::bounded_prepare::{PrepareError, RecoveryBudget, RecoveryResource};
use crate::coding::Encode;
use crate::segment::meta::{CompressionType, Metadata, TableType};
use crate::KeyRange;

struct Ledger { remaining: u64, max_key: u64 }
impl RecoveryBudget for Ledger {
    fn charge(&mut self, resource: RecoveryResource, amount: u64) -> Result<(), PrepareError> {
        self.remaining = self.remaining.checked_sub(amount).ok_or(PrepareError::Limit { resource, limit: self.remaining })?;
        Ok(())
    }
    fn check_key_bytes(&mut self, amount: u64) -> Result<(), PrepareError> {
        if amount > self.max_key {
            return Err(PrepareError::Limit { resource: RecoveryResource::KeyBytes, limit: self.max_key });
        }
        Ok(())
    }
}

fn encoded() -> Vec<u8> {
    Metadata { id: 7, created_at: 5, item_count: 1, key_count: 1, tombstone_count: 0,
        range_tombstone_count: 0, file_size: 42, uncompressed_size: 42,
        data_block_size: 4096, index_block_size: 4096, data_block_count: 1,
        index_block_count: 1, compression: CompressionType::None, table_type: TableType::Block,
        seqnos: (1, 1), key_range: KeyRange::new(("abc".into(), "defg".into())) }.encode_into_vec()
}

#[test]
fn parses_existing_metadata_at_key_boundary() {
    let metadata = parse_metadata(&encoded(), &mut Ledger { remaining: 4096, max_key: 4 }).unwrap();
    assert_eq!(metadata.id, 7);
    assert_eq!(metadata.key_range.max().as_ref(), b"defg");
}

#[test]
fn rejects_key_and_truncated_metadata_before_copy() {
    let mut ledger = Ledger { remaining: 4096, max_key: 3 };
    assert!(matches!(parse_metadata(&encoded(), &mut ledger), Err(PrepareError::Limit { resource: RecoveryResource::KeyBytes, .. })));
    assert_eq!(ledger.remaining, 4096);
    let mut bytes = encoded();
    bytes.pop();
    assert!(matches!(parse_metadata(&bytes, &mut Ledger { remaining: 4096, max_key: 4 }), Err(PrepareError::Corruption(_))));
}

#[test]
fn long_key_heap_headers_are_in_allocation_admission() {
    let mut metadata = parse_metadata(&encoded(), &mut Ledger { remaining: 4096, max_key: 100 }).unwrap();
    metadata.key_range = KeyRange::new((vec![b'a'; 24].into(), vec![b'z'; 24].into()));
    struct AllocationCap(u64);
    impl RecoveryBudget for AllocationCap {
        fn charge(&mut self, resource: RecoveryResource, amount: u64) -> Result<(), PrepareError> {
            if resource == RecoveryResource::AllocationBytes {
                self.0 = self.0.checked_sub(amount).ok_or(PrepareError::Limit { resource, limit: self.0 })?;
            }
            Ok(())
        }
        fn check_key_bytes(&mut self, _: u64) -> Result<(), PrepareError> { Ok(()) }
    }
    assert!(matches!(parse_metadata(&metadata.encode_into_vec(), &mut AllocationCap(48)), Err(PrepareError::Limit { resource: RecoveryResource::AllocationBytes, .. })));
}
