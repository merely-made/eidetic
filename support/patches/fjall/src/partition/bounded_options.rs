// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Read v2 partition configuration without mutation or length-driven allocation.

use crate::{
    bounded_recovery::{
        PartitionOptionsPlan, RecoveryBudget, RecoveryError, RecoveryLimitKind as Kind,
    },
    compaction::Strategy,
    partition::options::CreateOptions,
};
use byteorder::{BigEndian, ReadBytesExt};
use lsm_tree::{CompressionType, TreeType};
use std::{
    fs::File,
    io::{Cursor, Read},
    path::Path,
};

// The complete v2 standard config has fixed fields; 64 exceeds its largest
// valid encoding, including FIFO TTL. Longer files are not admitted here.
const MAX_CONFIG_BYTES: usize = 64;

fn corrupt() -> RecoveryError {
    RecoveryError::Corruption("invalid partition configuration".into())
}

fn compression(input: &mut Cursor<&[u8]>) -> Result<CompressionType, RecoveryError> {
    let tag = input.read_u8().map_err(|_| corrupt())?;
    let parameter = input.read_u8().map_err(|_| corrupt())?;
    match (tag, parameter) {
        (0, 0) => Ok(CompressionType::None),
        #[cfg(feature = "lz4")]
        (1, 0) => Ok(CompressionType::Lz4),
        #[cfg(not(feature = "lz4"))]
        (1, 0) => Err(RecoveryError::Unsupported("LZ4 partition configuration")),
        #[cfg(feature = "miniz")]
        (2, level @ 0..=10) => Ok(CompressionType::Miniz(level)),
        #[cfg(not(feature = "miniz"))]
        (2, 0..=10) => Err(RecoveryError::Unsupported("Miniz partition configuration")),
        _ => Err(corrupt()),
    }
}

fn decode(bytes: &[u8]) -> Result<CreateOptions, RecoveryError> {
    let mut input = Cursor::new(bytes);
    let mut header = [0_u8; 4];
    input.read_exact(&mut header).map_err(|_| corrupt())?;
    if header != *crate::file::MAGIC_BYTES {
        return Err(corrupt());
    }
    let level_count = input.read_u8().map_err(|_| corrupt())?;
    let tree = input.read_u8().map_err(|_| corrupt())?;
    match tree {
        0 => {}
        1 => return Err(RecoveryError::Unsupported("blob-separated partition")),
        _ => return Err(corrupt()),
    }
    let max_memtable_size = input.read_u32::<BigEndian>().map_err(|_| corrupt())?;
    let data_block_size = input.read_u32::<BigEndian>().map_err(|_| corrupt())?;
    let index_block_size = input.read_u32::<BigEndian>().map_err(|_| corrupt())?;
    let compression = compression(&mut input)?;
    let manual_journal_persist = input.read_u8().map_err(|_| corrupt())? != 0;
    let bloom_bits_per_key = input.read_i8().map_err(|_| corrupt())?;
    let compaction_strategy = match input.read_u8().map_err(|_| corrupt())? {
        0 => Strategy::Leveled(crate::compaction::Leveled {
            l0_threshold: input.read_u8().map_err(|_| corrupt())?,
            level_ratio: input.read_u8().map_err(|_| corrupt())?,
            target_size: input.read_u32::<BigEndian>().map_err(|_| corrupt())?,
        }),
        1 => Strategy::SizeTiered(crate::compaction::SizeTiered {
            level_ratio: input.read_u8().map_err(|_| corrupt())?,
            base_size: input.read_u32::<BigEndian>().map_err(|_| corrupt())?,
        }),
        2 => {
            let limit = input.read_u64::<BigEndian>().map_err(|_| corrupt())?;
            let ttl = match input.read_u8().map_err(|_| corrupt())? {
                0 => None,
                1 => Some(input.read_u64::<BigEndian>().map_err(|_| corrupt())?),
                _ => return Err(corrupt()),
            };
            Strategy::Fifo(crate::compaction::Fifo::new(limit, ttl))
        }
        _ => return Err(corrupt()),
    };
    match input.read_u8().map_err(|_| corrupt())? {
        0 => {}
        1 => return Err(RecoveryError::Unsupported("blob-separated partition")),
        _ => return Err(corrupt()),
    }
    Ok(CreateOptions {
        max_memtable_size,
        data_block_size,
        index_block_size,
        level_count,
        bloom_bits_per_key,
        tree_type: TreeType::Standard,
        compression,
        manual_journal_persist,
        compaction_strategy,
        kv_separation: None,
    })
}

pub(crate) fn prepare(
    path: &Path,
    budget: &mut RecoveryBudget,
) -> Result<PartitionOptionsPlan, RecoveryError> {
    let path_len = path.as_os_str().as_encoded_bytes().len() as u64;
    // Two fixed-name joins can each clone and grow the input path. Reserve
    // their conservative cumulative request plus the fixed scratch and options.
    let metadata = path_len
        .checked_mul(6)
        .and_then(|n| {
            n.checked_add(
                128 + MAX_CONFIG_BYTES as u64 + std::mem::size_of::<CreateOptions>() as u64,
            )
        })
        .ok_or(RecoveryError::Limit {
            kind: Kind::MetadataBytes,
            limit: budget.limits.max_metadata_bytes,
        })?;
    budget.metadata(metadata)?;
    let blob_path = path.join(lsm_tree::file::BLOBS_FOLDER);
    crate::journal::bounded_set::charge_path_io(&blob_path, budget)?;
    match std::fs::symlink_metadata(blob_path) {
        Ok(_) => return Err(RecoveryError::Unsupported("blob-separated partition")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(RecoveryError::Io(error)),
    }
    let config_path = path.join(crate::file::PARTITION_CONFIG_FILE);
    crate::journal::bounded_set::charge_path_io(&config_path, budget)?;
    let mut file = File::open(config_path).map_err(RecoveryError::Io)?;
    let len = file.metadata().map_err(RecoveryError::Io)?.len();
    if len > MAX_CONFIG_BYTES as u64 {
        return Err(RecoveryError::Unsupported(
            "oversized partition configuration",
        ));
    }
    budget.charge(Kind::InputBytes, len)?;
    let mut bytes = [0_u8; MAX_CONFIG_BYTES];
    let slice = bytes.get_mut(..len as usize).ok_or_else(corrupt)?;
    file.read_exact(slice).map_err(RecoveryError::Io)?;
    let options = decode(slice)?;
    Ok(PartitionOptionsPlan { options })
}
