// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::bounded_index::parse_index_entries;
use crate::bounded_prepare::{PrepareError, RecoveryBudget, RecoveryResource};
use crate::coding::Encode;
use crate::segment::{block::offset::BlockOffset, block_index::block_handle::KeyedBlockHandle};

struct Ledger { remaining: u64, max_key: u64 }
impl RecoveryBudget for Ledger {
    fn charge(&mut self, resource: RecoveryResource, amount: u64) -> Result<(), PrepareError> {
        self.remaining = self.remaining.checked_sub(amount).ok_or(PrepareError::Limit { resource, limit: self.remaining })?;
        Ok(())
    }
    fn check_key_bytes(&mut self, amount: u64) -> Result<(), PrepareError> {
        if amount > self.max_key {
            return Err(PrepareError::Limit { resource: RecoveryResource::KeyBytes, limit: self.max_key });
        }
        Ok(())
    }
}

fn encoded() -> Vec<u8> {
    let mut bytes = 2_u32.to_be_bytes().to_vec();
    KeyedBlockHandle::new("abc", BlockOffset(7)).encode_into(&mut bytes).unwrap();
    KeyedBlockHandle::new("defg", BlockOffset(42)).encode_into(&mut bytes).unwrap();
    bytes
}

#[test]
fn decodes_existing_index_entries_at_key_boundary() {
    let items = parse_index_entries(&encoded(), &mut Ledger { remaining: 4096, max_key: 4 }).unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].end_key.as_ref(), b"abc");
    assert_eq!(*items[1].offset, 42);
}

#[test]
fn rejects_untrusted_key_before_entry_allocation() {
    let mut ledger = Ledger { remaining: 4096, max_key: 3 };
    assert!(matches!(parse_index_entries(&encoded(), &mut ledger), Err(PrepareError::Limit { resource: RecoveryResource::KeyBytes, .. })));
    assert_eq!(ledger.remaining, 4094);
    let mut truncated = encoded();
    truncated.pop();
    assert!(matches!(parse_index_entries(&truncated, &mut Ledger { remaining: 4096, max_key: 4 }), Err(PrepareError::Corruption(_))));
}

#[test]
fn long_key_heap_header_is_in_allocation_admission() {
    let mut bytes = 1_u32.to_be_bytes().to_vec();
    KeyedBlockHandle::new(vec![b'a'; 24], BlockOffset(7)).encode_into(&mut bytes).unwrap();
    struct AllocationCap(u64);
    impl RecoveryBudget for AllocationCap {
        fn charge(&mut self, resource: RecoveryResource, amount: u64) -> Result<(), PrepareError> {
            if resource == RecoveryResource::AllocationBytes {
                self.0 = self.0.checked_sub(amount).ok_or(PrepareError::Limit { resource, limit: self.0 })?;
            }
            Ok(())
        }
        fn check_key_bytes(&mut self, _: u64) -> Result<(), PrepareError> { Ok(()) }
    }
    let cap = (std::mem::size_of::<KeyedBlockHandle>() + 24) as u64;
    assert!(matches!(parse_index_entries(&bytes, &mut AllocationCap(cap)), Err(PrepareError::Limit { resource: RecoveryResource::AllocationBytes, .. })));
}

#[test]
fn malformed_index_order_is_rejected_before_entry_allocation() {
    let mut bytes = 2_u32.to_be_bytes().to_vec();
    KeyedBlockHandle::new("z", BlockOffset(7)).encode_into(&mut bytes).unwrap();
    KeyedBlockHandle::new("a", BlockOffset(42)).encode_into(&mut bytes).unwrap();
    let mut ledger = Ledger { remaining: 4096, max_key: 8 };
    assert!(matches!(parse_index_entries(&bytes, &mut ledger), Err(PrepareError::Corruption(_))));
    assert_eq!(ledger.remaining, 4094);
}

#[test]
fn visits_are_charged_before_failed_index_validation() {
    struct Visits { remaining: u64, used: u64 }
    impl RecoveryBudget for Visits {
        fn charge(&mut self, resource: RecoveryResource, amount: u64) -> Result<(), PrepareError> {
            if resource == RecoveryResource::VisitedItems {
                if amount > self.remaining { return Err(PrepareError::Limit { resource, limit: self.remaining }); }
                self.remaining -= amount;
                self.used += amount;
            }
            Ok(())
        }
        fn check_key_bytes(&mut self, _: u64) -> Result<(), PrepareError> { Ok(()) }
    }
    let mut bytes = 2_u32.to_be_bytes().to_vec();
    KeyedBlockHandle::new("z", BlockOffset(7)).encode_into(&mut bytes).unwrap();
    KeyedBlockHandle::new("a", BlockOffset(42)).encode_into(&mut bytes).unwrap();
    let mut zero = Visits { remaining: 0, used: 0 };
    assert!(matches!(parse_index_entries(&bytes, &mut zero), Err(PrepareError::Limit { resource: RecoveryResource::VisitedItems, .. })));
    let mut repeated = Visits { remaining: 2, used: 0 };
    assert!(matches!(parse_index_entries(&bytes, &mut repeated), Err(PrepareError::Corruption(_))));
    assert_eq!(repeated.used, 2);
    assert!(matches!(parse_index_entries(&bytes, &mut repeated), Err(PrepareError::Limit { resource: RecoveryResource::VisitedItems, .. })));
    assert_eq!(repeated.used, 2);
}
