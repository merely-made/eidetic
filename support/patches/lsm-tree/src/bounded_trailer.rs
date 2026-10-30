// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Bounded segment trailer and metadata admission.

use crate::{
    bounded_metadata::parse_metadata,
    bounded_prepare::{PrepareError, RecoveryBudget, RecoveryResource},
    coding::Decode,
    segment::{file_offsets::FileOffsets, meta::Metadata, trailer::TRAILER_SIZE},
};
use std::io::{Cursor, Read, Seek, SeekFrom};

fn corrupt() -> PrepareError {
    PrepareError::Corruption("invalid segment trailer or metadata extent".into())
}

fn read_key_len<R: Read + Seek>(
    reader: &mut R,
    position: u64,
    end: u64,
    budget: &mut dyn RecoveryBudget,
) -> Result<u64, PrepareError> {
    if position.checked_add(2).filter(|p| *p <= end).is_none() {
        return Err(corrupt());
    }
    budget.charge(RecoveryResource::InputBytes, 2)?;
    reader
        .seek(SeekFrom::Start(position))
        .map_err(PrepareError::Io)?;
    let mut bytes = [0; 2];
    reader.read_exact(&mut bytes).map_err(PrepareError::Io)?;
    let len = u16::from_be_bytes(bytes) as u64;
    budget.check_key_bytes(len)?;
    if position
        .checked_add(2)
        .and_then(|p| p.checked_add(len))
        .filter(|p| *p <= end)
        .is_none()
    {
        return Err(corrupt());
    }
    Ok(len)
}

pub(crate) fn read_segment_metadata<R: Read + Seek>(
    reader: &mut R,
    budget: &mut dyn RecoveryBudget,
) -> Result<(Metadata, FileOffsets), PrepareError> {
    let file_len = reader.seek(SeekFrom::End(0)).map_err(PrepareError::Io)?;
    let trailer_at = file_len
        .checked_sub(TRAILER_SIZE as u64)
        .ok_or_else(corrupt)?;
    budget.charge(RecoveryResource::InputBytes, TRAILER_SIZE as u64)?;
    reader
        .seek(SeekFrom::Start(trailer_at))
        .map_err(PrepareError::Io)?;
    let mut trailer = [0; TRAILER_SIZE];
    reader.read_exact(&mut trailer).map_err(PrepareError::Io)?;
    if trailer[TRAILER_SIZE - crate::file::MAGIC_BYTES.len()..] != crate::file::MAGIC_BYTES {
        return Err(corrupt());
    }
    let offsets = FileOffsets::decode_from(&mut Cursor::new(&trailer)).map_err(|_| corrupt())?;
    if *offsets.range_tombstones_ptr != 0 {
        return Err(PrepareError::Unsupported("range tombstones"));
    }
    let metadata_at = *offsets.metadata_ptr;
    const FIXED_METADATA_BYTES: u64 = 111;
    let min_at = metadata_at
        .checked_add(FIXED_METADATA_BYTES)
        .ok_or_else(corrupt)?;
    let min = read_key_len(reader, min_at, trailer_at, budget)?;
    let max_at = min_at
        .checked_add(2)
        .and_then(|p| p.checked_add(min))
        .ok_or_else(corrupt)?;
    let max = read_key_len(reader, max_at, trailer_at, budget)?;
    let metadata_end = max_at
        .checked_add(2)
        .and_then(|p| p.checked_add(max))
        .ok_or_else(corrupt)?;
    let metadata_len = metadata_end.checked_sub(metadata_at).ok_or_else(corrupt)?;
    budget.charge(RecoveryResource::InputBytes, metadata_len)?;
    budget.charge(RecoveryResource::MetadataBytes, metadata_len)?;
    budget.charge(RecoveryResource::AllocationBytes, metadata_len)?;
    let len = usize::try_from(metadata_len).map_err(|_| corrupt())?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(len)
        .map_err(|_| PrepareError::Unsupported("metadata allocation failed"))?;
    bytes.resize(len, 0);
    reader
        .seek(SeekFrom::Start(metadata_at))
        .map_err(PrepareError::Io)?;
    reader.read_exact(&mut bytes).map_err(PrepareError::Io)?;
    let metadata = parse_metadata(&bytes, budget)?;
    Ok((metadata, offsets))
}
