// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::bounded_bloom::parse_bloom;
use crate::bounded_prepare::{PrepareError, RecoveryBudget, RecoveryResource};
use crate::coding::Encode;

struct Ledger { remaining: u64 }
impl RecoveryBudget for Ledger {
    fn charge(&mut self, resource: RecoveryResource, amount: u64) -> Result<(), PrepareError> {
        self.remaining = self.remaining.checked_sub(amount).ok_or(PrepareError::Limit { resource, limit: self.remaining })?;
        Ok(())
    }
    fn check_key_bytes(&mut self, amount: u64) -> Result<(), PrepareError> {
        self.charge(RecoveryResource::KeyBytes, amount)
    }
}

#[test]
fn accepts_existing_filter_bytes() {
    let filter = crate::bloom::BloomFilter::with_bpk(10, 10);
    let bytes = filter.encode_into_vec();
    assert_eq!(parse_bloom(&bytes, &mut Ledger { remaining: 4096 }).unwrap(), filter);
}

#[test]
fn rejects_declared_huge_filter_before_allocating() {
    let mut bytes = crate::file::MAGIC_BYTES.to_vec();
    bytes.extend_from_slice(&[0, 0]);
    bytes.extend_from_slice(&u64::MAX.to_be_bytes());
    bytes.extend_from_slice(&1_u64.to_be_bytes());
    let mut ledger = Ledger { remaining: 4096 };
    assert!(matches!(parse_bloom(&bytes, &mut ledger), Err(PrepareError::Corruption(_))));
    assert_eq!(ledger.remaining, 4096);
}
