// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::bounded_validation::{validate_data_entries, ValidationError};
use crate::{coding::Encode, InternalValue, ValueType};

fn encoded_block() -> Vec<u8> {
    let mut bytes = 2_u32.to_be_bytes().to_vec();
    InternalValue::from_components("a", "one", 2, ValueType::Value)
        .encode_into(&mut bytes)
        .unwrap();
    InternalValue::from_components("b", "longer", 1, ValueType::Value)
        .encode_into(&mut bytes)
        .unwrap();
    bytes
}

#[test]
fn checks_all_entries_before_any_value_copy() {
    let bytes = encoded_block();
    assert!(matches!(validate_data_entries(&bytes, 8, 5, 2), Err(ValidationError::ValueLimit)));
    assert!(validate_data_entries(&bytes, 8, 6, 2).is_ok());
}

#[test]
fn rejects_truncated_values_and_excessive_item_count() {
    let mut bytes = encoded_block();
    bytes.pop();
    assert!(matches!(validate_data_entries(&bytes, 8, 6, 2), Err(ValidationError::Corrupt)));
    assert!(matches!(validate_data_entries(&[255, 255, 255, 255], 8, 8, 2), Err(ValidationError::ItemLimit)));
}
