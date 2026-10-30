// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Allocation-free preflight of the existing v2 data-block entry encoding.

use byteorder::{BigEndian, ReadBytesExt};
use std::io::Cursor;
use varint_rs::VarintReader;

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ValidationError {
    Corrupt,
    KeyLimit,
    ValueLimit,
    ItemLimit,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ValidationSummary {
    pub(crate) items: u32,
    pub(crate) key_bytes: u64,
    pub(crate) value_bytes: u64,
}

fn skip(cursor: &mut Cursor<&[u8]>, len: usize) -> Result<(), ValidationError> {
    let pos = usize::try_from(cursor.position()).map_err(|_| ValidationError::Corrupt)?;
    let end = pos.checked_add(len).ok_or(ValidationError::Corrupt)?;
    if end > cursor.get_ref().len() {
        return Err(ValidationError::Corrupt);
    }
    cursor.set_position(end as u64);
    Ok(())
}

pub(crate) fn validate_data_entries(
    bytes: &[u8],
    max_key: usize,
    max_value: usize,
    max_items: u32,
) -> Result<ValidationSummary, ValidationError> {
    let mut cursor = Cursor::new(bytes);
    let count = cursor.read_u32::<BigEndian>().map_err(|_| ValidationError::Corrupt)?;
    if count > max_items {
        return Err(ValidationError::ItemLimit);
    }
    let mut key_bytes = 0_u64;
    let mut value_bytes = 0_u64;
    let mut previous: Option<(&[u8], u64)> = None;
    for _ in 0..count {
        let seqno = cursor.read_u64_varint().map_err(|_| ValidationError::Corrupt)?;
        let tag = cursor.read_u8().map_err(|_| ValidationError::Corrupt)?;
        if tag > 2 {
            return Err(ValidationError::Corrupt);
        }
        let key_len = usize::from(cursor.read_u16_varint().map_err(|_| ValidationError::Corrupt)?);
        if key_len == 0 {
            return Err(ValidationError::Corrupt);
        }
        if key_len > max_key {
            return Err(ValidationError::KeyLimit);
        }
        let start = usize::try_from(cursor.position()).map_err(|_| ValidationError::Corrupt)?;
        skip(&mut cursor, key_len)?;
        let end = usize::try_from(cursor.position()).map_err(|_| ValidationError::Corrupt)?;
        let key = &bytes[start..end];
        if previous.is_some_and(|(prior, prior_seqno)| prior > key || (prior == key && prior_seqno < seqno)) {
            return Err(ValidationError::Corrupt);
        }
        previous = Some((key, seqno));
        key_bytes = key_bytes.checked_add(key_len as u64).ok_or(ValidationError::Corrupt)?;
        if tag == 0 {
            let value_len = cursor.read_u32_varint().map_err(|_| ValidationError::Corrupt)? as usize;
            if value_len > max_value {
                return Err(ValidationError::ValueLimit);
            }
            skip(&mut cursor, value_len)?;
            value_bytes = value_bytes.checked_add(value_len as u64).ok_or(ValidationError::Corrupt)?;
        }
    }
    if cursor.position() != bytes.len() as u64 {
        return Err(ValidationError::Corrupt);
    }
    Ok(ValidationSummary { items: count, key_bytes, value_bytes })
}
