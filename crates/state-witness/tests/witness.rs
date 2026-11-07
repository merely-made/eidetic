// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The keyed witness, its fold and its divergence report (F120 to F124,
//! F127, F128).

use state_witness::{
    Divergence, Error, Witness, divergences, first_divergence, hash_bytes, hash_value, label,
};

fn labels(w: &Witness) -> Vec<&str> {
    w.entries().iter().map(|e| e.label.as_str()).collect()
}

fn sites(ids: &[u64], bump: Option<u64>) -> Witness {
    let mut w = Witness::new();
    for &id in ids {
        let value = if bump == Some(id) {
            id * 100 + 1
        } else {
            id * 100
        };
        w.insert(label("site", [id]), &value).unwrap();
    }
    w
}

#[test]
fn entries_sit_in_natural_order() {
    let mut w = Witness::new();
    for l in [
        "site:10", "site:9", "clock", "site:09", "a2b10", "a2b9", "site:", "Z", "é",
    ] {
        w.insert_bytes(l, l.as_bytes()).unwrap();
    }
    assert_eq!(
        labels(&w),
        [
            "Z", "a2b9", "a2b10", "clock", "site:", "site:09", "site:9", "site:10", "é"
        ]
    );
}

#[test]
fn insertion_order_never_changes_a_result() {
    let ids: Vec<u64> = (0..40).collect();
    let forward = sites(&ids, None);
    let backward = sites(&ids.iter().rev().copied().collect::<Vec<_>>(), None);
    assert_eq!(forward, backward);
    assert_eq!(forward.digest(), backward.digest());
}

#[test]
fn insert_refuses_duplicates_nuls_and_unencodable_values() {
    let mut w = Witness::new();
    w.insert("site:9", &1u8).unwrap();
    assert_eq!(
        w.insert("site:9", &2u8),
        Err(Error::DuplicateLabel("site:9".into()))
    );
    assert_eq!(
        w.insert_bytes("site:9", b""),
        Err(Error::DuplicateLabel("site:9".into()))
    );
    assert_eq!(
        w.insert("a\0b", &1u8),
        Err(Error::NulInLabel("a\0b".into()))
    );
    // A sequence of unknown length, which postcard cannot encode.
    struct Unsized;
    impl serde::Serialize for Unsized {
        fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            use serde::ser::SerializeSeq;
            s.serialize_seq(None)?.end()
        }
    }
    assert_eq!(w.insert("seq", &Unsized), Err(Error::Encode));
    assert_eq!(w.len(), 1, "refusals leave the witness unchanged");
    assert_eq!(w.get("site:9"), Some(hash_value(&1u8).unwrap()));
}

#[test]
fn the_digest_is_balaurs_fold() {
    let mut w = Witness::new();
    w.insert("site:2", &7u64).unwrap();
    w.insert("clock", &(3u64, 4u64)).unwrap();
    let mut bytes = Vec::new();
    for (l, d) in [
        ("clock", hash_value(&(3u64, 4u64)).unwrap()),
        ("site:2", hash_value(&7u64).unwrap()),
    ] {
        bytes.extend(l.as_bytes());
        bytes.push(0);
        bytes.extend(d.to_le_bytes());
    }
    assert_eq!(w.digest(), hash_bytes(&bytes));
    assert_eq!(Witness::new().digest(), hash_bytes(b""));
    // The fold covers labels: same values under another label differ.
    let mut renamed = Witness::new();
    renamed.insert("site:3", &7u64).unwrap();
    renamed.insert("clock", &(3u64, 4u64)).unwrap();
    assert_ne!(w.digest(), renamed.digest());
}

#[test]
fn equal_witnesses_do_not_diverge() {
    let ids: Vec<u64> = (0..200).collect();
    assert_eq!(
        first_divergence(&sites(&ids, None), &sites(&ids, None)),
        None
    );
    assert_eq!(divergences(&Witness::new(), &Witness::new()).count(), 0);
}

#[test]
fn a_planted_change_is_named_by_label() {
    let ids: Vec<u64> = (0..200).collect();
    let (a, b) = (sites(&ids, None), sites(&ids, Some(137)));
    assert_eq!(
        first_divergence(&a, &b),
        Some(Divergence {
            label: "site:137".into(),
            left: Some(hash_value(&13_700u64).unwrap()),
            right: Some(hash_value(&13_701u64).unwrap()),
        })
    );
    assert_ne!(a.digest(), b.digest());
    assert_eq!(divergences(&a, &b).count(), 1);
}

#[test]
fn a_label_on_one_side_diverges_at_its_place_in_either_direction() {
    let a = sites(&[1, 2, 9, 10], None);
    let b = sites(&[1, 2, 5, 9, 10], None);
    let found = first_divergence(&a, &b).unwrap();
    assert_eq!(
        (found.label.as_str(), found.left, found.right.is_some()),
        ("site:5", None, true)
    );
    let back = first_divergence(&b, &a).unwrap();
    assert_eq!(
        (back.label.as_str(), back.left.is_some(), back.right),
        ("site:5", true, None)
    );
    // Earliest in natural order wins: an extra site:3 left beats site:5 right.
    let c = sites(&[1, 2, 3, 9, 10], None);
    assert_eq!(first_divergence(&c, &b).unwrap().label, "site:3");
    let all: Vec<String> = divergences(&c, &b).map(|d| d.label).collect();
    assert_eq!(all, ["site:3", "site:5"]);
    // A trailing label on one side is still reported.
    let short = sites(&[1, 2], None);
    assert_eq!(first_divergence(&short, &a).unwrap().label, "site:9");
}

#[test]
fn the_report_prints_both_sides() {
    let d = Divergence {
        label: "sites".into(),
        left: Some(0xab),
        right: None,
    };
    assert_eq!(d.to_string(), "sites: 00000000000000ab vs absent");
}

#[test]
fn the_label_helper_fixes_the_grammar() {
    assert_eq!(label("site", [7]), "site:7");
    assert_eq!(label("arrival", ["ev-3", "12"]), "arrival:ev-3/12");
    assert_eq!(label("clock", std::iter::empty::<u8>()), "clock");
}

#[test]
fn raw_bytes_and_serde_values_do_not_agree() {
    let floats = [1.5f32, -2.25];
    let raw: Vec<u8> = floats
        .iter()
        .flat_map(|v| v.to_bits().to_le_bytes())
        .collect();
    let mut w = Witness::new();
    w.insert_bytes("raw", &raw).unwrap();
    w.insert("serde", &floats.to_vec()).unwrap();
    assert_eq!(w.get("raw"), Some(hash_bytes(&raw)));
    assert_ne!(
        w.get("raw"),
        w.get("serde"),
        "a postcard Vec carries a length prefix"
    );
}
