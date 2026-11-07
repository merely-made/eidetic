// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! F119's equality control and F129's `hash_value`.

use std::collections::{BTreeMap, BTreeSet};

use state_witness::{hash_bytes, hash_value};

/// Verbatim from isometry `shared/isometer/crates/isometer-core/src/snapshot.rs`,
/// copied rather than depended on (F119).
fn isometer_hash_bytes(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01B3);
    }
    hash
}

/// The control: one constant off, so it must disagree.
fn off_by_one_prime(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01B5);
    }
    hash
}

fn inputs() -> Vec<Vec<u8>> {
    let mut inputs = vec![Vec::new(), b"a".to_vec(), b"foobar".to_vec()];
    inputs.extend((0..=255u8).map(|b| vec![b]));
    inputs.push(
        (0..1024u32)
            .map(|i| (i.wrapping_mul(31) + 7) as u8)
            .collect(),
    );
    inputs
}

#[test]
fn hash_bytes_equals_isometers_and_the_control_does_not() {
    let inputs = inputs();
    let agree = inputs
        .iter()
        .filter(|i| hash_bytes(i) == isometer_hash_bytes(i))
        .count();
    let control = inputs
        .iter()
        .filter(|i| off_by_one_prime(i) == isometer_hash_bytes(i))
        .count();
    println!(
        "equality control: {agree}/{} inputs agree with isometer; off-by-one prime agrees on {control} (the empty input, which never multiplies)",
        inputs.len()
    );
    println!(
        "figures: \"\" {:016x}, \"a\" {:016x}, \"foobar\" {:016x}, 1 KiB {:016x}",
        hash_bytes(b""),
        hash_bytes(b"a"),
        hash_bytes(b"foobar"),
        hash_bytes(inputs.last().unwrap())
    );
    assert_eq!(agree, inputs.len());
    assert_eq!(control, 1, "only the empty input escapes the prime");
}

#[test]
fn the_published_fnv1a_64_vectors_hold() {
    assert_eq!(hash_bytes(b""), 0xcbf2_9ce4_8422_2325);
    assert_eq!(hash_bytes(b"a"), 0xaf63_dc4c_8601_ec8c);
    assert_eq!(hash_bytes(b"foobar"), 0x8594_4171_f739_67e8);
}

fn same<T: serde::Serialize>(value: T) {
    let bytes = postcard::to_allocvec(&value).unwrap();
    assert_eq!(hash_value(&value).unwrap(), hash_bytes(&bytes));
}

#[test]
fn hash_value_streams_the_postcard_bytes() {
    same(0u64);
    same(u64::MAX);
    same(-3i32);
    same("site:7");
    same(String::new());
    same(Some(5u8));
    same(None::<u8>);
    same(vec![1u32, 2, 3]);
    same(BTreeMap::from([(9u64, "nine"), (10, "ten")]));
    same(BTreeSet::from([(1u64, 2u8, 3u64)]));
    same((1.5f32, -2.25f64));
    same(vec![0u8; 300]);
}

#[test]
fn floats_hash_by_bit_pattern() {
    assert_ne!(hash_value(&0.0f32).unwrap(), hash_value(&-0.0f32).unwrap());
    let (a, b) = (
        f64::from_bits(0x7ff8_0000_0000_0001),
        f64::from_bits(0x7ff8_0000_0000_0002),
    );
    assert_ne!(hash_value(&a).unwrap(), hash_value(&b).unwrap());
    // seiche-repeat's fingerprint: `to_bits` little-endian, as postcard writes it.
    let raw: Vec<u8> = [1.5f32, -2.25]
        .iter()
        .flat_map(|v| v.to_bits().to_le_bytes())
        .collect();
    assert_eq!(hash_value(&(1.5f32, -2.25f32)).unwrap(), hash_bytes(&raw));
}

#[test]
fn a_tuple_hashes_as_its_fields_concatenated() {
    let mut bytes = postcard::to_allocvec(&11u64).unwrap();
    bytes.extend(postcard::to_allocvec("rev").unwrap());
    bytes.extend(postcard::to_allocvec(&BTreeMap::from([(1u64, 2u64)])).unwrap());
    let value = (11u64, "rev", BTreeMap::from([(1u64, 2u64)]));
    assert_eq!(hash_value(&value).unwrap(), hash_bytes(&bytes));
}

#[test]
fn an_unencodable_value_is_refused() {
    struct Unsized;
    impl serde::Serialize for Unsized {
        fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            use serde::ser::SerializeSeq;
            s.serialize_seq(None)?.end()
        }
    }
    assert_eq!(hash_value(&Unsized), Err(state_witness::Error::Encode));
}
