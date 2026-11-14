// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Known-answer vectors for the journal digests: fixed journals and the hex
//! digests their prefixes must have, so a change to an encoding shows here.

use std::collections::BTreeMap;

use serde::Serialize;

use super::{DigestChain, Journal};

#[derive(Serialize)]
struct Edit {
    author: &'static str,
    at_ms: u64,
    fields: BTreeMap<&'static str, &'static str>,
}

fn edit(author: &'static str, at_ms: u64, fields: &[(&'static str, &'static str)]) -> Edit {
    Edit {
        author,
        at_ms,
        fields: fields.iter().copied().collect(),
    }
}

/// The vector journal: three edits, one with no fields.
fn journal() -> Journal<Edit> {
    let mut log = Journal::new();
    log.append(edit("a", 1, &[("k", "v")]));
    log.append(edit("b", 2, &[("y", "2"), ("x", "1")]));
    log.append(edit("c", 3, &[]));
    log
}

/// v1 (`prefix_digest`) at lengths 0 to 3, pinned before v2 was added.
const V1: [&str; 4] = [
    "448053ccd3e8387f45e09ec33f272e8c291eeb21de0ce7356d366753c600405b",
    "b9ec94014e7e30d2f767d48ff22198da6c34fe1ee29240e6d535d3854aa9351d",
    "be28a8f8a0cf0191c1d91d8abfa18e2f0a0a99a9c0113fdd41c71b7fb64d8f72",
    "0897c78cb3d421234539b124d7860d9f5c93aa5a1612b5a9c611ef9cb88615b7",
];

#[test]
fn v1_prefix_digests_match_their_vectors() {
    let log = journal();
    for (len, want) in V1.iter().enumerate() {
        let got = log.prefix_digest(len).unwrap().unwrap().to_hex();
        assert_eq!(got, *want, "v1 at {len}");
    }
}

/// v2 (`chain_digest`, `DigestChain`) at lengths 0 to 3.
const V2: [&str; 4] = [
    "d07475b941dddc278b6ac72a34c7156ca231d4c923ece239da1c8b0926c22f84",
    "87d2bfa71cac6befbd4479ac3926711b49c8425a4961bf4f1abadd7e801c251f",
    "c419b7a3c4ad38053e4081fbe71ea48e0ba1a26642bee2e04a7036ed1df6a46b",
    "42acb2e3c40e50c9e3d1a9d2a2f385cce05433f215c777492704aa8ca4dd45d9",
];

#[test]
fn v2_chain_digests_match_their_vectors() {
    let log = journal();
    let chain = DigestChain::of(&log).unwrap();
    for (len, want) in V2.iter().enumerate() {
        let got = log.chain_digest(len).unwrap().unwrap().to_hex();
        assert_eq!(got, *want, "v2 at {len}");
        assert_eq!(chain.at(len).unwrap().to_hex(), *want, "v2 chain at {len}");
    }
}
