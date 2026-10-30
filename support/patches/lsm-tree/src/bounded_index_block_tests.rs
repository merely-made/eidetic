// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::bounded_index_block::read_index_block;
use crate::bounded_prepare::{PrepareError, RecoveryBudget, RecoveryResource};
use crate::coding::Encode;
use crate::segment::{block::offset::BlockOffset, block_index::{block_handle::KeyedBlockHandle, IndexBlock}, meta::CompressionType};
use std::io::{Cursor, Read};

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
fn block(compression: CompressionType) -> Vec<u8> {
    let entries = [KeyedBlockHandle::new("key", BlockOffset(5))];
    let (header, packed) = IndexBlock::to_bytes_compressed(&entries, BlockOffset(0), compression).unwrap();
    let mut bytes = Vec::new();
    header.encode_into(&mut bytes).unwrap();
    bytes.extend_from_slice(&packed);
    bytes
}
struct TrackingReader<'a> { inner: Cursor<&'a [u8]>, bytes_read: usize }
impl Read for TrackingReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.bytes_read += n;
        Ok(n)
    }
}
impl std::io::Seek for TrackingReader<'_> {
    fn seek(&mut self, pos: std::io::SeekFrom) -> std::io::Result<u64> { self.inner.seek(pos) }
}

#[test]
fn index_block_checks_encoded_budget_before_payload_read() {
    let bytes = block(CompressionType::None);
    let mut reader = TrackingReader { inner: Cursor::new(&bytes), bytes_read: 0 };
    let mut budget = Ledger { remaining: crate::segment::block::header::Header::serialized_len() as u64, max_key: 8 };
    assert!(matches!(read_index_block(&mut reader, &mut budget), Err(PrepareError::Limit { .. })));
    assert_eq!(reader.bytes_read, crate::segment::block::header::Header::serialized_len());
}

#[test]
fn index_block_decodes_existing_uncompressed_data() {
    let bytes = block(CompressionType::None);
    let result = read_index_block(&mut Cursor::new(bytes), &mut Ledger { remaining: 4096, max_key: 3 }).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].end_key.as_ref(), b"key");
}

#[cfg(feature = "lz4")]
#[test]
fn index_block_decodes_existing_lz4_data() {
    let bytes = block(CompressionType::Lz4);
    let result = read_index_block(&mut Cursor::new(bytes), &mut Ledger { remaining: 4096, max_key: 3 }).unwrap();
    assert_eq!(result.len(), 1);
}

#[cfg(feature = "miniz")]
#[test]
fn index_block_decodes_existing_miniz_data() {
    let bytes = block(CompressionType::Miniz(6));
    let result = read_index_block(&mut Cursor::new(bytes), &mut Ledger { remaining: 4096, max_key: 3 }).unwrap();
    assert_eq!(result.len(), 1);
}

#[test]
fn read_index_checks_physical_length_before_allocating_payload() {
    use crate::bounded_budget::{Budget, BudgetKind, ReadLimits};
    use crate::bounded_read::ReadError;
    use crate::coding::Decode;
    let bytes = block(CompressionType::None);
    let mut header = crate::segment::block::header::Header::decode_from(&mut Cursor::new(bytes)).unwrap();
    header.data_length = 100;
    let mut truncated = Vec::new();
    header.encode_into(&mut truncated).unwrap();
    let mut config = ReadLimits::unlimited();
    config.max_block_encoded_bytes = 1024;
    config.max_block_decoded_bytes = 1024;
    let mut call = Budget::new(config.clone());
    let mut operation = Budget::new(config);
    assert!(matches!(crate::bounded_read_index::read_index(&mut Cursor::new(truncated), &mut call, &mut operation), Err(ReadError::Corruption)));
    assert_eq!(operation.used(BudgetKind::AllocationBytes), 0);
}

#[test]
fn read_index_rejects_length_past_its_metadata_boundary_before_allocation() {
    use crate::bounded_budget::{Budget, BudgetKind, ReadLimits};
    use crate::bounded_read::ReadError;
    let bytes = block(CompressionType::None);
    let boundary = crate::segment::block::header::Header::serialized_len() as u64;
    let mut call = Budget::new(ReadLimits::unlimited());
    let mut operation = Budget::new(ReadLimits::unlimited());
    assert!(matches!(crate::bounded_read_index::read_index_until(&mut Cursor::new(bytes), &mut call, &mut operation, Some(boundary)), Err(ReadError::Corruption)));
    assert_eq!(operation.used(BudgetKind::AllocationBytes), 0);
}

