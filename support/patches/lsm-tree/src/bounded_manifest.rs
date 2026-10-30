// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Bounded decoding of the v2 level manifest.

use crate::bounded_prepare::{PrepareError, RecoveryBudget, RecoveryResource};
use byteorder::{BigEndian, ReadBytesExt};
use std::io::Cursor;

fn corrupt() -> PrepareError {
    PrepareError::Corruption("invalid level manifest".into())
}

fn reserve<T>(vec: &mut Vec<T>, count: usize, budget: &mut dyn RecoveryBudget) -> Result<(), PrepareError> {
    let bytes = count.checked_mul(std::mem::size_of::<T>()).ok_or_else(corrupt)?;
    budget.charge(RecoveryResource::MetadataBytes, bytes as u64)?;
    budget.charge(RecoveryResource::AllocationBytes, bytes as u64)?;
    vec.try_reserve_exact(count).map_err(|_| PrepareError::Io(std::io::Error::other("level manifest allocation failed")))
}

pub(crate) fn parse_levels(bytes: &[u8], budget: &mut dyn RecoveryBudget) -> Result<Vec<Vec<u64>>, PrepareError> {
    if bytes.get(..4) != Some(crate::file::MAGIC_BYTES.as_slice()) {
        return Err(corrupt());
    }
    let mut cursor = Cursor::new(bytes);
    cursor.set_position(4);
    let level_count = cursor.read_u8().map_err(|_| corrupt())? as usize;
    if level_count == 0 {
        return Err(corrupt());
    }

    // First validate all declared lengths before reserving any vectors.
    for _ in 0..level_count {
        let count = cursor.read_u32::<BigEndian>().map_err(|_| corrupt())?;
        let segment_bytes = usize::try_from(count).map_err(|_| corrupt())?
            .checked_mul(std::mem::size_of::<u64>()).ok_or_else(corrupt)?;
        let position = usize::try_from(cursor.position()).map_err(|_| corrupt())?;
        let end = position.checked_add(segment_bytes).ok_or_else(corrupt)?;
        if end > bytes.len() {
            return Err(corrupt());
        }
        cursor.set_position(end as u64);
    }

    let mut levels = Vec::new();
    reserve(&mut levels, level_count, budget)?;
    cursor.set_position(5);
    for _ in 0..level_count {
        let count = cursor.read_u32::<BigEndian>().map_err(|_| corrupt())? as usize;
        budget.charge(RecoveryResource::VisitedItems, count as u64)?;
        let mut level = Vec::new();
        reserve(&mut level, count, budget)?;
        for _ in 0..count {
            level.push(cursor.read_u64::<BigEndian>().map_err(|_| corrupt())?);
        }
        levels.push(level);
    }
    Ok(levels)
}
