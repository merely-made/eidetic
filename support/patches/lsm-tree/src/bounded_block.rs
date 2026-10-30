// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Explicitly bounded v2 data-block decoding.

use crate::InternalValue;
use crate::bounded_budget::{Budget, BudgetError, BudgetKind};
use crate::bounded_validation::{ValidationError, validate_data_entries};
use crate::coding::Decode;
use crate::segment::{block::header::Header, meta::CompressionType, value_block::ValueBlock};
use std::io::{Cursor, Read, Seek, SeekFrom};

pub(crate) struct BlockLimits {
    pub(crate) max_encoded: usize,
    pub(crate) max_decoded: usize,
    pub(crate) max_value: usize,
    pub(crate) max_key: usize,
    pub(crate) max_items: u32,
    pub(crate) max_allocation: usize,
}

#[derive(Debug)]
pub(crate) enum BlockReadError {
    Io(std::io::Error),
    Corrupt,
    UnsupportedCodec,
    EncodedLimit,
    DecodedLimit,
    ValueLimit,
    KeyLimit,
    ItemLimit,
    AllocationLimit,
    Budget(BudgetError),
}

fn allocate_zeroed(len: usize) -> Result<Vec<u8>, BlockReadError> {
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(len)
        .map_err(|_| BlockReadError::AllocationLimit)?;
    bytes.resize(len, 0);
    Ok(bytes)
}

#[cfg(test)]
pub(crate) fn read_data_block<R: Read + Seek>(
    reader: &mut R,
    limits: &BlockLimits,
) -> Result<ValueBlock, BlockReadError> {
    let mut owner = Budget::new(crate::bounded_budget::ReadLimits::unlimited());
    let mut operation = Budget::new(crate::bounded_budget::ReadLimits::unlimited());
    read_data_block_budgeted_until(reader, limits, &mut owner, &mut operation, None)
}

#[cfg(test)]
pub(crate) fn read_data_block_budgeted<R: Read + Seek>(
    reader: &mut R,
    limits: &BlockLimits,
    owner: &mut Budget,
    operation: &mut Budget,
) -> Result<ValueBlock, BlockReadError> {
    read_data_block_budgeted_until(reader, limits, owner, operation, None)
}

pub(crate) fn read_data_block_budgeted_until<R: Read + Seek>(
    reader: &mut R,
    limits: &BlockLimits,
    owner: &mut Budget,
    operation: &mut Budget,
    end: Option<u64>,
) -> Result<ValueBlock, BlockReadError> {
    let effective = BlockLimits {
        max_encoded: limits
            .max_encoded
            .min(owner.limits.max_block_encoded_bytes)
            .min(operation.limits.max_block_encoded_bytes),
        max_decoded: limits
            .max_decoded
            .min(owner.limits.max_block_decoded_bytes)
            .min(operation.limits.max_block_decoded_bytes),
        max_value: limits
            .max_value
            .min(owner.limits.max_value_bytes)
            .min(operation.limits.max_value_bytes),
        max_key: limits
            .max_key
            .min(owner.limits.max_key_bytes)
            .min(operation.limits.max_key_bytes),
        max_items: limits.max_items,
        max_allocation: limits.max_allocation,
    };
    read_data_block_inner(reader, &effective, end, owner, operation)
}

fn charge(
    owner: &mut Budget,
    operation: &mut Budget,
    kind: BudgetKind,
    amount: u64,
) -> Result<(), BlockReadError> {
    Budget::charge_both(owner, operation, kind, amount).map_err(BlockReadError::Budget)
}

