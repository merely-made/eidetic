// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;
use std::io::Cursor;

struct TestProtector;

impl DropProtector for TestProtector {
    fn suite_id(&self) -> u16 {
        77
    }

    fn protect(&self, plaintext: &[u8], aad: &[u8]) -> Result<Vec<u8>, NativeDropError> {
        let mut out = blake3::hash(aad).as_bytes().to_vec();
        out.extend(plaintext.iter().map(|byte| byte ^ 0xa5));
        Ok(out)
    }

    fn unprotect(
        &self,
        protected: &[u8],
        aad: &[u8],
        max_plaintext_bytes: u64,
    ) -> Result<Vec<u8>, NativeDropError> {
        if protected.get(..32) != Some(blake3::hash(aad).as_bytes()) {
            return Err(NativeDropError::Protection(
                "test authentication failed".into(),
            ));
        }
        limit(
            "protected bytes",
            (protected.len() - 32) as u64,
            max_plaintext_bytes,
        )?;
        Ok(protected[32..].iter().map(|byte| byte ^ 0xa5).collect())
    }
}

fn records() -> Vec<DropRecord> {
    vec![
        DropRecord::Operation {
            header: b"header".to_vec(),
            inline_body: Some(b"body".to_vec()),
        },
        DropRecord::BlobChunk {
            blob_hash: [7; 32],
            offset: 4,
            bytes: b"chunk".to_vec(),
        },
    ]
}

fn encoded() -> (Vec<u8>, DropWriteReceipt) {
    let mut bytes = Vec::new();
    let receipt = write_plain_drop(&mut bytes, &records(), DropLimits::default()).unwrap();
    (bytes, receipt)
}

fn raw_future_record(critical: bool) -> Vec<u8> {
    let payload = b"future record".to_vec();
    let entry = ManifestEntry {
        kind: 99,
        critical,
        digest: *blake3::hash(&payload).as_bytes(),
        byte_len: payload.len() as u64,
    };
    let manifest = DropManifest {
        version: VERSION,
        records: vec![entry.clone()],
    };
    let manifest_bytes = encode_cbor(&manifest).unwrap();
    let protected_bytes = 8 + manifest_bytes.len() as u64 + FRAME_HEADER_LEN + entry.byte_len;
    let mut hasher = blake3::Hasher::new();
    hasher.update(&(manifest_bytes.len() as u64).to_le_bytes());
    hasher.update(&manifest_bytes);
    hasher.update(&frame_header(&entry));
    hasher.update(&payload);
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&MAGIC);
    bytes.extend_from_slice(&VERSION.to_le_bytes());
    bytes.extend_from_slice(&PLAIN_SUITE.to_le_bytes());
    bytes.extend_from_slice(&protected_bytes.to_le_bytes());
    bytes.extend_from_slice(hasher.finalize().as_bytes());
    bytes.extend_from_slice(&(manifest_bytes.len() as u64).to_le_bytes());
    bytes.extend_from_slice(&manifest_bytes);
    bytes.extend_from_slice(&frame_header(&entry));
    bytes.extend_from_slice(&payload);
    bytes
}

#[test]
fn plaintext_drop_round_trips_and_visits_in_order() {
    let (bytes, written) = encoded();
    let mut visited = Vec::new();
    let read = visit_plain_drop(Cursor::new(bytes), DropLimits::default(), |record| {
        visited.push(record);
        Ok(())
    })
    .unwrap();
    assert_eq!(visited, records());
    assert_eq!(read.id, written.id);
    assert_eq!(read.visited_records, 2);
}

#[test]
fn corruption_and_truncation_fail_closed() {
    let (mut corrupt, _) = encoded();
    *corrupt.last_mut().unwrap() ^= 1;
    assert!(matches!(
        read_plain_drop(Cursor::new(corrupt), DropLimits::default()),
        Err(NativeDropError::FalseRecord { .. })
            | Err(NativeDropError::FalseBodyDigest)
            | Err(NativeDropError::Codec(_))
    ));

    let (mut truncated, _) = encoded();
    truncated.pop();
    assert!(matches!(
        read_plain_drop(Cursor::new(truncated), DropLimits::default()),
        Err(NativeDropError::Truncated)
    ));
}

