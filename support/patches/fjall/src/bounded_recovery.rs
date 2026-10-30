// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Read-only admission for bounded keyspace recovery.

pub(crate) mod store;

/// Resource ceilings for admission of the whole keyspace.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecoveryLimits {
    /// Maximum individual value length.
    pub max_value_bytes: usize,
    /// Maximum sum of partition and user-key lengths.
    pub max_key_bytes: usize,
    /// Maximum cumulative decoded journal bytes.
    pub max_replay_bytes: u64,
    /// Maximum cumulative journal marker/item and metadata visits.
    pub max_replay_items: u64,
    /// Maximum cumulative metadata capacity.
    pub max_metadata_bytes: u64,
    /// Maximum cumulative input bytes accessed.
    pub max_input_bytes: u64,
    /// Maximum cumulative requested allocation capacity.
    pub max_alloc_bytes: u64,
}

/// Identifies a recovery ceiling.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecoveryLimitKind {
    /// Individual value length.
    ValueBytes,
    /// Combined partition and key length.
    KeyBytes,
    /// Decoded replay bytes.
    ReplayBytes,
    /// Journal marker/item and metadata visits.
    ReplayItems,
    /// Metadata allocations.
    MetadataBytes,
    /// Input bytes.
    InputBytes,
    /// Requested allocation capacity.
    AllocationBytes,
}

/// A bounded recovery preparation failure.
#[derive(Debug)]
pub enum RecoveryError {
    /// A configured recovery ceiling was exceeded.
    Limit {
        /// The exhausted resource.
        kind: RecoveryLimitKind,
        /// The configured ceiling.
        limit: u64,
    },
    /// Input requires a feature not supported by bounded recovery.
    Unsupported(&'static str),
    /// Input is corrupt.
    Corruption(String),
    /// An operating-system operation failed.
    Io(std::io::Error),
}

impl std::fmt::Display for RecoveryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "bounded recovery: {self:?}")
    }
}

impl std::error::Error for RecoveryError {}

/// Counters shared across every input in one recovery admission.
#[derive(Debug)]
pub struct RecoveryBudget {
    pub(crate) limits: RecoveryLimits,
    pub(crate) identity: std::sync::Arc<()>,
    pub(crate) replay_bytes: u64,
    pub(crate) replay_items: u64,
    pub(crate) metadata_bytes: u64,
    pub(crate) input_bytes: u64,
    pub(crate) alloc_bytes: u64,
}

impl RecoveryBudget {
    /// Start an admission ledger.
    #[must_use]
    pub fn new(limits: RecoveryLimits) -> Self {
        Self {
            limits,
            identity: std::sync::Arc::new(()),
            replay_bytes: 0,
            replay_items: 0,
            metadata_bytes: 0,
            input_bytes: 0,
            alloc_bytes: 0,
        }
    }

    /// Allocation capacity charged so far, including work before a refusal.
    #[must_use]
    pub fn allocated_bytes(&self) -> u64 {
        self.alloc_bytes
    }

    /// Metadata capacity charged so far, including work before a refusal.
    #[doc(hidden)]
    #[must_use]
    pub fn metadata_bytes(&self) -> u64 {
        self.metadata_bytes
    }

    /// Tighten the remaining allocation ceiling without resetting prior charges.
    #[doc(hidden)]
    pub fn tighten_allocation_limit(&mut self, limit: u64) {
        self.limits.max_alloc_bytes = self.limits.max_alloc_bytes.min(limit);
    }

    pub(crate) fn charge(
        &mut self,
        kind: RecoveryLimitKind,
        amount: u64,
    ) -> Result<(), RecoveryError> {
        let (counter, limit) = match kind {
            RecoveryLimitKind::ReplayBytes => {
                (&mut self.replay_bytes, self.limits.max_replay_bytes)
            }
            RecoveryLimitKind::ReplayItems => {
                (&mut self.replay_items, self.limits.max_replay_items)
            }
            RecoveryLimitKind::MetadataBytes => {
                (&mut self.metadata_bytes, self.limits.max_metadata_bytes)
            }
            RecoveryLimitKind::InputBytes => (&mut self.input_bytes, self.limits.max_input_bytes),
            RecoveryLimitKind::AllocationBytes => {
                (&mut self.alloc_bytes, self.limits.max_alloc_bytes)
            }
            RecoveryLimitKind::KeyBytes | RecoveryLimitKind::ValueBytes => unreachable!(),
        };
        let next = counter
            .checked_add(amount)
            .filter(|n| *n <= limit)
            .ok_or(RecoveryError::Limit { kind, limit })?;
        *counter = next;
        Ok(())
    }

    pub(crate) fn metadata(&mut self, amount: u64) -> Result<(), RecoveryError> {
        self.charge(RecoveryLimitKind::MetadataBytes, amount)?;
        self.charge(RecoveryLimitKind::AllocationBytes, amount)
    }
}

// LSM keys do not contain their partition name. Keep that context with the
// adapter so every individual key is checked against the combined ceiling.
pub(crate) struct PartitionRecoveryBudget<'a> {
    pub(crate) ledger: &'a mut RecoveryBudget,
    pub(crate) partition_bytes: u64,
}

