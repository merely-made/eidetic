// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::{coding::Encode, InternalValue, ValueType};
use crate::bounded_block::{read_data_block, read_data_block_budgeted, BlockLimits, BlockReadError};
use crate::segment::{block::offset::BlockOffset, meta::CompressionType, value_block::ValueBlock};
use std::io::{Cursor, Read};

fn block(compression: CompressionType) -> Vec<u8> {
    let values = [InternalValue::from_components("k", "value", 1, ValueType::Value)];
    let (header, packed) = ValueBlock::to_bytes_compressed(&values, BlockOffset(0), compression).unwrap();
    let mut result = Vec::new();
    header.encode_into(&mut result).unwrap();
    result.extend_from_slice(&packed);
    result
}

fn limits() -> BlockLimits {
    BlockLimits { max_encoded: 1024, max_decoded: 1024, max_value: 5, max_key: 8, max_items: 8, max_allocation: 8192 }
}

struct TrackingReader<'a> { inner: Cursor<&'a [u8]>, bytes_read: usize }
impl std::io::Seek for TrackingReader<'_> {
    fn seek(&mut self, position: std::io::SeekFrom) -> std::io::Result<u64> { self.inner.seek(position) }
}
impl Read for TrackingReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.bytes_read += n;
        Ok(n)
    }
}

#[test]
fn encoded_limit_refuses_before_reading_payload() {
    let bytes = block(CompressionType::None);
    let mut reader = TrackingReader { inner: Cursor::new(&bytes), bytes_read: 0 };
    let mut limit = limits();
    limit.max_encoded = 0;
    assert!(matches!(read_data_block(&mut reader, &limit), Err(BlockReadError::EncodedLimit)));
    assert_eq!(reader.bytes_read, crate::segment::block::header::Header::serialized_len());
}

#[test]
fn uncompressed_decoded_limit_refuses_before_reading_payload() {
    let bytes = block(CompressionType::None);
    let mut reader = TrackingReader { inner: Cursor::new(&bytes), bytes_read: 0 };
    let mut limit = limits();
    limit.max_decoded = 0;
    assert!(matches!(read_data_block(&mut reader, &limit), Err(BlockReadError::DecodedLimit)));
    assert_eq!(reader.bytes_read, crate::segment::block::header::Header::serialized_len());
}

#[test]
fn value_and_allocation_limits_refuse_before_materializing_entries() {
    let bytes = block(CompressionType::None);
    let mut limit = limits();
    limit.max_value = 4;
    assert!(matches!(read_data_block(&mut Cursor::new(&bytes), &limit), Err(BlockReadError::ValueLimit)));
    limit.max_value = 5;
    limit.max_allocation = 0;
    assert!(matches!(read_data_block(&mut Cursor::new(&bytes), &limit), Err(BlockReadError::AllocationLimit)));
    assert_eq!(read_data_block(&mut Cursor::new(&bytes), &limits()).unwrap().items[0].value.as_ref(), b"value");
}

#[test]
fn two_reads_share_cumulative_allocation_budget() {
    use crate::bounded_budget::{Budget, BudgetKind, ReadLimits};

    let bytes = block(CompressionType::None);
    let one_block_cost = bytes.len() - crate::segment::block::header::Header::serialized_len()
        + 2 * std::mem::size_of::<InternalValue>() + 2 * std::mem::size_of::<std::sync::atomic::AtomicU64>() + 1 + 5;
    let config = ReadLimits { max_value_bytes: 8, max_key_bytes: 8, max_block_encoded_bytes: 1024,
        max_block_decoded_bytes: 1024, max_input_bytes: 4096, max_decode_bytes: 4096,
        max_alloc_bytes: one_block_cost as u64, max_metadata_bytes: 4096, max_visited_items: 32,
        max_output_items: 32, max_output_key_bytes: 1024 };
    let mut owner = Budget::new(config.clone());
    let mut budget = Budget::new(config);
    read_data_block_budgeted(&mut Cursor::new(&bytes), &limits(), &mut owner, &mut budget).unwrap();
    let first = budget.used(BudgetKind::AllocationBytes);
    assert!(first > 0);
    assert!(read_data_block_budgeted(&mut Cursor::new(&bytes), &limits(), &mut owner, &mut budget).is_err());
    assert_eq!(budget.used(BudgetKind::AllocationBytes), first);
}