#[test]
fn limits_are_checked_before_record_allocation() {
    let (bytes, _) = encoded();
    let limits = DropLimits {
        max_record_bytes: 1,
        ..DropLimits::default()
    };
    assert!(matches!(
        read_plain_drop(Cursor::new(bytes), limits),
        Err(NativeDropError::Limit {
            limit: "record bytes",
            ..
        })
    ));
}

#[test]
fn unknown_optional_records_skip_and_unknown_critical_records_fail() {
    let optional = raw_future_record(false);
    let report = visit_plain_drop(Cursor::new(optional), DropLimits::default(), |_| {
        panic!("unknown records are not decoded")
    })
    .unwrap();
    assert_eq!(report.visited_records, 0);
    assert_eq!(report.skipped_optional_records, 1);

    let critical = raw_future_record(true);
    assert!(matches!(
        read_plain_drop(Cursor::new(critical), DropLimits::default()),
        Err(NativeDropError::UnknownCriticalRecord { kind: 99 })
    ));
}

#[test]
fn unsupported_protection_and_false_body_length_fail_closed() {
    let (mut unsupported, _) = encoded();
    unsupported[10..12].copy_from_slice(&7u16.to_le_bytes());
    assert!(matches!(
        read_plain_drop(Cursor::new(unsupported), DropLimits::default()),
        Err(NativeDropError::UnsupportedProtection(7))
    ));

    let (mut false_length, _) = encoded();
    let stated = u64_le(&false_length[12..20]);
    false_length[12..20].copy_from_slice(&(stated - 1).to_le_bytes());
    assert!(matches!(
        read_plain_drop(Cursor::new(false_length), DropLimits::default()),
        Err(NativeDropError::BodyLengthMismatch)
    ));
}

#[test]
fn operation_headers_can_arrive_before_their_payload_chunks() {
    let records = vec![
        DropRecord::Operation {
            header: b"signed header".to_vec(),
            inline_body: None,
        },
        DropRecord::PayloadChunk {
            payload_hash: [9; 32],
            offset: 0,
            bytes: b"later body".to_vec(),
        },
    ];
    let mut bytes = Vec::new();
    write_plain_drop(&mut bytes, &records, DropLimits::default()).unwrap();
    let (_, decoded) = read_plain_drop(Cursor::new(bytes), DropLimits::default()).unwrap();
    assert_eq!(decoded, records);
}

#[test]
fn protected_drop_requires_and_round_trips_through_its_suite() {
    let records = records();
    let mut bytes = Vec::new();
    let written =
        write_protected_drop(&mut bytes, &records, DropLimits::default(), &TestProtector).unwrap();
    assert_eq!(u16_le(&bytes[10..12]), 77);
    let (read, decoded) =
        read_protected_drop(Cursor::new(bytes), DropLimits::default(), &TestProtector).unwrap();
    assert_eq!(read.id, written.id);
    assert_eq!(decoded, records);
}

#[test]
// Re-frozen for p2panda 0.7.1: `encode_cbor` moved from ciborium to
// cbor-core, which emits canonical CBOR and therefore orders map keys
// (shortest first, then bytewise) instead of writing struct fields in
// declaration order. `ManifestEntry`'s bytes changed, so the manifest
// digest — and with it the `DropId` — changed. The drop format itself is
// unchanged; only its canonical byte encoding moved.
fn golden_vector_freezes_manifest_identity_and_cover() {
    let (bytes, receipt) = encoded();
    assert_eq!(&bytes[0..8], b"MEREDRP\0");
    assert_eq!(receipt.total_bytes as usize, bytes.len());
    assert_eq!(
        hex::encode(receipt.id.0),
        "f92cf6cf4dc067f05f14454b9b1cbb618935a02002da9f15b2d37fd6f5452a55"
    );
    assert_eq!(
        hex::encode(&bytes[0..52]),
        "4d45524544525000010000007d010000000000005d67dd629e91bbcd8e04fd16ba72de402c27ca75bc74fb6f257f5b1f90f47d1b"
    );
}