fn read_data_block_inner<R: Read + Seek>(
    reader: &mut R,
    limits: &BlockLimits,
    extent_end: Option<u64>,
    owner: &mut Budget,
    operation: &mut Budget,
) -> Result<ValueBlock, BlockReadError> {
    let mut header_bytes = [0_u8; Header::serialized_len()];
    charge(
        owner,
        operation,
        BudgetKind::InputBytes,
        header_bytes.len() as u64,
    )?;
    reader
        .read_exact(&mut header_bytes)
        .map_err(BlockReadError::Io)?;
    if header_bytes.get(..4) != Some(crate::file::MAGIC_BYTES.as_slice()) {
        return Err(BlockReadError::Corrupt);
    }
    match (header_bytes[4], header_bytes[5]) {
        (0, 0) => {}
        #[cfg(feature = "lz4")]
        (1, 0) => {}
        #[cfg(feature = "miniz")]
        (2, level) if level <= 10 => {}
        (1, _) | (2, _) => return Err(BlockReadError::UnsupportedCodec),
        _ => return Err(BlockReadError::Corrupt),
    }
    let header =
        Header::decode_from(&mut Cursor::new(header_bytes)).map_err(|_| BlockReadError::Corrupt)?;
    let encoded_len = header.data_length as usize;
    let payload_at = reader.stream_position().map_err(BlockReadError::Io)?;
    let file_end = reader.seek(SeekFrom::End(0)).map_err(BlockReadError::Io)?;
    reader
        .seek(SeekFrom::Start(payload_at))
        .map_err(BlockReadError::Io)?;
    let end = extent_end.map_or(file_end, |e| e.min(file_end));
    if payload_at > end || encoded_len as u64 > end - payload_at {
        return Err(BlockReadError::Corrupt);
    }
    if encoded_len > limits.max_encoded {
        return Err(BlockReadError::EncodedLimit);
    }
    if header.compression == CompressionType::None && encoded_len > limits.max_decoded {
        return Err(BlockReadError::DecodedLimit);
    }
    if encoded_len > limits.max_allocation {
        return Err(BlockReadError::AllocationLimit);
    }
    charge(
        owner,
        operation,
        BudgetKind::AllocationBytes,
        encoded_len as u64,
    )?;
    charge(owner, operation, BudgetKind::InputBytes, encoded_len as u64)?;
    if header.compression == CompressionType::None {
        charge(
            owner,
            operation,
            BudgetKind::DecodeBytes,
            encoded_len as u64,
        )?;
    }
    let mut encoded = allocate_zeroed(encoded_len)?;
    reader
        .read_exact(&mut encoded)
        .map_err(BlockReadError::Io)?;
    if crate::segment::block::checksum::Checksum::from_bytes(&encoded) != header.checksum {
        return Err(BlockReadError::Corrupt);
    }
    let (decoded, mut charged) = match header.compression {
        CompressionType::None => (encoded, encoded_len),
        #[cfg(feature = "lz4")]
        CompressionType::Lz4 => {
            let len_bytes: [u8; 4] = encoded
                .get(..4)
                .ok_or(BlockReadError::Corrupt)?
                .try_into()
                .map_err(|_| BlockReadError::Corrupt)?;
            let decoded_len = u32::from_le_bytes(len_bytes) as usize;
            if decoded_len > limits.max_decoded {
                return Err(BlockReadError::DecodedLimit);
            }
            let charged = encoded_len
                .checked_add(decoded_len)
                .ok_or(BlockReadError::AllocationLimit)?;
            if charged > limits.max_allocation {
                return Err(BlockReadError::AllocationLimit);
            }
            charge(
                owner,
                operation,
                BudgetKind::AllocationBytes,
                decoded_len as u64,
            )?;
            charge(
                owner,
                operation,
                BudgetKind::DecodeBytes,
                decoded_len as u64,
            )?;
            let mut decoded = allocate_zeroed(decoded_len)?;
            let written = lz4_flex::block::decompress_into(&encoded[4..], &mut decoded)
                .map_err(|_| BlockReadError::Corrupt)?;
            if written != decoded_len {
                return Err(BlockReadError::Corrupt);
            }
            (decoded, charged)
        }
        #[cfg(feature = "miniz")]
        CompressionType::Miniz(_) => {
            use crate::bounded_inflate::{InflateError, inflate};
            let remaining_alloc = limits
                .max_allocation
                .checked_sub(encoded_len)
                .ok_or(BlockReadError::AllocationLimit)?;
            let (decoded, requested) = inflate(
                &encoded,
                limits.max_decoded,
                remaining_alloc,
                false,
                owner,
                operation,
            )
            .map_err(|error| match error {
                InflateError::Budget(e) => BlockReadError::Budget(e),
                InflateError::DecodedLimit => BlockReadError::DecodedLimit,
                InflateError::Corrupt => BlockReadError::Corrupt,
                InflateError::Allocation => BlockReadError::AllocationLimit,
            })?;
            let charged = encoded_len
                .checked_add(requested)
                .ok_or(BlockReadError::AllocationLimit)?;
            (decoded, charged)
        }
    };
    let count_bytes: [u8; 4] = decoded
        .get(..4)
        .ok_or(BlockReadError::Corrupt)?
        .try_into()
        .map_err(|_| BlockReadError::Corrupt)?;
    let count = u32::from_be_bytes(count_bytes);
    if count > limits.max_items {
        return Err(BlockReadError::ItemLimit);
    }
    charge(owner, operation, BudgetKind::VisitedItems, u64::from(count))?;
    let summary =
        validate_data_entries(&decoded, limits.max_key, limits.max_value, limits.max_items)
            .map_err(|error| match error {
                ValidationError::Corrupt => BlockReadError::Corrupt,
                ValidationError::KeyLimit => BlockReadError::KeyLimit,
                ValidationError::ValueLimit => BlockReadError::ValueLimit,
                ValidationError::ItemLimit => BlockReadError::ItemLimit,
            })?;
    let decoded_entries = summary
        .key_bytes
        .checked_add(summary.value_bytes)
        .ok_or(BlockReadError::AllocationLimit)?;
    charge(owner, operation, BudgetKind::DecodeBytes, decoded_entries)?;
    let item_capacity = (summary.items as usize)
        .checked_mul(std::mem::size_of::<InternalValue>())
        .ok_or(BlockReadError::AllocationLimit)?;
    // Each byteview copy may request an AtomicU64 header; charge both key and
    // value headers conservatively, even for inline values or tombstones.
    let headers = (summary.items as usize)
        .checked_mul(2 * std::mem::size_of::<std::sync::atomic::AtomicU64>())
        .ok_or(BlockReadError::AllocationLimit)?;
    // into_boxed_slice may allocate again if the vector has excess capacity.
    for amount in [
        item_capacity,
        item_capacity,
        headers,
        summary.key_bytes as usize,
        summary.value_bytes as usize,
    ] {
        charged = charged
            .checked_add(amount)
            .ok_or(BlockReadError::AllocationLimit)?;
    }
    if charged > limits.max_allocation {
        return Err(BlockReadError::AllocationLimit);
    }
    let entry_alloc = item_capacity
        .checked_mul(2)
        .and_then(|x| x.checked_add(headers))
        .and_then(|x| x.checked_add(summary.key_bytes as usize))
        .and_then(|x| x.checked_add(summary.value_bytes as usize))
        .ok_or(BlockReadError::AllocationLimit)?;
    charge(
        owner,
        operation,
        BudgetKind::AllocationBytes,
        entry_alloc as u64,
    )?;
    let mut items = Vec::new();
    items
        .try_reserve_exact(summary.items as usize)
        .map_err(|_| BlockReadError::AllocationLimit)?;
    let mut cursor = Cursor::new(decoded.as_slice());
    cursor.set_position(4);
    for _ in 0..summary.items {
        items.push(InternalValue::decode_from(&mut cursor).map_err(|_| BlockReadError::Corrupt)?);
    }
    Ok(ValueBlock {
        header,
        items: items.into_boxed_slice(),
    })
}