#[test]
fn owner_value_ceiling_cannot_be_relaxed_by_operation_budget() {
    use crate::bounded_budget::{Budget, ReadLimits};
    let bytes = block(CompressionType::None);
    let config = ReadLimits { max_value_bytes: 5, max_key_bytes: 8, max_block_encoded_bytes: 1024,
        max_block_decoded_bytes: 1024, max_input_bytes: 4096, max_decode_bytes: 4096,
        max_alloc_bytes: 8192, max_metadata_bytes: 4096, max_visited_items: 32,
        max_output_items: 32, max_output_key_bytes: 1024 };
    let mut owner = Budget::new(ReadLimits { max_value_bytes: 4, ..config.clone() });
    let mut operation = Budget::new(config);
    assert!(matches!(read_data_block_budgeted(&mut Cursor::new(bytes), &limits(), &mut owner, &mut operation), Err(BlockReadError::ValueLimit)));
}

#[cfg(feature = "lz4")]
#[test]
fn compressed_block_refuses_decoded_limit() {
    let bytes = block(CompressionType::Lz4);
    let mut limit = limits();
    limit.max_decoded = 4;
    assert!(matches!(read_data_block(&mut Cursor::new(&bytes), &limit), Err(BlockReadError::DecodedLimit)));
    assert_eq!(read_data_block(&mut Cursor::new(&bytes), &limits()).unwrap().items[0].value.as_ref(), b"value");
}

#[cfg(feature = "miniz")]
#[test]
fn miniz_block_obeys_decoded_and_allocation_limits() {
    let bytes = block(CompressionType::Miniz(6));
    let mut limit = limits();
    limit.max_decoded = 4;
    assert!(matches!(read_data_block(&mut Cursor::new(&bytes), &limit), Err(BlockReadError::DecodedLimit)));
    assert_eq!(read_data_block(&mut Cursor::new(&bytes), &limits()).unwrap().items[0].value.as_ref(), b"value");
}

#[test]
fn long_entry_heap_headers_are_counted_before_materialization() {
    let values = [InternalValue::from_components(vec![b'k'; 24], vec![b'v'; 24], 1, ValueType::Value)];
    let (header, packed) = ValueBlock::to_bytes_compressed(&values, BlockOffset(0), CompressionType::None).unwrap();
    let mut bytes = Vec::new();
    header.encode_into(&mut bytes).unwrap();
    bytes.extend_from_slice(&packed);
    let mut limit = limits();
    limit.max_key = 24;
    limit.max_value = 24;
    limit.max_allocation = packed.len() + 2 * std::mem::size_of::<InternalValue>() + 48;
    assert!(matches!(read_data_block(&mut Cursor::new(bytes), &limit), Err(BlockReadError::AllocationLimit)));
}

#[test]
fn malformed_entry_order_is_reported_as_corruption() {
    let values = [InternalValue::from_components("z", "one", 1, ValueType::Value), InternalValue::from_components("a", "two", 2, ValueType::Value)];
    let (header, packed) = ValueBlock::to_bytes_compressed(&values, BlockOffset(0), CompressionType::None).unwrap();
    let mut bytes = Vec::new();
    header.encode_into(&mut bytes).unwrap();
    bytes.extend_from_slice(&packed);
    assert!(matches!(read_data_block(&mut Cursor::new(bytes), &limits()), Err(BlockReadError::Corrupt)));
}

