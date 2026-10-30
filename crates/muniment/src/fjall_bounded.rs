// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Bounded operations for the concrete Fjall owner.

use super::{DEFAULT_PARTITION, FjallStore};
use crate::{
    FjallBoundError, FjallOwnerLimits, FjallReadBudget, FjallReadLimits, StoreError,
    StoreLimitKind as Kind,
};
use fjall::{
    Config, PartitionCreateOptions, Slice,
    bounded_budget::{BudgetError, BudgetKind},
    bounded_read::ReadError,
    bounded_recovery::{RecoveryError, RecoveryLimitKind, RecoveryLimits},
};
use std::path::Path;

fn limit(kind: Kind, limit: u64) -> FjallBoundError {
    FjallBoundError::Limit { kind, limit }
}
fn budget_error(error: BudgetError) -> FjallBoundError {
    let kind = match error.kind {
        BudgetKind::InputBytes => Kind::InputBytes,
        BudgetKind::DecodeBytes => Kind::DecodeBytes,
        BudgetKind::AllocationBytes => Kind::AllocationBytes,
        BudgetKind::MetadataBytes => Kind::MetadataBytes,
        BudgetKind::VisitedItems => Kind::VisitedItems,
        BudgetKind::OutputItems => Kind::OutputItems,
        BudgetKind::OutputKeyBytes => Kind::OutputKeyBytes,
    };
    limit(kind, error.limit)
}
fn read_error(error: ReadError) -> FjallBoundError {
    match error {
        ReadError::Budget(e) => budget_error(e),
        ReadError::ValueLimit { limit: n } => limit(Kind::ValueBytes, n as u64),
        ReadError::KeyLimit { limit: n } => limit(Kind::KeyBytes, n as u64),
        ReadError::EncodedLimit { limit: n } => limit(Kind::BlockEncodedBytes, n as u64),
        ReadError::DecodedLimit { limit: n } => limit(Kind::BlockDecodedBytes, n as u64),
        ReadError::Corruption => {
            FjallBoundError::Store(StoreError::Corruption("invalid Fjall data block".into()))
        }
        ReadError::Unsupported(s) => FjallBoundError::Store(StoreError::Unsupported(s.into())),
        ReadError::Io(e) => FjallBoundError::Store(StoreError::Backend(format!("fjall read: {e}"))),
    }
}
fn recovery_error(error: RecoveryError) -> StoreError {
    match error {
        RecoveryError::Limit { kind, limit } => StoreError::LimitExceeded {
            kind: match kind {
                RecoveryLimitKind::ValueBytes => Kind::ValueBytes,
                RecoveryLimitKind::KeyBytes => Kind::KeyBytes,
                RecoveryLimitKind::ReplayBytes => Kind::ReplayBytes,
                RecoveryLimitKind::ReplayItems => Kind::ReplayItems,
                RecoveryLimitKind::MetadataBytes => Kind::MetadataBytes,
                RecoveryLimitKind::InputBytes => Kind::InputBytes,
                RecoveryLimitKind::AllocationBytes => Kind::AllocationBytes,
            },
            limit,
        },
        RecoveryError::Unsupported(s) => StoreError::Unsupported(s.into()),
        RecoveryError::Corruption(s) => StoreError::Corruption(s),
        RecoveryError::Io(e) => StoreError::Backend(format!("fjall open: {e}")),
    }
}
fn debit(
    call: &mut FjallReadBudget,
    operation: &mut FjallReadBudget,
    kind: BudgetKind,
    amount: u64,
) -> Result<(), FjallBoundError> {
    FjallReadBudget::charge_both(call, operation, kind, amount).map_err(budget_error)
}
fn prefix_end(
    prefix: &str,
    call: &mut FjallReadBudget,
    operation: &mut FjallReadBudget,
) -> Result<Option<Vec<u8>>, FjallBoundError> {
    let key_limit = call
        .limits()
        .max_key_bytes
        .min(operation.limits().max_key_bytes);
    if prefix.len() > key_limit {
        return Err(limit(Kind::KeyBytes, key_limit as u64));
    }
    if prefix.is_empty() {
        return Ok(None);
    }
    let capacity = prefix.len();
    debit(call, operation, BudgetKind::MetadataBytes, capacity as u64)?;
    debit(
        call,
        operation,
        BudgetKind::AllocationBytes,
        capacity as u64,
    )?;
    let mut end = Vec::new();
    end.try_reserve_exact(capacity).map_err(|_| {
        FjallBoundError::Store(StoreError::Backend("prefix allocation failed".into()))
    })?;
    end.extend_from_slice(prefix.as_bytes());
    *end.last_mut().expect("nonempty UTF-8 prefix") += 1;
    Ok(Some(end))
}
fn strings(
    keys: Vec<Slice>,
    call: &mut FjallReadBudget,
    operation: &mut FjallReadBudget,
) -> Result<Vec<String>, FjallBoundError> {
    let capacity = keys
        .len()
        .checked_mul(std::mem::size_of::<String>())
        .ok_or_else(|| {
            FjallBoundError::Store(StoreError::Corruption("key count overflow".into()))
        })?;
    debit(call, operation, BudgetKind::MetadataBytes, capacity as u64)?;
    debit(
        call,
        operation,
        BudgetKind::AllocationBytes,
        capacity as u64,
    )?;
    let mut result = Vec::new();
    result.try_reserve_exact(keys.len()).map_err(|_| {
        FjallBoundError::Store(StoreError::Backend("key list allocation failed".into()))
    })?;
    for key in keys {
        let text = std::str::from_utf8(&key).map_err(|_| {
            FjallBoundError::Store(StoreError::Corruption("non-UTF-8 Fjall key".into()))
        })?;
        debit(
            call,
            operation,
            BudgetKind::AllocationBytes,
            text.len() as u64,
        )?;
        debit(call, operation, BudgetKind::OutputKeyBytes, 0)?;
        let mut value = String::new();
        value.try_reserve_exact(text.len()).map_err(|_| {
            FjallBoundError::Store(StoreError::Backend("key allocation failed".into()))
        })?;
        value.push_str(text);
        result.push(value);
    }
    Ok(result)
}
impl FjallStore {
    pub(super) fn admit_write(&self, key: &str, value: &[u8]) -> Result<(), StoreError> {
        let Some(owner) = &self.owner_limits else {
            return Ok(());
        };
        let value_limit = owner
            .read
            .max_value_bytes
            .min(owner.recovery.max_value_bytes);
        if value.len() > value_limit {
            return Err(StoreError::LimitExceeded {
                kind: Kind::ValueBytes,
                limit: value_limit as u64,
            });
        }
        self.admit_key(key)
    }
    pub(super) fn admit_key(&self, key: &str) -> Result<(), StoreError> {
        let Some(owner) = &self.owner_limits else {
            return Ok(());
        };
        let key_limit = owner.read.max_key_bytes.min(u16::MAX as usize);
        if key.len() > key_limit {
            return Err(StoreError::LimitExceeded {
                kind: Kind::KeyBytes,
                limit: key_limit as u64,
            });
        }
        if key
            .len()
            .checked_add(self.partition.name.len())
            .is_none_or(|n| n > owner.recovery.max_key_bytes)
        {
            return Err(StoreError::LimitExceeded {
                kind: Kind::KeyBytes,
                limit: owner.recovery.max_key_bytes as u64,
            });
        }
        Ok(())
    }
    /// Open a keyspace with owner recovery and read ceilings.
    pub fn open_with_limits(
        path: impl AsRef<Path>,
        limits: FjallOwnerLimits,
    ) -> Result<Self, StoreError> {
        let r = &limits.recovery;
        let recovery = RecoveryLimits {
            max_value_bytes: r.max_value_bytes,
            max_key_bytes: r.max_key_bytes,
            max_replay_bytes: r.max_replay_bytes,
            max_replay_items: r.max_replay_items,
            max_metadata_bytes: r.max_metadata_bytes,
            max_input_bytes: r.max_input_bytes,
            max_alloc_bytes: r.max_alloc_bytes,
        };
        let keyspace = Config::new(path)
            .open_with_recovery_limits(recovery)
            .map_err(recovery_error)?;
        let partition = keyspace
            .open_partition(DEFAULT_PARTITION, PartitionCreateOptions::default())
            .map_err(|e| StoreError::Backend(format!("fjall partition: {e}")))?;
        Ok(Self {
            keyspace,
            partition,
            owner_limits: Some(limits),
        })
    }
    fn call_budget(&self, limits: &FjallReadLimits) -> FjallReadBudget {
        FjallReadBudget::new(
            self.owner_limits
                .as_ref()
                .map_or_else(|| limits.clone(), |owner| owner.read.intersect(limits)),
        )
    }
    /// Get a value under an individual call ceiling and shared operation ledger.
    pub fn get_bounded(
        &self,
        key: &str,
        limits: &FjallReadLimits,
        operation: &mut FjallReadBudget,
    ) -> Result<Option<Vec<u8>>, FjallBoundError> {
        let mut call = self.call_budget(limits);
        let value = self
            .partition
            .get_bounded(key.as_bytes(), &mut call, operation)
            .map_err(read_error)?;
        value
            .map(|value| {
                debit(
                    &mut call,
                    operation,
                    BudgetKind::AllocationBytes,
                    value.len() as u64,
                )?;
                debit(
                    &mut call,
                    operation,
                    BudgetKind::DecodeBytes,
                    value.len() as u64,
                )?;
                let mut output = Vec::new();
                output.try_reserve_exact(value.len()).map_err(|_| {
                    FjallBoundError::Store(StoreError::Backend("value allocation failed".into()))
                })?;
                output.extend_from_slice(&value);
                Ok(output)
            })
            .transpose()
    }
    /// Return every key matching a prefix, or a typed refusal.
    pub fn list_bounded(
        &self,
        prefix: &str,
        limits: &FjallReadLimits,
        operation: &mut FjallReadBudget,
    ) -> Result<Vec<String>, FjallBoundError> {
        let mut call = self.call_budget(limits);
        let end = prefix_end(prefix, &mut call, operation)?;
        let keys = self
            .partition
            .scan_bounded(prefix.as_bytes(), end.as_deref(), &mut call, operation)
            .map_err(read_error)?;
        strings(keys, &mut call, operation)
    }
    /// Probe whether a prefix has a visible key, without creating an output list.
    pub fn exists_bounded(
        &self,
        prefix: &str,
        limits: &FjallReadLimits,
        operation: &mut FjallReadBudget,
    ) -> Result<bool, FjallBoundError> {
        let mut call = self.call_budget(limits);
        let end = prefix_end(prefix, &mut call, operation)?;
        self.partition
            .exists_bounded(prefix.as_bytes(), end.as_deref(), &mut call, operation)
            .map_err(read_error)
    }
    /// Return every key in a half-open interval, or a typed refusal.
    pub fn scan_bounded(
        &self,
        start: &str,
        end: &str,
        limits: &FjallReadLimits,
        operation: &mut FjallReadBudget,
    ) -> Result<Vec<String>, FjallBoundError> {
        let mut call = self.call_budget(limits);
        let keys = self
            .partition
            .scan_bounded(start.as_bytes(), Some(end.as_bytes()), &mut call, operation)
            .map_err(read_error)?;
        strings(keys, &mut call, operation)
    }
}
