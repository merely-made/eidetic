// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use std::collections::HashMap;

use serde::Serialize;
use serde::ser::{Error as _, Serializer};

use super::*;
use crate::LogId;

#[derive(Clone, Serialize)]
struct Edit {
    author: String,
    at_ms: u64,
    fields: HashMap<String, String>,
}

/// A journal of `len` edits of varied authors and sizes.
fn journal(len: usize) -> Journal<Edit> {
    let mut log = Journal::new();
    for n in 0..len {
        log.append(Edit {
            author: format!("author-{}", n % 7),
            at_ms: n as u64 * 13,
            fields: (0..n % 11)
                .map(|f| (format!("key-{f}"), format!("value-{n}-{f}")))
                .collect(),
        });
    }
    log
}

fn hash(input: &[u8]) -> Hash {
    let mut hasher = blake3::Hasher::new_derive_key("muniment journal prefix digest v2");
    hasher.update(input);
    Hash::from_bytes(*hasher.finalize().as_bytes())
}

#[test]
fn the_definition_written_out_by_hand_matches() {
    // A u64 entry's canonical encoding is the UINT tag (2), then the value
    // widened to sixteen little-endian bytes.
    let encode = |v: u64| {
        let mut bytes = vec![2u8];
        bytes.extend(u128::from(v).to_le_bytes());
        bytes
    };
    let mut log = Journal::new();
    let mut want = hash(&[]);
    assert_eq!(log.chain_digest(0).unwrap(), Some(want));
    for v in [0u64, 7, u64::MAX] {
        log.append(v);
        let c = encode(v);
        let mut input = want.as_bytes().to_vec();
        input.extend((c.len() as u64).to_le_bytes());
        input.extend(&c);
        want = hash(&input);
        assert_eq!(log.chain_digest(log.len()).unwrap(), Some(want));
    }
    assert_eq!(DigestChain::of(&log).unwrap().newest(), want);
}

#[test]
fn every_cursor_in_the_chain_equals_its_digest_from_scratch() {
    let log = journal(300);
    let chain = DigestChain::of(&log).unwrap();
    assert_eq!(chain.len(), 300);
    for k in 0..=log.len() {
        assert_eq!(chain.at(k), log.chain_digest(k).unwrap(), "cursor {k}");
    }
    assert_eq!(chain.at(301), None);
    assert_eq!(log.chain_digest(301).unwrap(), None);
    assert_eq!(chain.newest(), chain.at(300).unwrap());
}

#[test]
fn appending_never_changes_an_earlier_cursor() {
    let full = journal(120);
    let mut log = Journal::new();
    let mut chain = DigestChain::new();
    let mut seen = vec![chain.newest()];
    for entry in full.entries() {
        log.append(entry.clone());
        let newest = chain.push(entry).unwrap();
        assert!(!seen.contains(&newest), "each prefix has its own digest");
        seen.push(newest);
        for (k, digest) in seen.iter().enumerate() {
            assert_eq!(chain.at(k), Some(*digest), "cursor {k} after {}", log.len());
        }
    }
    assert_eq!(chain, DigestChain::of(&full).unwrap());
}

#[test]
fn catching_up_equals_building_whole() {
    let full = journal(90);
    let mut log = Journal::new();
    let mut chain = DigestChain::new();
    for (n, entry) in full.entries().iter().enumerate() {
        log.append(entry.clone());
        if n % 17 == 0 {
            chain.catch_up(&log).unwrap();
        }
    }
    assert_eq!(
        chain.catch_up(&log).unwrap(),
        full.chain_digest(90).unwrap().unwrap()
    );
    assert_eq!(chain, DigestChain::of(&full).unwrap());
    // Caught up, a second call adds nothing.
    assert_eq!(chain.catch_up(&log).unwrap(), chain.newest());
    assert_eq!(chain.len(), 90);
}

#[test]
fn a_chain_longer_than_its_journal_is_refused() {
    let chain = DigestChain::of(&journal(5)).unwrap();
    let mut wrong = chain.clone();
    assert!(matches!(
        wrong.catch_up(&journal(4)),
        Err(StoreError::Corruption(_))
    ));
    assert_eq!(wrong, chain);
}

#[test]
fn v2_is_not_v1_at_any_cursor() {
    let log = journal(40);
    let chain = DigestChain::of(&log).unwrap();
    for k in 0..=log.len() {
        assert_ne!(chain.at(k), log.prefix_digest(k).unwrap(), "cursor {k}");
    }
}

#[test]
fn the_chain_binds_entries_only() {
    let anonymous = journal(6);
    let mut named = Journal::with_id(LogId::new("named"));
    for entry in anonymous.entries() {
        named.append(entry.clone());
    }
    let fork = anonymous.fork(LogId::new("fork"));
    let want = DigestChain::of(&anonymous).unwrap();
    assert_eq!(
        DigestChain::of(&named).unwrap(),
        want,
        "identity is not an entry"
    );
    assert_eq!(
        DigestChain::of(&fork).unwrap(),
        want,
        "a fork shares its prefix"
    );
}

#[test]
fn an_entry_that_fails_to_encode_leaves_the_chain_unchanged() {
    struct Refuses;
    impl Serialize for Refuses {
        fn serialize<S: Serializer>(&self, _: S) -> Result<S::Ok, S::Error> {
            Err(S::Error::custom("refused"))
        }
    }
    let mut chain = DigestChain::new();
    chain.push(&1u8).unwrap();
    let before = chain.clone();
    assert!(matches!(chain.push(&Refuses), Err(StoreError::Codec(_))));
    assert_eq!(chain, before);
    assert!(!chain.is_empty());
}
