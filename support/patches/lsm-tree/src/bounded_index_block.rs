// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Read an index block under recovery metadata and allocation budgets.
//! Used while preparing a segment for bounded recovery.

use crate::{
    bounded_index::parse_index_entries,
    bounded_prepare::{PrepareError, RecoveryBudget, RecoveryResource},
    coding::Decode,
    segment::{
        block::header::Header, block_index::block_handle::KeyedBlockHandle, meta::CompressionType,
    },
};
use std::io::{Cursor, Read, Seek, SeekFrom};

fn corrupt() -> PrepareError {
    PrepareError::Corruption("invalid index block".into())
}
fn allocation(budget: &mut dyn RecoveryBudget, len: usize) -> Result<(), PrepareError> {
    budget.charge(RecoveryResource::MetadataBytes, len as u64)?;
    budget.charge(RecoveryResource::AllocationBytes, len as u64)
}
fn buffer(len: usize) -> Result<Vec<u8>, PrepareError> {
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(len)
        .map_err(|_| PrepareError::Io(std::io::Error::other("index allocation failed")))?;
    bytes.resize(len, 0);
    Ok(bytes)
}

#[cfg(test)]
pub(crate) fn read_index_block<R: Read + Seek>(
    reader: &mut R,
    budget: &mut dyn RecoveryBudget,
) -> Result<Vec<KeyedBlockHandle>, PrepareError> {
    read_index_block_until(reader, budget, None)
}
pub(crate) fn read_index_block_until<R: Read + Seek>(
    reader: &mut R,
    budget: &mut dyn RecoveryBudget,
    extent_end: Option<u64>,
) -> Result<Vec<KeyedBlockHandle>, PrepareError> {
    let header_at = reader.stream_position().map_err(PrepareError::Io)?;
    let file_end = reader.seek(SeekFrom::End(0)).map_err(PrepareError::Io)?;
    reader
        .seek(SeekFrom::Start(header_at))
        .map_err(PrepareError::Io)?;
    let end = extent_end.map_or(file_end, |extent| extent.min(file_end));
    if header_at
        .checked_add(Header::serialized_len() as u64)
        .is_none_or(|header_end| header_end > end)
    {
        return Err(corrupt());
    }
    let mut header_bytes = [0_u8; Header::serialized_len()];
    budget.charge(RecoveryResource::InputBytes, header_bytes.len() as u64)?;
    reader
        .read_exact(&mut header_bytes)
        .map_err(PrepareError::Io)?;
    if header_bytes.get(..4) != Some(crate::file::MAGIC_BYTES.as_slice()) {
        return Err(corrupt());
    }
    match (header_bytes[4], header_bytes[5]) {
        (0, 0) => {}
        #[cfg(feature = "lz4")]
        (1, 0) => {}
        #[cfg(feature = "miniz")]
        (2, level) if level <= 10 => {}
        (1, _) | (2, _) => return Err(PrepareError::Unsupported("index compression")),
        _ => return Err(corrupt()),
    }
    let header = Header::decode_from(&mut Cursor::new(header_bytes)).map_err(|_| corrupt())?;
    let encoded_len = header.data_length as usize;
    let start = reader.stream_position().map_err(PrepareError::Io)?;
    let end = reader.seek(SeekFrom::End(0)).map_err(PrepareError::Io)?;
    reader
        .seek(SeekFrom::Start(start))
        .map_err(PrepareError::Io)?;
    let end = extent_end.map_or(end, |extent| extent.min(end));
    if start > end || (encoded_len as u64) > end - start {
        return Err(corrupt());
    }
    budget.charge(RecoveryResource::InputBytes, encoded_len as u64)?;
    allocation(budget, encoded_len)?;
    let mut encoded = buffer(encoded_len)?;
    reader.read_exact(&mut encoded).map_err(PrepareError::Io)?;
    if crate::segment::block::checksum::Checksum::from_bytes(&encoded) != header.checksum {
        return Err(corrupt());
    }
    let decoded = match header.compression {
        CompressionType::None => encoded,
        #[cfg(feature = "lz4")]
        CompressionType::Lz4 => {
            let len: [u8; 4] = encoded
                .get(..4)
                .ok_or_else(corrupt)?
                .try_into()
                .map_err(|_| corrupt())?;
            let len = u32::from_le_bytes(len) as usize;
            allocation(budget, len)?;
            let mut output = buffer(len)?;
            let written = lz4_flex::block::decompress_into(&encoded[4..], &mut output)
                .map_err(|_| corrupt())?;
            if written != len {
                return Err(corrupt());
            }
            output
        }
        #[cfg(feature = "miniz")]
        CompressionType::Miniz(_) => decompress_miniz(&encoded, budget)?,
    };
    parse_index_entries(&decoded, budget)
}

#[cfg(feature = "miniz")]
fn decompress_miniz(
    encoded: &[u8],
    budget: &mut dyn RecoveryBudget,
) -> Result<Vec<u8>, PrepareError> {
    use miniz_oxide::inflate::{
        TINFLStatus,
        core::{
            DecompressorOxide, decompress, inflate_flags::TINFL_FLAG_USING_NON_WRAPPING_OUTPUT_BUF,
        },
    };
    let mut output = Vec::new();
    let mut state = DecompressorOxide::new();
    let mut consumed = 0_usize;
    let mut written = 0_usize;
    let mut target = encoded.len().saturating_mul(2).clamp(1, 4096);
    loop {
        allocation(budget, target)?;
        output
            .try_reserve_exact(target.saturating_sub(output.len()))
            .map_err(|_| PrepareError::Io(std::io::Error::other("index allocation failed")))?;
        output.resize(target, 0);
        let (status, input, output_bytes) = decompress(
            &mut state,
            &encoded[consumed..],
            &mut output,
            written,
            TINFL_FLAG_USING_NON_WRAPPING_OUTPUT_BUF,
        );
        consumed = consumed.checked_add(input).ok_or_else(corrupt)?;
        written = written.checked_add(output_bytes).ok_or_else(corrupt)?;
        match status {
            TINFLStatus::Done => {
                output.truncate(written);
                return Ok(output);
            }
            TINFLStatus::HasMoreOutput => target = target.checked_mul(2).ok_or_else(corrupt)?,
            _ => return Err(corrupt()),
        }
    }
}
