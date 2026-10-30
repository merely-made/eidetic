// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Cumulative resource ledgers for bounded storage operations.

/// Limits for one bounded read operation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReadLimits {
    /// Largest separately materialized value.
    pub max_value_bytes: usize,
    /// Largest key.
    pub max_key_bytes: usize,
    /// Largest encoded block.
    pub max_block_encoded_bytes: usize,
    /// Largest decoded block.
    pub max_block_decoded_bytes: usize,
    /// Cumulative encoded input accessed.
    pub max_input_bytes: u64,
    /// Cumulative decoded bytes processed.
    pub max_decode_bytes: u64,
    /// Cumulative requested allocation capacity.
    pub max_alloc_bytes: u64,
    /// Cumulative metadata capacity, also counted as allocation.
    pub max_metadata_bytes: u64,
    /// Physical entries visited.
    pub max_visited_items: u64,
    /// Output key count.
    pub max_output_items: usize,
    /// Aggregate output key bytes.
    pub max_output_key_bytes: usize,
}

impl ReadLimits {
    /// Combine two ceilings, retaining the stricter value for each resource.
    #[must_use]
    pub fn intersect(&self, other: &Self) -> Self {
        Self {
            max_value_bytes: self.max_value_bytes.min(other.max_value_bytes),
            max_key_bytes: self.max_key_bytes.min(other.max_key_bytes),
            max_block_encoded_bytes: self.max_block_encoded_bytes.min(other.max_block_encoded_bytes),
            max_block_decoded_bytes: self.max_block_decoded_bytes.min(other.max_block_decoded_bytes),
            max_input_bytes: self.max_input_bytes.min(other.max_input_bytes),
            max_decode_bytes: self.max_decode_bytes.min(other.max_decode_bytes),
            max_alloc_bytes: self.max_alloc_bytes.min(other.max_alloc_bytes),
            max_metadata_bytes: self.max_metadata_bytes.min(other.max_metadata_bytes),
            max_visited_items: self.max_visited_items.min(other.max_visited_items),
            max_output_items: self.max_output_items.min(other.max_output_items),
            max_output_key_bytes: self.max_output_key_bytes.min(other.max_output_key_bytes),
        }
    }

    /// A per-call ceiling that leaves the owner's and operation's limits in force.
    #[must_use]
    pub const fn unlimited() -> Self {
        Self { max_value_bytes: usize::MAX, max_key_bytes: usize::MAX,
            max_block_encoded_bytes: usize::MAX, max_block_decoded_bytes: usize::MAX,
            max_input_bytes: u64::MAX, max_decode_bytes: u64::MAX,
            max_alloc_bytes: u64::MAX, max_metadata_bytes: u64::MAX,
            max_visited_items: u64::MAX, max_output_items: usize::MAX,
            max_output_key_bytes: usize::MAX }
    }
}

/// A cumulative ledger counter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BudgetKind {
    /// Encoded input bytes.
    InputBytes,
    /// Decoded bytes.
    DecodeBytes,
    /// Requested allocation capacity.
    AllocationBytes,
    /// Metadata capacity.
    MetadataBytes,
    /// Physical entries visited.
    VisitedItems,
    /// Output key count.
    OutputItems,
    /// Aggregate output key bytes.
    OutputKeyBytes,
}

/// A resource request rejected by one of the two ledgers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BudgetError {
    /// The refused resource.
    pub kind: BudgetKind,
    /// The configured ceiling on the refusing ledger.
    pub limit: u64,
}

/// Mutable cumulative operation ledger.
#[derive(Debug)]
pub struct Budget {
    /// Per-operation ceilings.
    pub(crate) limits: ReadLimits,
    input: u64,
    decode: u64,
    allocation: u64,
    metadata: u64,
    visited: u64,
    output: u64,
    output_key: u64,
}

impl Budget {
    /// Starts a ledger with no work charged.
    #[must_use]
    pub fn new(limits: ReadLimits) -> Self {
        Self { limits, input: 0, decode: 0, allocation: 0, metadata: 0, visited: 0, output: 0, output_key: 0 }
    }

    /// The immutable configured ceilings for this ledger.
    #[must_use]
    pub fn limits(&self) -> &ReadLimits { &self.limits }

    /// Cumulative units already charged.
    #[must_use]
    pub fn used(&self, kind: BudgetKind) -> u64 {
        match kind {
            BudgetKind::InputBytes => self.input,
            BudgetKind::DecodeBytes => self.decode,
            BudgetKind::AllocationBytes => self.allocation,
            BudgetKind::MetadataBytes => self.metadata,
            BudgetKind::VisitedItems => self.visited,
            BudgetKind::OutputItems => self.output,
            BudgetKind::OutputKeyBytes => self.output_key,
        }
    }

    /// Remaining cumulative work for a resource.
    #[must_use]
    pub fn remaining(&self, kind: BudgetKind) -> u64 { self.limit(kind).saturating_sub(self.used(kind)) }

    fn limit(&self, kind: BudgetKind) -> u64 {
        match kind {
            BudgetKind::InputBytes => self.limits.max_input_bytes,
            BudgetKind::DecodeBytes => self.limits.max_decode_bytes,
            BudgetKind::AllocationBytes => self.limits.max_alloc_bytes,
            BudgetKind::MetadataBytes => self.limits.max_metadata_bytes,
            BudgetKind::VisitedItems => self.limits.max_visited_items,
            BudgetKind::OutputItems => self.limits.max_output_items as u64,
            BudgetKind::OutputKeyBytes => self.limits.max_output_key_bytes as u64,
        }
    }

    fn used_mut(&mut self, kind: BudgetKind) -> &mut u64 {
        match kind {
            BudgetKind::InputBytes => &mut self.input,
            BudgetKind::DecodeBytes => &mut self.decode,
            BudgetKind::AllocationBytes => &mut self.allocation,
            BudgetKind::MetadataBytes => &mut self.metadata,
            BudgetKind::VisitedItems => &mut self.visited,
            BudgetKind::OutputItems => &mut self.output,
            BudgetKind::OutputKeyBytes => &mut self.output_key,
        }
    }

    /// Checks both ceilings before charging either ledger; prior charges remain.
    ///
    /// # Errors
    /// Returns the first limit that would be exceeded, including arithmetic overflow.
    pub fn charge_both(owner: &mut Self, operation: &mut Self, kind: BudgetKind, amount: u64) -> Result<(), BudgetError> {
        for ledger in [&*owner, &*operation] {
            let limit = ledger.limit(kind);
            if ledger.used(kind).checked_add(amount).map_or(true, |value| value > limit) {
                return Err(BudgetError { kind, limit });
            }
        }
        *owner.used_mut(kind) += amount;
        *operation.used_mut(kind) += amount;
        Ok(())
    }
}
