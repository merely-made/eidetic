// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Limits for the opt-in bounded Fjall owner.

use crate::StoreError;
pub use crate::StoreLimitKind as FjallLimitKind;
pub use fjall::bounded_budget::{Budget as FjallReadBudget, ReadLimits as FjallReadLimits};

/// Limits applied to recovery of the complete keyspace.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FjallRecoveryLimits {
    pub max_value_bytes: usize,
    pub max_key_bytes: usize,
    pub max_replay_bytes: u64,
    pub max_replay_items: u64,
    pub max_metadata_bytes: u64,
    pub max_input_bytes: u64,
    pub max_alloc_bytes: u64,
}

/// Recovery and per-call ceilings installed on a bounded owner.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FjallOwnerLimits {
    pub recovery: FjallRecoveryLimits,
    pub read: FjallReadLimits,
}

/// A bounded operation refused a resource request or encountered a store error.
#[derive(Debug, Clone, PartialEq)]
pub enum FjallBoundError {
    Limit { kind: FjallLimitKind, limit: u64 },
    Store(StoreError),
}

impl std::fmt::Display for FjallBoundError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Limit { kind, limit } => write!(f, "fjall {kind:?} limit exceeded ({limit})"),
            Self::Store(error) => error.fmt(f),
        }
    }
}
impl std::error::Error for FjallBoundError {}
impl From<FjallBoundError> for StoreError {
    fn from(value: FjallBoundError) -> Self {
        match value {
            FjallBoundError::Limit { kind, limit } => Self::LimitExceeded { kind, limit },
            FjallBoundError::Store(error) => error,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bound_error_converts_without_losing_identity() {
        let error: StoreError = FjallBoundError::Limit { kind: FjallLimitKind::ValueBytes, limit: 4 }.into();
        assert_eq!(error, StoreError::LimitExceeded { kind: FjallLimitKind::ValueBytes, limit: 4 });
    }
}
