// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::bounded_manifest::parse_levels;
use crate::bounded_prepare::{PrepareError, RecoveryBudget, RecoveryResource};

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
fn reads_existing_level_manifest() {
    let mut bytes = crate::file::MAGIC_BYTES.to_vec();
    bytes.push(2);
    bytes.extend_from_slice(&1_u32.to_be_bytes());
    bytes.extend_from_slice(&7_u64.to_be_bytes());
    bytes.extend_from_slice(&0_u32.to_be_bytes());
    let levels = parse_levels(&bytes, &mut Ledger { remaining: 1024 }).unwrap();
    assert_eq!(levels, vec![vec![7], vec![]]);
}

#[test]
fn rejects_untrusted_count_without_allocating_it() {
    let mut bytes = crate::file::MAGIC_BYTES.to_vec();
    bytes.push(1);
    bytes.extend_from_slice(&u32::MAX.to_be_bytes());
    let mut ledger = Ledger { remaining: 1024 };
    assert!(matches!(parse_levels(&bytes, &mut ledger), Err(PrepareError::Corruption(_))));
    assert!(ledger.remaining > 900);
}