#[test]
fn recovery_index_extent_is_checked_before_payload_allocation() {
    let bytes = block(CompressionType::None);
    let boundary = crate::segment::block::header::Header::serialized_len() as u64;
    let mut ledger = Ledger { remaining: 4096, max_key: 8 };
    assert!(matches!(crate::bounded_index_block::read_index_block_until(&mut Cursor::new(bytes), &mut ledger, Some(boundary)), Err(PrepareError::Corruption(_))));
    assert_eq!(ledger.remaining, 4096 - boundary);
}

#[cfg(feature = "miniz")]
#[test]
fn miniz_index_uses_actual_cumulative_decoded_work() {
    use crate::bounded_budget::{Budget, BudgetKind, ReadLimits};
    let bytes = block(CompressionType::Miniz(6));
    let header_size = crate::segment::block::header::Header::serialized_len();
    let decoded = miniz_oxide::inflate::decompress_to_vec(&bytes[header_size..]).unwrap();
    let exact = decoded.len() as u64 * 3;
    let mut cap = ReadLimits::unlimited();
    cap.max_block_encoded_bytes = 1024; cap.max_block_decoded_bytes = 1_048_576;
    cap.max_alloc_bytes = 4_194_304; cap.max_metadata_bytes = 4_194_304; cap.max_decode_bytes = exact * 2;
    let mut operation = Budget::new(cap.clone());
    crate::bounded_read_index::read_index(&mut Cursor::new(&bytes), &mut Budget::new(cap.clone()), &mut operation).unwrap();
    assert_eq!(operation.used(BudgetKind::DecodeBytes), exact);
    crate::bounded_read_index::read_index(&mut Cursor::new(&bytes), &mut Budget::new(cap.clone()), &mut operation).unwrap();
    assert_eq!(operation.used(BudgetKind::DecodeBytes), exact * 2);
    assert!(crate::bounded_read_index::read_index(&mut Cursor::new(&bytes), &mut Budget::new(cap.clone()), &mut operation).is_err());
    cap.max_decode_bytes = decoded.len() as u64 - 1;
    let mut short = Budget::new(cap.clone());
    assert!(crate::bounded_read_index::read_index(&mut Cursor::new(bytes), &mut Budget::new(cap), &mut short).is_err());
    assert_eq!(short.used(BudgetKind::DecodeBytes), decoded.len() as u64 - 1);
}

#[test]
fn recovery_index_header_cannot_cross_metadata_boundary() {
    let bytes = block(CompressionType::None);
    let mut reader = TrackingReader { inner: Cursor::new(&bytes), bytes_read: 0 };
    let mut ledger = Ledger { remaining: 4096, max_key: 8 };
    let boundary = crate::segment::block::header::Header::serialized_len() as u64 - 1;
    assert!(matches!(crate::bounded_index_block::read_index_block_until(&mut reader, &mut ledger, Some(boundary)), Err(PrepareError::Corruption(_))));
    assert_eq!(reader.bytes_read, 0);
}

#[test]
fn malformed_index_failure_consumes_shared_visit_budget() {
    use crate::bounded_budget::{Budget, BudgetKind, ReadLimits};
    use crate::bounded_read::ReadError;
    let entries = [KeyedBlockHandle::new("z", BlockOffset(5)), KeyedBlockHandle::new("a", BlockOffset(15))];
    let (header, packed) = IndexBlock::to_bytes_compressed(&entries, BlockOffset(0), CompressionType::None).unwrap();
    let mut bytes = Vec::new(); header.encode_into(&mut bytes).unwrap(); bytes.extend_from_slice(&packed);
    let mut cap = ReadLimits::unlimited(); cap.max_visited_items = 2;
    let mut shared = Budget::new(cap.clone());
    assert!(matches!(crate::bounded_read_index::read_index(&mut Cursor::new(&bytes), &mut Budget::new(cap.clone()), &mut shared), Err(ReadError::Corruption)));
    assert_eq!(shared.used(BudgetKind::VisitedItems), 2);
    assert!(matches!(crate::bounded_read_index::read_index(&mut Cursor::new(bytes), &mut Budget::new(cap), &mut shared), Err(ReadError::Budget(e)) if e.kind == BudgetKind::VisitedItems));
    assert_eq!(shared.used(BudgetKind::VisitedItems), 2);
}