impl lsm_tree::bounded_prepare::RecoveryBudget for PartitionRecoveryBudget<'_> {
    fn charge(
        &mut self,
        resource: lsm_tree::bounded_prepare::RecoveryResource,
        amount: u64,
    ) -> Result<(), lsm_tree::bounded_prepare::PrepareError> {
        use lsm_tree::bounded_prepare::{PrepareError, RecoveryResource};
        let kind = match resource {
            RecoveryResource::InputBytes => RecoveryLimitKind::InputBytes,
            RecoveryResource::AllocationBytes => RecoveryLimitKind::AllocationBytes,
            RecoveryResource::MetadataBytes => RecoveryLimitKind::MetadataBytes,
            RecoveryResource::VisitedItems => RecoveryLimitKind::ReplayItems,
            RecoveryResource::KeyBytes => return self.check_key_bytes(amount),
        };
        self.ledger
            .charge(kind, amount)
            .map_err(|error| match error {
                RecoveryError::Limit { limit, .. } => PrepareError::Limit { resource, limit },
                RecoveryError::Unsupported(reason) => PrepareError::Unsupported(reason),
                RecoveryError::Corruption(reason) => PrepareError::Corruption(reason),
                RecoveryError::Io(error) => PrepareError::Io(error),
            })
    }

    fn check_key_bytes(
        &mut self,
        amount: u64,
    ) -> Result<(), lsm_tree::bounded_prepare::PrepareError> {
        use lsm_tree::bounded_prepare::{PrepareError, RecoveryResource};
        let limit = self.ledger.limits.max_key_bytes as u64;
        match self.partition_bytes.checked_add(amount) {
            Some(total) if total <= limit => Ok(()),
            _ => Err(PrepareError::Limit {
                resource: RecoveryResource::KeyBytes,
                limit,
            }),
        }
    }
}

/// A retained, read-only journal admission result.
pub struct JournalPlan {
    pub(crate) batches: Vec<crate::journal::bounded::PreparedBatch>,
    pub(crate) truncate_to: Option<u64>,
}

impl JournalPlan {
    /// Number of complete entries admitted from this journal.
    #[must_use]
    pub fn item_count(&self) -> usize {
        self.batches.iter().map(|batch| batch.items.len()).sum()
    }
}

/// Admit a single journal without changing its bytes.
///
/// This does not by itself admit a keyspace or authorize any recovery mutation.
///
/// # Errors
/// Returns a typed refusal, I/O error, or corruption.
pub fn prepare_journal(
    path: impl AsRef<std::path::Path>,
    budget: &mut RecoveryBudget,
) -> Result<JournalPlan, RecoveryError> {
    crate::journal::bounded::prepare(path.as_ref(), budget)
}

/// A read-only admission of journal files, retaining the owner lease.
///
/// This is an internal building block, not a whole-keyspace admission.
#[doc(hidden)]
pub struct JournalSetPlan {
    pub(crate) owner_lock: std::sync::Arc<crate::keyspace::OwnerLease>,
    pub(crate) budget_identity: std::sync::Arc<()>,
    pub(crate) journals: Vec<(u64, std::path::PathBuf, JournalPlan)>,
}

impl JournalSetPlan {
    /// Number of journals admitted.
    #[must_use]
    pub fn journal_count(&self) -> usize {
        self.journals.len()
    }

    /// Number of complete journal entries admitted.
    #[must_use]
    pub fn item_count(&self) -> usize {
        self.journals
            .iter()
            .map(|(_, _, plan)| plan.item_count())
            .sum()
    }

    /// Prepare retained in-memory replay for a set of admitted partitions.
    ///
    /// # Errors
    /// Returns a typed refusal or unsupported input before any store mutation.
    pub fn prepare_replay(
        &self,
        partitions: &[ReplayPartition<'_>],
        budget: &mut RecoveryBudget,
    ) -> Result<ReplayPlan, RecoveryError> {
        if !std::sync::Arc::ptr_eq(&self.budget_identity, &budget.identity) {
            return Err(RecoveryError::Unsupported(
                "recovery ledger does not match journal admission",
            ));
        }
        crate::journal::bounded_replay::prepare(self, partitions, budget)
    }
}

/// Persisted sequence state for one standard partition.
#[doc(hidden)]
pub struct ReplayPartition<'a> {
    /// Stored partition name.
    pub name: &'a str,
    /// Highest sequence already persisted in that partition's segments.
    pub highest_persisted_seqno: Option<u64>,
}

/// In-memory replay state, built before any recovery mutation.
#[doc(hidden)]
pub struct ReplayPlan {
    pub(crate) partitions: Vec<crate::journal::bounded_replay::PreparedPartition>,
    // Retain the lease even if the journal plan is dropped before replay.
    pub(crate) _owner_lock: std::sync::Arc<crate::keyspace::OwnerLease>,
}

impl ReplayPlan {
    /// Inspect the newest replayed value for a key; persisted segments are not read.
    #[must_use]
    pub fn get(&self, partition: &str, key: &[u8]) -> Option<crate::Slice> {
        crate::journal::bounded_replay::get(self, partition, key)
    }
}

/// Prepare every journal under a shared budget without mutating the store.
///
/// This admits journal files only; partition metadata and replay are separate.
///
/// # Errors
/// Returns a typed refusal or error before applying recovery actions.
#[doc(hidden)]
pub fn prepare_journal_set(
    path: impl AsRef<std::path::Path>,
    budget: &mut RecoveryBudget,
) -> Result<JournalSetPlan, RecoveryError> {
    crate::journal::bounded_set::prepare(path.as_ref(), budget)
}

/// Retained partition options admitted without mutating the partition.
#[doc(hidden)]
pub struct PartitionOptionsPlan {
    pub(crate) options: crate::PartitionCreateOptions,
}

/// Prepare a v2 standard-partition configuration without changing its files.
///
/// This is one component of admission, not whole-store recovery.
///
/// # Errors
/// Returns a typed limit, unsupported-format, corruption or I/O error.
#[doc(hidden)]
pub fn prepare_partition_options(
    partition_dir: impl AsRef<std::path::Path>,
    budget: &mut RecoveryBudget,
) -> Result<PartitionOptionsPlan, RecoveryError> {
    crate::partition::bounded_options::prepare(partition_dir.as_ref(), budget)
}
