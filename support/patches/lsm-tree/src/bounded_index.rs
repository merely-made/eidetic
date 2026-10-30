// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Bounded v2 index entry parsing for recovery and reads.

use crate::bounded_prepare::{PrepareError, RecoveryBudget, RecoveryResource};
use crate::segment::{block::offset::BlockOffset, block_index::block_handle::KeyedBlockHandle};
use byteorder::{BigEndian, ReadBytesExt};
use std::io::Cursor;
use varint_rs::VarintReader;

fn corrupt() -> PrepareError {
    PrepareError::Corruption("invalid index block".into())
}

fn field(
    cursor: &mut Cursor<&[u8]>,
    budget: &mut dyn RecoveryBudget,
) -> Result<(u64, usize, usize), PrepareError> {
    let offset = cursor.read_u64_varint().map_err(|_| corrupt())?;
    let len = usize::from(cursor.read_u16_varint().map_err(|_| corrupt())?);
    budget.check_key_bytes(len as u64)?;
    let start = usize::try_from(cursor.position()).map_err(|_| corrupt())?;
    let end = start.checked_add(len).ok_or_else(corrupt)?;
    if end > cursor.get_ref().len() {
        return Err(corrupt());
    }
    cursor.set_position(end as u64);
    Ok((offset, start, end))
}

pub(crate) fn parse_index_entries(
    bytes: &[u8],
    budget: &mut dyn RecoveryBudget,
) -> Result<Vec<KeyedBlockHandle>, PrepareError> {
    let mut cursor = Cursor::new(bytes);
    let count = cursor.read_u32::<BigEndian>().map_err(|_| corrupt())?;
    // Each entry needs at least two varints; validate the full encoding before allocating.
    if u64::from(count).saturating_mul(2) > bytes.len().saturating_sub(4) as u64 {
        return Err(corrupt());
    }
    budget.charge(RecoveryResource::VisitedItems, u64::from(count))?;
    let mut key_bytes = 0_u64;
    let mut previous: Option<(&[u8], u64)> = None;
    for _ in 0..count {
        let (offset, start, end) = field(&mut cursor, budget)?;
        let key = &bytes[start..end];
        if previous.is_some_and(|(prior, prior_offset)| prior > key || prior_offset >= offset) {
            return Err(corrupt());
        }
        previous = Some((key, offset));
        key_bytes = key_bytes
            .checked_add((end - start) as u64)
            .ok_or_else(corrupt)?;
    }
    if cursor.position() != bytes.len() as u64 {
        return Err(corrupt());
    }
    let vector_bytes = usize::try_from(count)
        .map_err(|_| corrupt())?
        .checked_mul(std::mem::size_of::<KeyedBlockHandle>())
        .ok_or_else(corrupt)? as u64;
    // byteview 0.6.1 copies long keys into a header plus payload allocation.
    // Charge a header for every key, including inline keys, conservatively.
    let headers = u64::from(count)
        .checked_mul(std::mem::size_of::<std::sync::atomic::AtomicU64>() as u64)
        .ok_or_else(corrupt)?;
    let allocation = vector_bytes
        .checked_add(key_bytes)
        .and_then(|n| n.checked_add(headers))
        .ok_or_else(corrupt)?;
    budget.charge(RecoveryResource::MetadataBytes, allocation)?;
    budget.charge(RecoveryResource::AllocationBytes, allocation)?;
    let mut entries = Vec::new();
    entries
        .try_reserve_exact(count as usize)
        .map_err(|_| PrepareError::Io(std::io::Error::other("index allocation failed")))?;
    cursor.set_position(4);
    for _ in 0..count {
        let (offset, start, end) = field(&mut cursor, budget)?;
        entries.push(KeyedBlockHandle::new(
            &bytes[start..end],
            BlockOffset(offset),
        ));
    }
    Ok(entries)
}
