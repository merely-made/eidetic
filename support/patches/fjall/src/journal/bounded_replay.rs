// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Build replay memtables before any recovery actions are applied.

use crate::bounded_recovery::{
    JournalSetPlan, RecoveryBudget, RecoveryError, RecoveryLimitKind as Kind, ReplayPartition,
    ReplayPlan,
};
use lsm_tree::{InternalValue, Memtable, ValueType};
use std::{
    io,
    sync::atomic::{AtomicU64, AtomicUsize},
};

// crossbeam-skiplist 0.1.3's repr(C) node stores InternalValue's key and
// value fields, one AtomicUsize, and up to 32 crossbeam-epoch Atomic pointers.
// In the pinned epoch implementation Atomic wraps one AtomicUsize and a
// zero-sized PhantomData. InternalValue's size bounds the two field sizes;
// four maximum-alignment units conservatively cover all node layout padding.
fn max_skip_node_bytes() -> usize {
    let alignment = std::mem::align_of::<InternalValue>().max(std::mem::align_of::<AtomicUsize>());
    std::mem::size_of::<InternalValue>() + 33 * std::mem::size_of::<AtomicUsize>() + 4 * alignment
}

pub(crate) struct PreparedMemtable {
    pub(crate) journal_id: u64,
    pub(crate) watermark: u64,
    pub(crate) active: bool,
    pub(crate) skipped: bool,
    pub(crate) table: Memtable,
}

pub(crate) struct PreparedPartition {
    pub(crate) name: String,
    pub(crate) tables: Vec<PreparedMemtable>,
}

fn checked_capacity<T>(budget: &mut RecoveryBudget, count: usize) -> Result<Vec<T>, RecoveryError> {
    let size = count
        .checked_mul(std::mem::size_of::<T>())
        .ok_or(RecoveryError::Limit {
            kind: Kind::MetadataBytes,
            limit: budget.limits.max_metadata_bytes,
        })?;
    budget.metadata(size as u64)?;
    let mut vec = Vec::new();
    vec.try_reserve_exact(count).map_err(|_| {
        RecoveryError::Io(io::Error::new(
            io::ErrorKind::OutOfMemory,
            "replay metadata allocation failed",
        ))
    })?;
    Ok(vec)
}