#[test]
fn truncated_encoded_block_refuses_before_payload_allocation() {
    use crate::bounded_budget::{Budget, BudgetKind, ReadLimits};
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
    assert!(matches!(read_data_block_budgeted(&mut Cursor::new(truncated), &limits(), &mut call, &mut operation), Err(BlockReadError::Corrupt)));
    assert_eq!(operation.used(BudgetKind::AllocationBytes), 0);
}

#[cfg(feature = "miniz")]
#[test]
fn miniz_data_uses_actual_cumulative_decoded_work() {
    use crate::bounded_budget::{Budget, BudgetKind, ReadLimits};
    let bytes = block(CompressionType::Miniz(6));
    let header_size = crate::segment::block::header::Header::serialized_len();
    let decoded = miniz_oxide::inflate::decompress_to_vec(&bytes[header_size..]).unwrap();
    let exact = decoded.len() as u64 + 1 + 5;
    let mut cap = ReadLimits::unlimited();
    cap.max_value_bytes = 5; cap.max_key_bytes = 8; cap.max_block_encoded_bytes = 1024;
    cap.max_block_decoded_bytes = 1_048_576; cap.max_alloc_bytes = 4_194_304;
    cap.max_decode_bytes = exact * 2;
    let mut operation = Budget::new(cap.clone());
    let mut one_call = Budget::new(cap.clone());
    let mut block_limits = limits();
    block_limits.max_decoded = 1_048_576; block_limits.max_allocation = 4_194_304;
    read_data_block_budgeted(&mut Cursor::new(&bytes), &block_limits, &mut one_call, &mut operation).unwrap();
    assert_eq!(operation.used(BudgetKind::DecodeBytes), exact);
    read_data_block_budgeted(&mut Cursor::new(&bytes), &block_limits, &mut Budget::new(cap.clone()), &mut operation).unwrap();
    assert_eq!(operation.used(BudgetKind::DecodeBytes), exact * 2);
    assert!(read_data_block_budgeted(&mut Cursor::new(&bytes), &block_limits, &mut Budget::new(cap.clone()), &mut operation).is_err());
    cap.max_decode_bytes = decoded.len() as u64 - 1;
    let mut short = Budget::new(cap.clone());
    assert!(read_data_block_budgeted(&mut Cursor::new(bytes), &block_limits, &mut Budget::new(cap), &mut short).is_err());
    assert_eq!(short.used(BudgetKind::DecodeBytes), decoded.len() as u64 - 1);
}

#[cfg(feature = "miniz")]
#[test]
fn miniz_data_crosses_multiple_bounded_output_steps() {
    use crate::bounded_budget::{Budget, BudgetKind, ReadLimits};
    let values = [InternalValue::from_components("key", vec![b'v'; 10_000], 1, ValueType::Value)];
    let (header, packed) = ValueBlock::to_bytes_compressed(&values, BlockOffset(0), CompressionType::Miniz(6)).unwrap();
    let expected = miniz_oxide::inflate::decompress_to_vec(&packed).unwrap().len() as u64 + 10_003;
    let mut bytes = Vec::new(); header.encode_into(&mut bytes).unwrap(); bytes.extend_from_slice(&packed);
    let mut cap = ReadLimits::unlimited(); cap.max_value_bytes = 10_000; cap.max_key_bytes = 3;
    cap.max_block_encoded_bytes = 100_000; cap.max_block_decoded_bytes = 20_000;
    cap.max_decode_bytes = expected; cap.max_alloc_bytes = 100_000;
    let mut local = limits(); local.max_value = 10_000; local.max_decoded = 20_000; local.max_allocation = 100_000;
    let mut shared = Budget::new(cap.clone());
    let result = read_data_block_budgeted(&mut Cursor::new(bytes), &local, &mut Budget::new(cap), &mut shared).unwrap();
    assert_eq!(result.items[0].value.len(), 10_000);
    assert_eq!(shared.used(BudgetKind::DecodeBytes), expected);
    assert!(shared.used(BudgetKind::AllocationBytes) > 10_000);
}
