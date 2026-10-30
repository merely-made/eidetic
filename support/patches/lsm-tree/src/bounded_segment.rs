// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Build an admitted standard segment without changing its file or descriptor table.

use crate::{
    bounded_index_block::read_index_block_until,
    bounded_prepare::{PrepareError, RecoveryBudget, RecoveryResource},
    bounded_trailer::read_segment_metadata,
    cache::Cache,
    descriptor_table::FileDescriptorTable,
    segment::{
        Segment,
        block_index::{BlockIndexImpl, block_handle::KeyedBlockHandle, full_index::FullBlockIndex},
        inner::Inner,
    },
    tree::inner::TreeId,
};
use std::{
    fs::File,
    io::{Seek, SeekFrom},
    path::Path,
    sync::{Arc, atomic::AtomicBool},
};

fn corrupt() -> PrepareError {
    PrepareError::Corruption("invalid segment index".into())
}
fn charge_capacity(budget: &mut dyn RecoveryBudget, bytes: usize) -> Result<(), PrepareError> {
    let bytes = u64::try_from(bytes).map_err(|_| corrupt())?;
    budget.charge(RecoveryResource::MetadataBytes, bytes)?;
    budget.charge(RecoveryResource::AllocationBytes, bytes)
}

pub(crate) fn prepare_segment(
    path: &Path,
    tree_id: TreeId,
    cache: Arc<Cache>,
    descriptor_table: Arc<FileDescriptorTable>,
    budget: &mut dyn RecoveryBudget,
) -> Result<Segment, PrepareError> {
    #[cfg(feature = "bytes")]
    return Err(PrepareError::Unsupported("bytes-backed bounded recovery"));
    #[cfg(not(feature = "bytes"))]
    {
        charge_capacity(budget, crate::bounded_tree::os_path_capacity(path)?)?;
        let mut file = File::open(path).map_err(PrepareError::Io)?;
        let (metadata, offsets) = read_segment_metadata(&mut file, budget)?;
        if metadata.index_block_count == 0 || *offsets.index_block_ptr >= *offsets.metadata_ptr {
            return Err(corrupt());
        }
        budget.charge(
            RecoveryResource::VisitedItems,
            u64::from(metadata.index_block_count),
        )?;
        file.seek(SeekFrom::Start(*offsets.index_block_ptr))
            .map_err(PrepareError::Io)?;
        let mut entries = Vec::<KeyedBlockHandle>::new();
        for _ in 0..metadata.index_block_count {
            let block = read_index_block_until(&mut file, budget, Some(*offsets.metadata_ptr))?;
            for handle in &block {
                if handle
                    .offset
                    .checked_add(crate::segment::block::header::Header::serialized_len() as u64)
                    .is_none_or(|end| end > *offsets.index_block_ptr)
                {
                    return Err(corrupt());
                }
            }
            let new_len = entries.len().checked_add(block.len()).ok_or_else(corrupt)?;
            if new_len > entries.capacity() {
                let capacity = entries
                    .capacity()
                    .max(2)
                    .checked_mul(2)
                    .ok_or_else(corrupt)?
                    .max(new_len);
                charge_capacity(
                    budget,
                    capacity
                        .checked_mul(std::mem::size_of::<KeyedBlockHandle>())
                        .ok_or_else(corrupt)?,
                )?;
                entries
                    .try_reserve_exact(capacity - entries.len())
                    .map_err(|_| {
                        PrepareError::Io(std::io::Error::other("segment index allocation failed"))
                    })?;
            }
            entries.extend(block);
        }
        if entries.is_empty()
            || entries
                .windows(2)
                .any(|pair| pair[0].end_key > pair[1].end_key || *pair[0].offset >= *pair[1].offset)
        {
            return Err(corrupt());
        }
        // Conversion to a boxed slice can request a new allocation when spare capacity exists.
        charge_capacity(
            budget,
            entries
                .len()
                .checked_mul(std::mem::size_of::<KeyedBlockHandle>())
                .ok_or_else(corrupt)?,
        )?;
        let index = FullBlockIndex::from_items(entries);
        let arcs = std::mem::size_of::<Inner>()
            .checked_add(std::mem::size_of::<BlockIndexImpl>())
            .and_then(|n| n.checked_add(4 * std::mem::size_of::<usize>()))
            .and_then(|n| n.checked_add(path.as_os_str().as_encoded_bytes().len()))
            .ok_or_else(corrupt)?;
        charge_capacity(budget, arcs)?;
        Ok(Segment::from(Inner {
            path: path.into(),
            tree_id,
            descriptor_table,
            metadata,
            offsets,
            block_index: Arc::new(BlockIndexImpl::Full(index)),
            cache,
            // Bounded recovery can omit the optional filter without changing lookup results.
            bloom_filter: None,
            is_deleted: AtomicBool::default(),
        }))
    }
}