pub(crate) fn prepare(
    set: &JournalSetPlan,
    specs: &[ReplayPartition<'_>],
    budget: &mut RecoveryBudget,
) -> Result<ReplayPlan, RecoveryError> {
    // The optional bytes-backed Slice has a different clone allocation path;
    // do not claim admission for it until that representation is audited.
    if cfg!(feature = "bytes") {
        return Err(RecoveryError::Unsupported(
            "bytes-backed replay is not audited",
        ));
    }
    let mut partitions = checked_capacity::<PreparedPartition>(budget, specs.len())?;
    for (index, spec) in specs.iter().enumerate() {
        budget.charge(Kind::ReplayItems, 1)?;
        if spec.name.is_empty() {
            return Err(RecoveryError::Corruption(
                "invalid or duplicate replay partition".into(),
            ));
        }
        for other in &specs[..index] {
            budget.charge(Kind::ReplayItems, 1)?;
            if other.name == spec.name {
                return Err(RecoveryError::Corruption(
                    "invalid or duplicate replay partition".into(),
                ));
            }
        }
        if spec.name.len() > budget.limits.max_key_bytes {
            return Err(RecoveryError::Limit {
                kind: Kind::KeyBytes,
                limit: budget.limits.max_key_bytes as u64,
            });
        }
        budget.metadata(spec.name.len() as u64)?;
        let mut name = String::new();
        name.try_reserve_exact(spec.name.len()).map_err(|_| {
            RecoveryError::Io(io::Error::new(
                io::ErrorKind::OutOfMemory,
                "partition name allocation failed",
            ))
        })?;
        name.push_str(spec.name);
        let mut tables = checked_capacity::<PreparedMemtable>(budget, set.journals.len())?;
        for (position, (journal_id, _, journal)) in set.journals.iter().enumerate() {
            let active = position + 1 == set.journals.len();
            let mut watermark: Option<u64> = None;
            for batch in &journal.batches {
                for item in &batch.items {
                    budget.charge(Kind::ReplayItems, 1)?;
                    if item.partition == spec.name {
                        watermark = Some(watermark.map_or(batch.seqno, |old| old.max(batch.seqno)));
                    }
                }
            }
            let Some(watermark) = watermark else {
                continue;
            };
            let skipped = !active
                && spec
                    .highest_persisted_seqno
                    .is_some_and(|persisted| persisted >= watermark);
            let table = Memtable::default();
            if !skipped {
                let mut table_bytes = 0_u32;
                for batch in &journal.batches {
                    for item in &batch.items {
                        budget.charge(Kind::ReplayItems, 1)?;
                        if item.partition != spec.name {
                            continue;
                        }
                        if item.key.is_empty() {
                            return Err(RecoveryError::Corruption("empty journal key".into()));
                        }
                        let key_len = item.partition.len().checked_add(item.key.len()).ok_or(
                            RecoveryError::Limit {
                                kind: Kind::KeyBytes,
                                limit: budget.limits.max_key_bytes as u64,
                            },
                        )?;
                        if key_len > budget.limits.max_key_bytes {
                            return Err(RecoveryError::Limit {
                                kind: Kind::KeyBytes,
                                limit: budget.limits.max_key_bytes as u64,
                            });
                        }
                        if item.value.len() > budget.limits.max_value_bytes {
                            return Err(RecoveryError::Limit {
                                kind: Kind::ValueBytes,
                                limit: budget.limits.max_value_bytes as u64,
                            });
                        }
                        let value: &[u8] = if item.value_type == ValueType::Value {
                            &item.value
                        } else {
                            &[]
                        };
                        let value_len = item.key.len().checked_add(value.len()).ok_or(
                            RecoveryError::Limit {
                                kind: Kind::ReplayBytes,
                                limit: budget.limits.max_replay_bytes,
                            },
                        )?;
                        budget.charge(Kind::ReplayBytes, value_len as u64)?;
                        // ByteView 0.6.1 allocates a u64 reference-count header
                        // plus the bytes (or inlines small values); charge the
                        // heap case conservatively for both copied slices.
                        let copies = value_len
                            .checked_add(2 * std::mem::size_of::<AtomicU64>())
                            .ok_or(RecoveryError::Limit {
                                kind: Kind::AllocationBytes,
                                limit: budget.limits.max_alloc_bytes,
                            })?;
                        budget.charge(Kind::AllocationBytes, copies as u64)?;
                        budget.charge(Kind::AllocationBytes, max_skip_node_bytes() as u64)?;
                        let item_bytes = std::mem::size_of::<u64>()
                            .checked_add(std::mem::size_of::<ValueType>())
                            .and_then(|v| v.checked_add(value_len))
                            .and_then(|v| u32::try_from(v).ok())
                            .ok_or(RecoveryError::Unsupported(
                                "replayed memtable exceeds v2 size",
                            ))?;
                        table_bytes = table_bytes.checked_add(item_bytes).ok_or(
                            RecoveryError::Unsupported("replayed memtable exceeds v2 size"),
                        )?;
                        let record = InternalValue::from_components(
                            item.key.as_slice(),
                            value,
                            batch.seqno,
                            item.value_type,
                        );
                        table.insert(record);
                    }
                }
            }
            tables.push(PreparedMemtable {
                journal_id: *journal_id,
                watermark,
                active,
                skipped,
                table,
            });
        }
        partitions.push(PreparedPartition { name, tables });
    }
    Ok(ReplayPlan {
        partitions,
        _owner_lock: set.owner_lock.clone(),
    })
}

pub(crate) fn get(plan: &ReplayPlan, partition: &str, key: &[u8]) -> Option<crate::Slice> {
    let mut newest: Option<(u64, ValueType, crate::Slice)> = None;
    for part in &plan.partitions {
        if part.name != partition {
            continue;
        }
        for table in &part.tables {
            for entry in table.table.items.iter() {
                let item = entry.key();
                if item.user_key.as_ref() == key
                    && newest.as_ref().is_none_or(|(seq, _, _)| item.seqno >= *seq)
                {
                    newest = Some((item.seqno, item.value_type, entry.value().clone()));
                }
            }
        }
    }
    newest.and_then(|(_, kind, value)| (kind == ValueType::Value).then_some(value))
}
