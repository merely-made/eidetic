// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use p2panda_core::{Body, Header};

use super::*;
use crate::processor::OperationPolicy;

fn snapshot_body(seed: u8) -> EditionBody {
    EditionBody {
        kind: EditionKind::Site,
        content: ContentRef::Snapshot {
            digest: EditionId([seed; 32]),
        },
        provenance: Provenance {
            source: "test".into(),
            created_ms: u64::from(seed),
        },
        fork_of: None,
        hints: vec!["hint:example".into()],
        sealed: None,
    }
}

fn log(key: &SigningKey, count: u8) -> Vec<EditionOperation> {
    let mut writer = EditionWriter::new(key.clone());
    (1..=count)
        .map(|seed| writer.append(&snapshot_body(seed)).unwrap())
        .collect()
}

fn key_of(key: &SigningKey) -> [u8; 32] {
    *key.verifying_key().as_bytes()
}

#[test]
fn chain_verifies_from_the_key_alone_in_any_order() {
    let key = SigningKey::generate();
    let mut operations = log(&key, 3);
    operations.reverse();
    let chain = verify_edition_chain(&key_of(&key), &operations, EditionLimits::default()).unwrap();
    assert_eq!(chain.editions().len(), 3);
    assert_eq!(chain.head().seq_num, 2);
    assert_eq!(chain.head().body, snapshot_body(3));
}

#[test]
fn body_round_trips_every_kind_and_refuses_mismatched_content() {
    let limits = EditionLimits::default();
    let clip = EditionBody {
        kind: EditionKind::Clip,
        content: ContentRef::Manifest {
            root: EditionId([9; 32]),
        },
        provenance: Provenance {
            source: "turnstone".into(),
            created_ms: 1,
        },
        fork_of: Some(EditionId([4; 32])),
        hints: vec![],
        sealed: Some(SealedMarker {
            scheme: "drop-protector".into(),
        }),
    };
    let bytes = clip.encode(limits).unwrap();
    assert_eq!(EditionBody::decode(&bytes, limits).unwrap(), clip);
    let mut wrong = snapshot_body(1);
    wrong.content = ContentRef::Manifest {
        root: EditionId([1; 32]),
    };
    assert!(wrong.encode(limits).is_err());
}

#[test]
fn tampered_body_and_forged_operation_refuse() {
    let key = SigningKey::generate();
    let limits = EditionLimits::default();
    let mut operations = log(&key, 2);
    operations[1].body = Some(Body::from_bytes(b"{}"));
    assert!(matches!(
        verify_edition_chain(&key_of(&key), &operations, limits),
        Err(EditionError::Invalid(_))
    ));
    // Another key's valid edition is not a member of this log.
    let intruder = SigningKey::generate();
    let mut forged = log(&key, 1);
    forged.extend(log(&intruder, 2).into_iter().skip(1));
    assert_eq!(
        verify_edition_chain(&key_of(&key), &forged, limits).unwrap_err(),
        EditionError::WrongAuthor
    );
    // Altering a signed header breaks the signature.
    let mut header = log(&key, 1)[0].header.encode();
    let last = header.len() - 1;
    header[last] ^= 1;
    let decoded: Result<Header<EditionExt>, _> = Header::decode(&header[..]);
    if let Ok(header) = decoded {
        let op = EditionOperation {
            hash: header.hash(),
            header,
            body: log(&key, 1)[0].body.clone(),
        };
        assert!(verify_edition_chain(&key_of(&key), &[op], limits).is_err());
    }
}

