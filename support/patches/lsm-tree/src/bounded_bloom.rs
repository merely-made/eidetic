// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Bounded decoding of the v2 Bloom filter encoding. Not yet used by recovery.

use crate::{bloom::BloomFilter, bounded_prepare::{PrepareError, RecoveryBudget, RecoveryResource}, coding::Decode};
use byteorder::{BigEndian, ReadBytesExt};
use std::io::Cursor;

fn corrupt() -> PrepareError {
    PrepareError::Corruption("invalid bloom filter".into())
}

pub(crate) fn parse_bloom(bytes: &[u8], budget: &mut dyn RecoveryBudget) -> Result<BloomFilter, PrepareError> {
    if bytes.get(..4) != Some(crate::file::MAGIC_BYTES.as_slice()) || bytes.get(4..6) != Some([0, 0].as_slice()) {
        return Err(corrupt());
    }
    let mut cursor = Cursor::new(bytes);
    cursor.set_position(6);
    let bits = cursor.read_u64::<BigEndian>().map_err(|_| corrupt())?;
    let hashes = cursor.read_u64::<BigEndian>().map_err(|_| corrupt())?;
    let byte_len = usize::try_from(bits / 8).map_err(|_| corrupt())?;
    usize::try_from(hashes).map_err(|_| corrupt())?;
    let end = usize::try_from(cursor.position()).map_err(|_| corrupt())?.checked_add(byte_len).ok_or_else(corrupt)?;
    if end > bytes.len() {
        return Err(corrupt());
    }
    budget.charge(RecoveryResource::MetadataBytes, byte_len as u64)?;
    budget.charge(RecoveryResource::AllocationBytes, byte_len as u64)?;
    BloomFilter::decode_from(&mut Cursor::new(bytes)).map_err(|_| corrupt())
}
