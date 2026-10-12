// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0
use super::*;
use crate::{PrivacyClass, ProvenanceRecord, SchemaRef, Timestamp, TrustEnvelope};
use chacha20poly1305::{
    Key, XChaCha20Poly1305, XNonce,
    aead::{Aead, KeyInit, Payload},
};
use muniment::MemoryBackend;
struct Wallet([u8; 32]);
impl PayloadSealer for Wallet {
    fn seal(&self, hash: &Hash, bytes: &[u8]) -> Result<(Vec<u8>, crate::seal::SealedBlobRef)> {
        let cipher = XChaCha20Poly1305::new(&Key::try_from(&self.0[..]).unwrap());
        let encrypted = cipher
            .encrypt(
                &XNonce::try_from(&hash.as_bytes()[..24]).unwrap(),
                Payload {
                    msg: bytes,
                    aad: hash.as_bytes(),
                },
            )
            .map_err(|_| Error::new("wallet seal failed"))?;
        Ok((
            encrypted,
            crate::seal::SealedBlobRef {
                epoch: crate::seal::SealEpochId([1; 16]),
                format: crate::seal::SEAL_FORMAT_V1.into(),
            },
        ))
    }
    fn unseal(
        &self,
        hash: &Hash,
        marker: &crate::seal::SealedBlobRef,
        bytes: &[u8],
    ) -> Result<Vec<u8>> {
        if marker.epoch != crate::seal::SealEpochId([1; 16]) {
            return Err(Error::new("Unknown wallet epoch"));
        }
        XChaCha20Poly1305::new(&Key::try_from(&self.0[..]).unwrap())
            .decrypt(
                &XNonce::try_from(&hash.as_bytes()[..24]).unwrap(),
                Payload {
                    msg: bytes,
                    aad: hash.as_bytes(),
                },
            )
            .map_err(|_| Error::new("wallet authentication failed"))
    }
}
fn manifest(bytes: &[u8]) -> BlobManifest {
    BlobManifest {
        id: ManifestId::of_blob(bytes),
        schema: SchemaRef::from_id(ManifestId::of_blob(b"text/plain")),
        content_hash: Hash::of(bytes),
        byte_size: bytes.len() as u64,
        created_at: Timestamp(1),
        last_accessed: None,
        sources: vec![BlobSource::Embedded {
            bytes: bytes.to_vec(),
            media_type: "text/plain".into(),
        }],
        privacy: PrivacyClass::PublicPortable,
        provenance: ProvenanceRecord::self_imported("retention test"),
        trust: TrustEnvelope::self_asserted(),
        schema_metadata: serde_json::Value::Null,
        manifest_version: BlobManifest::CURRENT_VERSION,
    }
}
fn limits() -> Limits {
    Limits {
        max_payload_bytes: 20,
        max_stored_bytes: 20,
    }
}
#[tokio::test]
async fn private_copy_uses_actual_aead_and_refuses_missing_or_wrong_wallet_keys() {
    let mut store = MemoryBackend::default();
    let mut m = manifest(b"private notes");
    m.privacy = PrivacyClass::LocalOnly;
    let wallet = Wallet([3; 32]);
    let limits = Limits {
        max_payload_bytes: 20,
        max_stored_bytes: 64,
    };
    let copy = retain(
        &mut store,
        [1; 32],
        &m,
        b"private notes",
        limits,
        [2; 32],
        10,
        Some(&wallet),
    )
    .await
    .unwrap();
    assert!(copy.stored_bytes > copy.payload_bytes);
    assert_ne!(
        store
            .get(&slot([1; 32], m.id, "body"))
            .await
            .unwrap()
            .unwrap(),
        b"private notes"
    );
    assert_eq!(
        read(&mut store, [1; 32], m.id, Some(&wallet))
            .await
            .unwrap()
            .unwrap()
            .2,
        b"private notes"
    );
    assert!(read(&mut store, [1; 32], m.id, None).await.is_err());
    assert!(
        read(&mut store, [1; 32], m.id, Some(&Wallet([4; 32])))
            .await
            .is_err()
    );
    assert_eq!(
        retain(
            &mut store,
            [1; 32],
            &m,
            b"private notes",
            limits,
            [2; 32],
            11,
            Some(&wallet)
        )
        .await
        .unwrap(),
        copy
    );
    let manifest_key = slot([1; 32], m.id, "manifest");
    let raw = store.get(&manifest_key).await.unwrap().unwrap();
    let mut forged: BlobManifest = serde_json::from_slice(&raw).unwrap();
    forged.privacy = PrivacyClass::PublicPortable;
    store
        .put(&manifest_key, &serde_json::to_vec(&forged).unwrap())
        .await
        .unwrap();
    assert!(
        read(&mut store, [1; 32], m.id, Some(&wallet))
            .await
            .is_err()
    );
}
#[tokio::test]
async fn exact_copy_is_atomic_idempotent_and_owner_scoped() {
    let mut store = MemoryBackend::default();
    let m = manifest(b"notes");
    let held = retain(
        &mut store,
        [1; 32],
        &m,
        b"notes",
        limits(),
        [2; 32],
        10,
        None,
    )
    .await
    .unwrap();
    let before = store.list("").await.unwrap();
    assert_eq!(before.len(), 3);
    let duplicate = retain(
        &mut store,
        [1; 32],
        &m,
        b"notes",
        limits(),
        [3; 32],
        11,
        None,
    )
    .await
    .unwrap();
    assert_eq!(duplicate, held);
    assert_eq!(before, store.list("").await.unwrap());
    assert!(
        read(&mut store, [9; 32], m.id, None)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        read(&mut store, [1; 32], m.id, None)
            .await
            .unwrap()
            .unwrap()
            .2,
        b"notes"
    );
    store
        .put(&slot([1; 32], m.id, "body"), b"wrong")
        .await
        .unwrap();
    assert!(read(&mut store, [1; 32], m.id, None).await.is_err());
    assert!(
        retain(
            &mut store,
            [1; 32],
            &m,
            b"notes",
            limits(),
            [2; 32],
            12,
            None
        )
        .await
        .is_err()
    );
}
#[tokio::test]
async fn integrity_quota_and_keyless_private_refusals_write_nothing() {
    let mut store = MemoryBackend::default();
    let mut m = manifest(b"notes");
    assert!(
        retain(
            &mut store,
            [1; 32],
            &m,
            b"wrong",
            limits(),
            [2; 32],
            10,
            None
        )
        .await
        .is_err()
    );
    m.content_hash = Hash::of(b"wrong");
    assert!(
        retain(
            &mut store,
            [1; 32],
            &m,
            b"wrong",
            limits(),
            [2; 32],
            10,
            None
        )
        .await
        .is_err()
    );
    m = manifest(b"notes");
    assert!(
        retain(
            &mut store,
            [1; 32],
            &m,
            b"notes",
            Limits {
                max_stored_bytes: 4,
                ..limits()
            },
            [2; 32],
            10,
            None
        )
        .await
        .is_err()
    );
    m.privacy = PrivacyClass::LocalOnly;
    let err = retain(
        &mut store,
        [1; 32],
        &m,
        b"notes",
        limits(),
        [2; 32],
        10,
        None,
    )
    .await
    .unwrap_err();
    assert!(err.to_string().contains("wallet sealer"));
    assert!(store.list("").await.unwrap().is_empty());
}
#[tokio::test]
async fn competing_copies_share_one_atomic_owner_quota() {
    let mut first = MemoryBackend::default();
    let mut second = first.clone();
    let a = manifest(b"aaa");
    let b = manifest(b"bbb");
    let limits = Limits {
        max_payload_bytes: 3,
        max_stored_bytes: 3,
    };
    let (a, b) = tokio::join!(
        retain(&mut first, [1; 32], &a, b"aaa", limits, [2; 32], 10, None),
        retain(&mut second, [1; 32], &b, b"bbb", limits, [2; 32], 10, None)
    );
    assert_ne!(a.is_ok(), b.is_ok());
    assert_eq!(first.list("").await.unwrap().len(), 3);
}
#[cfg(not(target_arch = "wasm32"))]
#[tokio::test]
async fn verified_copy_reopens_on_a_real_disk_backend() {
    let dir = tempfile::tempdir().unwrap();
    let m = manifest(b"disk notes");
    {
        let mut store = muniment::RedbBackend::open(dir.path().join("copies.redb")).unwrap();
        retain(
            &mut store,
            [1; 32],
            &m,
            b"disk notes",
            limits(),
            [2; 32],
            10,
            None,
        )
        .await
        .unwrap();
    }
    let mut store = muniment::RedbBackend::open(dir.path().join("copies.redb")).unwrap();
    assert_eq!(
        read(&mut store, [1; 32], m.id, None)
            .await
            .unwrap()
            .unwrap()
            .2,
        b"disk notes"
    );
}
#[cfg(not(target_arch = "wasm32"))]
#[tokio::test]
async fn a_nontransactional_backend_refuses_custody_without_fallback_writes() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = muniment::FjallStore::open(dir.path()).unwrap();
    let m = manifest(b"notes");
    let error = retain(
        &mut store,
        [1; 32],
        &m,
        b"notes",
        limits(),
        [2; 32],
        10,
        None,
    )
    .await
    .unwrap_err();
    assert!(error.to_string().contains("does not support transactional"));
    assert!(store.list("").await.unwrap().is_empty());
}
