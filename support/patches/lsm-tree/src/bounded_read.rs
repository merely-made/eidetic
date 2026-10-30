// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Synchronous bounded reads. Do not expose lazy iterators across a budget scope.

use crate::{
    InternalValue, SeqNo, Slice,
    bounded_block::{BlockLimits, BlockReadError, read_data_block_budgeted_until},
    bounded_budget::{Budget, BudgetError, BudgetKind},
    tree::Tree,
};
use std::{
    cmp::Ordering,
    fs::File,
    io::{Seek, SeekFrom},
};

/// A bounded read refusal or underlying storage error.
#[derive(Debug)]
pub enum ReadError {
    /// One cumulative budget was exhausted.
    Budget(BudgetError),
    /// A stored value exceeded the per-value ceiling.
    ValueLimit {
        /// Configured per-item ceiling.
        limit: usize,
    },
    /// A stored key exceeded the per-key ceiling.
    KeyLimit {
        /// Configured per-item ceiling.
        limit: usize,
    },
    /// A compressed block exceeded its ceiling.
    EncodedLimit {
        /// Configured per-item ceiling.
        limit: usize,
    },
    /// A decoded block exceeded its ceiling.
    DecodedLimit {
        /// Configured per-item ceiling.
        limit: usize,
    },
    /// The stored data was invalid.
    Corruption,
    /// This tree uses an unsupported representation.
    Unsupported(&'static str),
    /// Underlying I/O failed.
    Io(std::io::Error),
}
impl From<BudgetError> for ReadError {
    fn from(e: BudgetError) -> Self {
        Self::Budget(e)
    }
}

fn charge(
    call: &mut Budget,
    operation: &mut Budget,
    kind: BudgetKind,
    amount: u64,
) -> Result<(), ReadError> {
    Budget::charge_both(call, operation, kind, amount).map_err(Into::into)
}
fn append(
    items: &mut Vec<InternalValue>,
    item: InternalValue,
    call: &mut Budget,
    operation: &mut Budget,
) -> Result<(), ReadError> {
    if items.len() == items.capacity() {
        let next = items
            .capacity()
            .max(2)
            .checked_mul(2)
            .ok_or(ReadError::Corruption)?;
        let bytes = next
            .checked_mul(std::mem::size_of::<InternalValue>())
            .ok_or(ReadError::Corruption)? as u64;
        charge(call, operation, BudgetKind::MetadataBytes, bytes)?;
        charge(call, operation, BudgetKind::AllocationBytes, bytes)?;
        items
            .try_reserve_exact(next - items.len())
            .map_err(|_| ReadError::Unsupported("read allocation failed"))?;
    }
    items.push(item);
    Ok(())
}
fn block_error(error: BlockReadError, call: &Budget, operation: &Budget) -> ReadError {
    match error {
        BlockReadError::Io(e) => ReadError::Io(e),
        BlockReadError::Corrupt => ReadError::Corruption,
        BlockReadError::UnsupportedCodec => ReadError::Unsupported("block compression"),
        BlockReadError::EncodedLimit => ReadError::EncodedLimit {
            limit: call
                .limits
                .max_block_encoded_bytes
                .min(operation.limits.max_block_encoded_bytes),
        },
        BlockReadError::DecodedLimit => ReadError::DecodedLimit {
            limit: call
                .limits
                .max_block_decoded_bytes
                .min(operation.limits.max_block_decoded_bytes),
        },
        BlockReadError::ValueLimit => ReadError::ValueLimit {
            limit: call
                .limits
                .max_value_bytes
                .min(operation.limits.max_value_bytes),
        },
        BlockReadError::KeyLimit => ReadError::KeyLimit {
            limit: call
                .limits
                .max_key_bytes
                .min(operation.limits.max_key_bytes),
        },
        BlockReadError::ItemLimit | BlockReadError::AllocationLimit => {
            ReadError::Unsupported("block allocation or item limit")
        }
        BlockReadError::Budget(e) => ReadError::Budget(e),
    }
}
fn collect(
    tree: &Tree,
    lower: &[u8],
    upper: Option<&[u8]>,
    point: bool,
    seqno: Option<SeqNo>,
    call: &mut Budget,
    operation: &mut Budget,
) -> Result<Vec<InternalValue>, ReadError> {
    #[cfg(feature = "bytes")]
    return Err(ReadError::Unsupported("bytes-backed bounded reads"));
    let key_limit = call
        .limits
        .max_key_bytes
        .min(operation.limits.max_key_bytes);
    let value_limit = call
        .limits
        .max_value_bytes
        .min(operation.limits.max_value_bytes);
    if lower.len() > key_limit {
        return Err(ReadError::KeyLimit { limit: key_limit });
    }
    let in_range = |key: &[u8]| {
        if point {
            key == lower
        } else {
            key >= lower && upper.is_none_or(|end| key < end)
        }
    };
    let visible = |item: &InternalValue| seqno.is_none_or(|s| item.key.seqno < s);
    let mut items = Vec::new();
    // Upstream's lock order is levels, active memtable, sealed memtables.
    let levels = tree.levels.read().expect("lock is poisoned");
    let active = tree.active_memtable.read().expect("lock is poisoned");
    let sealed = tree.sealed_memtables.read().expect("lock is poisoned");
    for table in std::iter::once(&**active).chain(sealed.iter().rev().map(|(_, table)| &**table)) {
        for entry in table.items.iter() {
            charge(call, operation, BudgetKind::VisitedItems, 1)?;
            let key = entry.key();
            let value = entry.value();
            let work = key
                .user_key
                .len()
                .checked_add(value.len())
                .ok_or(ReadError::Corruption)?;
            charge(call, operation, BudgetKind::DecodeBytes, work as u64)?;
            if !in_range(&key.user_key) || seqno.is_some_and(|s| key.seqno >= s) {
                continue;
            }
            if key.user_key.len() > key_limit {
                return Err(ReadError::KeyLimit { limit: key_limit });
            }
            if value.len() > value_limit {
                return Err(ReadError::ValueLimit { limit: value_limit });
            }
            append(
                &mut items,
                InternalValue {
                    key: key.clone(),
                    value: value.clone(),
                },
                call,
                operation,
            )?;
        }
    }
    let block_limits = BlockLimits {
        max_encoded: usize::MAX,
        max_decoded: usize::MAX,
        max_value: usize::MAX,
        max_key: usize::MAX,
        max_items: u32::MAX,
        max_allocation: usize::MAX,
    };
    for level in &levels.levels {
        for segment in &level.segments {
            charge(call, operation, BudgetKind::VisitedItems, 1)?;
            if segment.metadata.key_range.max().as_ref() < lower
                || if point {
                    segment.metadata.key_range.min().as_ref() > lower
                } else {
                    upper.is_some_and(|end| segment.metadata.key_range.min().as_ref() >= end)
                }
            {
                continue;
            }
            let path_capacity = crate::bounded_tree::os_path_capacity(&segment.path).map_err(
                |error| match error {
                    crate::bounded_prepare::PrepareError::Unsupported(s) => {
                        ReadError::Unsupported(s)
                    }
                    _ => ReadError::Corruption,
                },
            )? as u64;
            charge(call, operation, BudgetKind::MetadataBytes, path_capacity)?;
            charge(call, operation, BudgetKind::AllocationBytes, path_capacity)?;
            let mut file = File::open(&segment.path).map_err(ReadError::Io)?;
            let index_start = *segment.offsets.index_block_ptr;
            file.seek(SeekFrom::Start(index_start))
                .map_err(ReadError::Io)?;
            let mut previous: Option<Slice> = None;
            let mut previous_offset: Option<u64> = None;
            let mut finished = false;
            for _ in 0..segment.metadata.index_block_count {
                charge(call, operation, BudgetKind::VisitedItems, 1)?;
                let index = crate::bounded_read_index::read_index_until(
                    &mut file,
                    call,
                    operation,
                    Some(*segment.offsets.metadata_ptr),
                )?;
                let next_index = file.stream_position().map_err(ReadError::Io)?;
                if next_index > *segment.offsets.metadata_ptr || index.is_empty() {
                    return Err(ReadError::Corruption);
                }
                for handle in index {
                    charge(call, operation, BudgetKind::VisitedItems, 1)?;
                    if previous.as_ref().is_some_and(|key| key > &handle.end_key)
                        || previous_offset.is_some_and(|offset| offset >= *handle.offset)
                        || *handle.offset >= index_start
                    {
                        return Err(ReadError::Corruption);
                    }
                    let beyond = previous.as_ref().is_some_and(|key| {
                        if point {
                            key.as_ref() > lower
                        } else {
                            upper.is_some_and(|end| key.as_ref() >= end)
                        }
                    });
                    if beyond {
                        finished = true;
                        break;
                    }
                    let selected = handle.end_key.as_ref() >= lower;
                    previous_offset = Some(*handle.offset);
                    previous = Some(handle.end_key);
                    if !selected {
                        continue;
                    }
                    if handle
                        .offset
                        .checked_add(crate::segment::block::header::Header::serialized_len() as u64)
                        .is_none_or(|end| end > index_start)
                    {
                        return Err(ReadError::Corruption);
                    }
                    file.seek(SeekFrom::Start(*handle.offset))
                        .map_err(ReadError::Io)?;
                    let block = read_data_block_budgeted_until(
                        &mut file,
                        &block_limits,
                        call,
                        operation,
                        Some(index_start),
                    )
                    .map_err(|e| block_error(e, call, operation))?;
                    if file.stream_position().map_err(ReadError::Io)? > index_start {
                        return Err(ReadError::Corruption);
                    }
                    for entry in block.items.into_vec() {
                        if in_range(&entry.key.user_key) && visible(&entry) {
                            append(&mut items, entry, call, operation)?;
                        }
                    }
                }
                if finished {
                    break;
                }
                file.seek(SeekFrom::Start(next_index))
                    .map_err(ReadError::Io)?;
            }
        }
    }
    // Stable bottom-up merge sort; the scratch vector and the worst-case
    // number of comparisons are charged before any sorting or cloning.
    let n = items.len();
    if n > 1 {
        let passes = (usize::BITS - (n - 1).leading_zeros()) as usize;
        let comparisons = n.checked_mul(passes).ok_or(ReadError::Corruption)?;
        charge(
            call,
            operation,
            BudgetKind::VisitedItems,
            comparisons as u64,
        )?;
        let actual_max_key = items
            .iter()
            .map(|item| item.key.user_key.len())
            .max()
            .unwrap_or(0);
        let work = comparisons
            .checked_mul(actual_max_key.saturating_mul(2))
            .ok_or(ReadError::Corruption)?;
        charge(call, operation, BudgetKind::DecodeBytes, work as u64)?;
        let capacity = n
            .checked_mul(std::mem::size_of::<InternalValue>())
            .ok_or(ReadError::Corruption)?;
        charge(call, operation, BudgetKind::MetadataBytes, capacity as u64)?;
        charge(
            call,
            operation,
            BudgetKind::AllocationBytes,
            capacity as u64,
        )?;
        let mut scratch = Vec::new();
        scratch
            .try_reserve_exact(n)
            .map_err(|_| ReadError::Unsupported("sort allocation failed"))?;
        let mut width = 1usize;
        while width < n {
            let mut start = 0usize;
            while start < n {
                let mid = start.saturating_add(width).min(n);
                let end = mid.saturating_add(width).min(n);
                let (mut left, mut right) = (start, mid);
                while left < mid || right < end {
                    let take_left = if right == end {
                        true
                    } else if left == mid {
                        false
                    } else {
                        let a = &items[left];
                        let b = &items[right];
                        a.key
                            .user_key
                            .cmp(&b.key.user_key)
                            .then_with(|| b.key.seqno.cmp(&a.key.seqno))
                            != Ordering::Greater
                    };
                    if take_left {
                        scratch.push(items[left].clone());
                        left += 1;
                    } else {
                        scratch.push(items[right].clone());
                        right += 1;
                    }
                }
                start = end;
            }
            std::mem::swap(&mut items, &mut scratch);
            scratch.clear();
            width = width.checked_mul(2).ok_or(ReadError::Corruption)?;
        }
    }
    Ok(items)
}
/// Read one value under a per-call and shared-operation ledger.
#[doc(hidden)]
pub fn get(
    tree: &Tree,
    key: &[u8],
    seqno: Option<SeqNo>,
    call: &mut Budget,
    operation: &mut Budget,
) -> Result<Option<Slice>, ReadError> {
    let items = collect(tree, key, None, true, seqno, call, operation)?;
    Ok(items
        .into_iter()
        .find(|item| item.key.user_key.as_ref() == key)
        .and_then(|item| {
            if item.is_tombstone() {
                None
            } else {
                Some(item.value)
            }
        }))
}
/// Return all visible keys in a half-open interval, or an explicit refusal.
#[doc(hidden)]
pub fn scan(
    tree: &Tree,
    lower: &[u8],
    upper: Option<&[u8]>,
    seqno: Option<SeqNo>,
    call: &mut Budget,
    operation: &mut Budget,
) -> Result<Vec<Slice>, ReadError> {
    let items = collect(tree, lower, upper, false, seqno, call, operation)?;
    let mut keys = Vec::new();
    let mut previous: Option<Slice> = None;
    for item in items {
        if previous
            .as_ref()
            .is_some_and(|key| key == &item.key.user_key)
        {
            continue;
        }
        previous = Some(item.key.user_key.clone());
        if item.is_tombstone() {
            continue;
        }
        charge(call, operation, BudgetKind::OutputItems, 1)?;
        charge(
            call,
            operation,
            BudgetKind::OutputKeyBytes,
            item.key.user_key.len() as u64,
        )?;
        if keys.len() == keys.capacity() {
            let next = keys
                .capacity()
                .max(2usize)
                .checked_mul(2)
                .ok_or(ReadError::Corruption)?;
            let bytes = next
                .checked_mul(std::mem::size_of::<Slice>())
                .ok_or(ReadError::Corruption)? as u64;
            charge(call, operation, BudgetKind::MetadataBytes, bytes)?;
            charge(call, operation, BudgetKind::AllocationBytes, bytes)?;
            keys.try_reserve_exact(next - keys.len())
                .map_err(|_| ReadError::Unsupported("output allocation failed"))?;
        }
        keys.push(item.key.user_key);
    }
    Ok(keys)
}

/// Test whether a half-open key interval contains a visible entry.
///
/// # Errors
/// Returns a typed budget, format, or I/O refusal.
#[doc(hidden)]
pub fn exists(
    tree: &Tree,
    lower: &[u8],
    upper: Option<&[u8]>,
    seqno: Option<SeqNo>,
    call: &mut Budget,
    operation: &mut Budget,
) -> Result<bool, ReadError> {
    let items = collect(tree, lower, upper, false, seqno, call, operation)?;
    let mut previous: Option<Slice> = None;
    for item in items {
        if previous
            .as_ref()
            .is_some_and(|key| key == &item.key.user_key)
        {
            continue;
        }
        previous = Some(item.key.user_key.clone());
        if !item.is_tombstone() {
            return Ok(true);
        }
    }
    Ok(false)
}
