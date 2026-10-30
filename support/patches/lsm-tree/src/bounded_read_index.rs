// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Read index blocks without trusting cached index metadata.

use crate::{
    bounded_budget::{Budget, BudgetError, BudgetKind},
    bounded_index::parse_index_entries,
    bounded_prepare::{PrepareError, RecoveryBudget, RecoveryResource},
    bounded_read::ReadError,
    coding::Decode,
    segment::{
        block::{checksum::Checksum, header::Header},
        block_index::block_handle::KeyedBlockHandle,
        meta::CompressionType,
    },
};
use std::io::{Cursor, Read, Seek, SeekFrom};

struct Adapter<'a> {
    call: &'a mut Budget,
    operation: &'a mut Budget,
}
impl RecoveryBudget for Adapter<'_> {
    fn charge(&mut self, resource: RecoveryResource, amount: u64) -> Result<(), PrepareError> {
        let kind = match resource {
            RecoveryResource::InputBytes => BudgetKind::InputBytes,
            RecoveryResource::AllocationBytes => BudgetKind::AllocationBytes,
            RecoveryResource::MetadataBytes => BudgetKind::MetadataBytes,
            RecoveryResource::VisitedItems => BudgetKind::VisitedItems,
            RecoveryResource::KeyBytes => {
                return Err(PrepareError::Unsupported("invalid cumulative key charge"));
            }
        };
        Budget::charge_both(self.call, self.operation, kind, amount).map_err(|error| {
            PrepareError::Limit {
                resource,
                limit: error.limit,
            }
        })
    }
    fn check_key_bytes(&mut self, amount: u64) -> Result<(), PrepareError> {
        let limit = self
            .call
            .limits()
            .max_key_bytes
            .min(self.operation.limits().max_key_bytes) as u64;
        if amount > limit {
            Err(PrepareError::Limit {
                resource: RecoveryResource::KeyBytes,
                limit,
            })
        } else {
            Ok(())
        }
    }
}
fn map_prepare(error: PrepareError) -> ReadError {
    match error {
        PrepareError::Limit {
            resource: RecoveryResource::KeyBytes,
            limit,
        } => ReadError::KeyLimit {
            limit: limit as usize,
        },
        PrepareError::Limit { resource, limit } => ReadError::Budget(BudgetError {
            kind: match resource {
                RecoveryResource::InputBytes => BudgetKind::InputBytes,
                RecoveryResource::AllocationBytes => BudgetKind::AllocationBytes,
                RecoveryResource::MetadataBytes => BudgetKind::MetadataBytes,
                RecoveryResource::VisitedItems => BudgetKind::VisitedItems,
                RecoveryResource::KeyBytes => unreachable!(),
            },
            limit,
        }),
        PrepareError::Corruption(_) => ReadError::Corruption,
        PrepareError::Unsupported(s) => ReadError::Unsupported(s),
        PrepareError::Io(e) => ReadError::Io(e),
    }
}
fn debit(
    call: &mut Budget,
    operation: &mut Budget,
    kind: BudgetKind,
    amount: usize,
) -> Result<(), ReadError> {
    Budget::charge_both(
        call,
        operation,
        kind,
        u64::try_from(amount).map_err(|_| ReadError::Corruption)?,
    )
    .map_err(ReadError::Budget)
}
fn allocation(call: &mut Budget, operation: &mut Budget, len: usize) -> Result<Vec<u8>, ReadError> {
    debit(call, operation, BudgetKind::MetadataBytes, len)?;
    debit(call, operation, BudgetKind::AllocationBytes, len)?;
    let mut result = Vec::new();
    result
        .try_reserve_exact(len)
        .map_err(|_| ReadError::Unsupported("index allocation failed"))?;
    result.resize(len, 0);
    Ok(result)
}
#[cfg(test)]
pub(crate) fn read_index<R: Read + Seek>(
    reader: &mut R,
    call: &mut Budget,
    operation: &mut Budget,
) -> Result<Vec<KeyedBlockHandle>, ReadError> {
    read_index_until(reader, call, operation, None)
}

