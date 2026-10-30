// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Bounded v2 segment metadata decoding.

use crate::{bounded_prepare::{PrepareError, RecoveryBudget, RecoveryResource}, coding::Decode, segment::meta::Metadata};
use byteorder::{BigEndian, ReadBytesExt};
use std::io::Cursor;

fn corrupt() -> PrepareError { PrepareError::Corruption("invalid segment metadata".into()) }

fn key(cursor: &mut Cursor<&[u8]>, budget: &mut dyn RecoveryBudget) -> Result<u64, PrepareError> {
    let len = cursor.read_u16::<BigEndian>().map_err(|_| corrupt())? as usize;
    budget.check_key_bytes(len as u64)?;
    let start = usize::try_from(cursor.position()).map_err(|_| corrupt())?;
    let end = start.checked_add(len).ok_or_else(corrupt)?;
    if end > cursor.get_ref().len() { return Err(corrupt()); }
    cursor.set_position(end as u64);
    Ok(len as u64)
}

pub(crate) fn parse_metadata(bytes: &[u8], budget: &mut dyn RecoveryBudget) -> Result<Metadata, PrepareError> {
    if bytes.get(..4) != Some(crate::file::MAGIC_BYTES.as_slice()) { return Err(corrupt()); }
    let mut cursor = Cursor::new(bytes);
    cursor.set_position(4);
    cursor.read_u64::<BigEndian>().map_err(|_| corrupt())?;
    cursor.read_u128::<BigEndian>().map_err(|_| corrupt())?;
    for _ in 0..6 { cursor.read_u64::<BigEndian>().map_err(|_| corrupt())?; }
    for _ in 0..4 { cursor.read_u32::<BigEndian>().map_err(|_| corrupt())?; }
    let tag = cursor.read_u8().map_err(|_| corrupt())?;
    let parameter = cursor.read_u8().map_err(|_| corrupt())?;
    match (tag, parameter) {
        (0, 0) => {},
        #[cfg(feature = "lz4")]
        (1, 0) => {},
        #[cfg(feature = "miniz")]
        (2, level) if level <= 10 => {},
        (1, _) | (2, _) => return Err(PrepareError::Unsupported("segment compression")),
        _ => return Err(corrupt()),
    }
    if cursor.read_u8().map_err(|_| corrupt())? != 0 { return Err(corrupt()); }
    cursor.read_u64::<BigEndian>().map_err(|_| corrupt())?;
    cursor.read_u64::<BigEndian>().map_err(|_| corrupt())?;
    let min = key(&mut cursor, budget)?;
    let max = key(&mut cursor, budget)?;
    // byteview 0.6.1 may allocate an AtomicU64 header for each copied key.
    // Conservatively charge the header even for inline keys.
    let total = min.checked_add(max).and_then(|n| n.checked_add(2 * std::mem::size_of::<std::sync::atomic::AtomicU64>() as u64)).ok_or_else(corrupt)?;
    budget.charge(RecoveryResource::MetadataBytes, total)?;
    budget.charge(RecoveryResource::AllocationBytes, total)?;
    Metadata::decode_from(&mut Cursor::new(bytes)).map_err(|_| corrupt())
}
