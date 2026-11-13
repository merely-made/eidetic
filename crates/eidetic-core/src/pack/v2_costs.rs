// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Declared-lens L1's cost receipt: encode, decode and verify for a
//! 1,000-term header. Ignored by default; run in release:
//!
//! ```text
//! cargo test --release -p eidetic --lib v2_costs -- --ignored --nocapture
//! ```

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use identity::Ed25519Keypair;

use super::v2_codec_tests::fixture;
use super::*;
use crate::schema::{HashFn, ModerationState, TrustEnvelope, TrustLevel};

const AXES: [&str; 8] = [
    "stratum",
    "durability",
    "retractability",
    "projectability",
    "force",
    "symmetry",
    "presentation",
    "derivation",
];

fn term(i: usize) -> BodyValue {
    let ns = "https://mere.computer/ns/";
    let axes = AXES
        .iter()
        .map(|a| {
            (
                format!("{ns}axis/{a}"),
                BodyValue::Text(format!("{ns}value/{a}/{}", i % 5)),
            )
        })
        .collect();
    BodyValue::Map(BTreeMap::from([
        (
            "iri".to_string(),
            BodyValue::Text(format!("{ns}term/t{i:04}")),
        ),
        ("kind".to_string(), BodyValue::Text("predicate".to_string())),
        (
            "family".to_string(),
            BodyValue::Text(format!("{ns}family/f{}", i % 6)),
        ),
        ("axes".to_string(), BodyValue::Map(axes)),
        (
            "labels".to_string(),
            BodyValue::Map(BTreeMap::from([(
                "en".to_string(),
                BodyValue::Text(format!("term {i}")),
            )])),
        ),
    ]))
}

fn median(mut runs: Vec<Duration>) -> Duration {
    runs.sort();
    runs[runs.len() / 2]
}

fn time<T>(n: usize, mut f: impl FnMut() -> T) -> Duration {
    median(
        (0..n)
            .map(|_| {
                let start = Instant::now();
                std::hint::black_box(f());
                start.elapsed()
            })
            .collect(),
    )
}

#[test]
#[ignore = "cost receipt; run in release"]
fn thousand_term_costs() {
    let mut header = fixture();
    header.kind = vec![PackKind::Vocabulary];
    header.body = BTreeMap::from([(
        PackKind::Vocabulary,
        BodyValue::Map(BTreeMap::from([(
            "terms".to_string(),
            BodyValue::Array((0..1000).map(term).collect()),
        )])),
    )]);
    let bytes = header.to_canonical().unwrap();
    let keypair = Ed25519Keypair::from_seed([3; 32]);
    let env = TrustEnvelope {
        level: TrustLevel::SelfAsserted,
        signatures: vec![sign_canonical(&bytes, &keypair)],
        moderation_state: ModerationState::Unreviewed,
    };
    assert!(matches!(verify_envelope(&bytes, &env), Verdict::Trusted(_)));
    let n = 31;
    let encode = time(n, || header.to_canonical().unwrap());
    let decode = time(n, || PackHeader::from_canonical(&bytes).unwrap());
    let blake3 = time(n, || PackHeader::pin_of(&bytes, HashFn::Blake3).unwrap());
    let sha256 = time(n, || crate::schema::Hash::digest(HashFn::Sha256, &bytes));
    let sign = time(n, || sign_canonical(&bytes, &keypair));
    let verify = time(n, || verify_envelope(&bytes, &env));
    eprintln!(
        "1000 terms, {} canonical bytes, median of {n}: encode {encode:?}, strict decode {decode:?}, \
         decode+BLAKE3 pin {blake3:?}, SHA-256 {sha256:?}, sign {sign:?}, verify {verify:?}",
        bytes.len()
    );
}