pub(crate) fn read_index_until<R: Read + Seek>(
    reader: &mut R,
    call: &mut Budget,
    operation: &mut Budget,
    extent_end: Option<u64>,
) -> Result<Vec<KeyedBlockHandle>, ReadError> {
    let header_at = reader.stream_position().map_err(ReadError::Io)?;
    let file_end = reader.seek(SeekFrom::End(0)).map_err(ReadError::Io)?;
    reader
        .seek(SeekFrom::Start(header_at))
        .map_err(ReadError::Io)?;
    let end = extent_end.map_or(file_end, |extent| extent.min(file_end));
    if header_at
        .checked_add(Header::serialized_len() as u64)
        .is_none_or(|header_end| header_end > end)
    {
        return Err(ReadError::Corruption);
    }
    let mut raw = [0; Header::serialized_len()];
    debit(call, operation, BudgetKind::InputBytes, raw.len())?;
    reader.read_exact(&mut raw).map_err(ReadError::Io)?;
    if raw.get(..4) != Some(crate::file::MAGIC_BYTES.as_slice()) {
        return Err(ReadError::Corruption);
    }
    match (raw[4], raw[5]) {
        (0, 0) => {}
        #[cfg(feature = "lz4")]
        (1, 0) => {}
        #[cfg(feature = "miniz")]
        (2, level) if level <= 10 => {}
        (1, _) | (2, _) => return Err(ReadError::Unsupported("index compression")),
        _ => return Err(ReadError::Corruption),
    }
    let header = Header::decode_from(&mut Cursor::new(raw)).map_err(|_| ReadError::Corruption)?;
    let encoded_len = header.data_length as usize;
    let encoded_limit = call
        .limits()
        .max_block_encoded_bytes
        .min(operation.limits().max_block_encoded_bytes);
    let decoded_limit = call
        .limits()
        .max_block_decoded_bytes
        .min(operation.limits().max_block_decoded_bytes);
    if encoded_len > encoded_limit {
        return Err(ReadError::EncodedLimit {
            limit: encoded_limit,
        });
    }
    if header.compression == CompressionType::None && encoded_len > decoded_limit {
        return Err(ReadError::DecodedLimit {
            limit: decoded_limit,
        });
    }
    let start = reader.stream_position().map_err(ReadError::Io)?;
    let end = reader.seek(SeekFrom::End(0)).map_err(ReadError::Io)?;
    reader.seek(SeekFrom::Start(start)).map_err(ReadError::Io)?;
    let end = extent_end.map_or(end, |extent| extent.min(end));
    if start > end || encoded_len as u64 > end - start {
        return Err(ReadError::Corruption);
    }
    debit(call, operation, BudgetKind::InputBytes, encoded_len)?;
    let mut encoded = allocation(call, operation, encoded_len)?;
    reader.read_exact(&mut encoded).map_err(ReadError::Io)?;
    if Checksum::from_bytes(&encoded) != header.checksum {
        return Err(ReadError::Corruption);
    }
    let decoded = match header.compression {
        CompressionType::None => {
            debit(call, operation, BudgetKind::DecodeBytes, encoded_len)?;
            encoded
        }
        #[cfg(feature = "lz4")]
        CompressionType::Lz4 => {
            let len = u32::from_le_bytes(
                encoded
                    .get(..4)
                    .ok_or(ReadError::Corruption)?
                    .try_into()
                    .map_err(|_| ReadError::Corruption)?,
            ) as usize;
            if len > decoded_limit {
                return Err(ReadError::DecodedLimit {
                    limit: decoded_limit,
                });
            }
            debit(call, operation, BudgetKind::DecodeBytes, len)?;
            let mut decoded = allocation(call, operation, len)?;
            let written = lz4_flex::block::decompress_into(&encoded[4..], &mut decoded)
                .map_err(|_| ReadError::Corruption)?;
            if written != len {
                return Err(ReadError::Corruption);
            }
            decoded
        }
        #[cfg(feature = "miniz")]
        CompressionType::Miniz(_) => {
            use crate::bounded_inflate::{InflateError, inflate};
            let (decoded, _) = inflate(&encoded, decoded_limit, usize::MAX, true, call, operation)
                .map_err(|error| match error {
                    InflateError::Budget(e) => ReadError::Budget(e),
                    InflateError::DecodedLimit => ReadError::DecodedLimit {
                        limit: decoded_limit,
                    },
                    InflateError::Corrupt => ReadError::Corruption,
                    InflateError::Allocation => ReadError::Unsupported("index allocation failed"),
                })?;
            decoded
        }
    };
    let work = decoded.len().checked_mul(2).ok_or(ReadError::Corruption)?;
    debit(call, operation, BudgetKind::DecodeBytes, work)?;
    parse_index_entries(&decoded, &mut Adapter { call, operation }).map_err(map_prepare)
}
