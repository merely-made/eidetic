// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Shared recovery-preparation accounting types and admitted tree preparation.

/// A cumulative resource charged while preparing an existing tree.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecoveryResource {
    /// Bytes read from stored files.
    InputBytes,
    /// Requested allocation capacity.
    AllocationBytes,
    /// Metadata allocation capacity, also charged as allocation.
    MetadataBytes,
    /// Physical entries and filesystem items inspected.
    VisitedItems,
    /// Individual LSM user-key bytes; enclosing owners add their partition prefix.
    KeyBytes,
}

/// A preparation refusal or underlying storage error.
#[derive(Debug)]
pub enum PrepareError {
    /// A configured limit refused a resource request.
    Limit {
        /// Refused resource.
        resource: RecoveryResource,
        /// Configured ceiling.
        limit: u64,
    },
    /// A stored format is not supported by bounded preparation.
    Unsupported(&'static str),
    /// Stored bytes are corrupt.
    Corruption(String),
    /// An underlying file operation failed.
    Io(std::io::Error),
}

/// A recovery-wide ledger provided by the enclosing storage owner.
pub trait RecoveryBudget {
    /// Charge work or allocation capacity before it is performed.
    ///
    /// # Errors
    /// Returns a typed limit refusal or another preparation error.
    fn charge(&mut self, resource: RecoveryResource, amount: u64) -> Result<(), PrepareError>;

    /// Check one LSM user-key length against the enclosing owner's per-key cap.
    ///
    /// # Errors
    /// Returns a typed limit refusal when the individual key is too long.
    fn check_key_bytes(&mut self, amount: u64) -> Result<(), PrepareError>;
}

/// An admitted in-memory tree and deferred cleanup.
pub use crate::bounded_tree::PreparedTree;

/// Admit a standard tree without mutating its files.
///
/// # Errors
/// Returns a typed limit, unsupported-format, corruption, or I/O error.
pub fn prepare(
    config: crate::Config,
    budget: &mut dyn RecoveryBudget,
) -> Result<PreparedTree, PrepareError> {
    crate::bounded_tree::prepare(config, budget)
}