#[test]
fn equivocation_and_gaps_refuse() {
    let key = SigningKey::generate();
    let limits = EditionLimits::default();
    let honest = log(&key, 3);
    let mut writer = EditionWriter::resume(
        key.clone(),
        &verify_edition_chain(&key_of(&key), &honest[..2], limits).unwrap(),
    )
    .unwrap();
    let rival = writer.append(&snapshot_body(99)).unwrap();
    let mut both = honest.clone();
    both.push(rival);
    assert_eq!(
        verify_edition_chain(&key_of(&key), &both, limits).unwrap_err(),
        EditionError::Equivocation { seq_num: 2 }
    );
    let gap = vec![honest[0].clone(), honest[2].clone()];
    assert!(matches!(
        verify_edition_chain(&key_of(&key), &gap, limits),
        Err(EditionError::Discontinuous { .. } | EditionError::Invalid(_))
    ));
    assert_eq!(
        verify_edition_chain(&key_of(&key), &[], limits).unwrap_err(),
        EditionError::Empty
    );
}

#[test]
fn fork_names_its_source_only_in_its_first_edition() {
    let limits = EditionLimits::default();
    let source_key = SigningKey::generate();
    let source = verify_edition_chain(&key_of(&source_key), &log(&source_key, 2), limits).unwrap();
    let fork_key = SigningKey::generate();
    let mut writer = EditionWriter::new(fork_key.clone());
    let mut first = snapshot_body(7);
    first.fork_of = Some(source.head().id);
    let a = writer.append(&first).unwrap();
    let fork = verify_edition_chain(&key_of(&fork_key), &[a], limits).unwrap();
    assert_eq!(fork.resolve_fork(&source).unwrap(), source.head());
    assert_eq!(
        source.resolve_fork(&source).unwrap_err(),
        EditionError::NotAFork
    );
    let other = verify_edition_chain(&key_of(&fork_key), fork.operations(), limits).unwrap();
    let elsewhere_key = SigningKey::generate();
    let elsewhere =
        verify_edition_chain(&key_of(&elsewhere_key), &log(&elsewhere_key, 1), limits).unwrap();
    assert_eq!(
        other.resolve_fork(&elsewhere).unwrap_err(),
        EditionError::ForkSourceMissing
    );
    let mut later = EditionWriter::resume(fork_key, &fork).unwrap();
    let mut second = snapshot_body(8);
    second.fork_of = Some(source.head().id);
    assert!(later.append(&second).is_err());
}

#[test]
fn evidence_round_trips_and_merge_keeps_the_newer_chain() {
    let key = SigningKey::generate();
    let limits = EditionLimits::default();
    let operations = log(&key, 4);
    let newer = encode_evidence(&operations);
    let older = encode_evidence(&operations[..2]);
    let merged = merge_evidence(&key_of(&key), Some(&newer), &older, limits).unwrap();
    assert_eq!(merged.head().seq_num, 3);
    assert!(decode_evidence(&newer[..newer.len() - 1], limits).is_err());
    let mut trailing = newer.clone();
    trailing.push(0);
    assert!(decode_evidence(&trailing, limits).is_err());
}

#[test]
fn policy_addresses_one_topic_and_log_per_key() {
    let key = SigningKey::generate();
    let policy = EditionPolicy::default();
    let operations = log(&key, 2);
    let admission = policy.admit(&operations[1]).unwrap();
    assert_eq!(admission.target.topic, edition_topic(&key_of(&key)));
    assert_eq!(admission.target.log_id, 0);
    let pinned = EditionPolicy {
        only_key: Some([1; 32]),
        ..EditionPolicy::default()
    };
    assert_eq!(
        pinned.admit(&operations[0]).unwrap_err().code,
        "edition-wrong-author"
    );
}

#[test]
fn writer_resumes_a_verified_chain() {
    let key = SigningKey::generate();
    let limits = EditionLimits::default();
    let chain = verify_edition_chain(&key_of(&key), &log(&key, 2), limits).unwrap();
    let mut writer = EditionWriter::resume(key.clone(), &chain).unwrap();
    let next = writer.append(&snapshot_body(5)).unwrap();
    assert_eq!(next.header.seq_num, 2);
    assert_eq!(
        next.header.backlink,
        Some(chain.head().id.0).map(p2panda_core::Hash::from_bytes)
    );
    assert_eq!(
        EditionWriter::resume(SigningKey::generate(), &chain).err(),
        Some(EditionError::WrongAuthor)
    );
}
