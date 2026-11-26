// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Schema-tagged payload bytes, with the schema named at runtime.
//!
//! The typed layer ([`crate::typed`]) names a payload's schema through its
//! type, [`TypedPayload::schema_ref`](crate::TypedPayload::schema_ref). A
//! caller that decides the schema at runtime, such as a host choosing which
//! schema its graph's snapshot binds to, saves, loads and lists through these
//! instead. The typed functions are these with `T::schema_ref()`, so both
//! store exactly the same manifests and blobs.

use crate::manifest::{
    BlobFetcher, BlobManifest, BlobSource, list_manifests, load_manifest, save_manifest,
};
use crate::schema::{
    Hash, ManifestId, PrivacyClass, ProvenanceRecord, SchemaRef, Timestamp, TrustEnvelope,
};
use crate::seal::{PayloadSealer, resolve_sealed_blob, seal_payload_for_store};
use crate::{Error, Result, Store};

/// Save `bytes` tagged with `schema`, sealing the private lane at rest as
/// [`save_typed_sealed`](crate::save_typed_sealed) does, and return the
/// manifest id: the BLAKE3 hash of `bytes`.
#[allow(clippy::too_many_arguments)] // mirrors save_typed_sealed + the schema.
pub async fn save_payload_sealed(
    store: &mut dyn Store,
    sealer: Option<&dyn PayloadSealer>,
    schema: SchemaRef,
    bytes: &[u8],
    additional_sources: Vec<BlobSource>,
    privacy: PrivacyClass,
    provenance: ProvenanceRecord,
    trust: TrustEnvelope,
    created_at: Timestamp,
) -> Result<ManifestId> {
    let hash = Hash::of(bytes);
    let id = ManifestId::from_hash(hash);
    let local_key = format!("blob:{}", hash.to_hex());

    let mut sources = Vec::with_capacity(1 + additional_sources.len());
    sources.push(BlobSource::Local {
        key: local_key.clone(),
    });
    sources.extend(additional_sources);

    let mut manifest = BlobManifest {
        id,
        schema,
        content_hash: hash,
        byte_size: bytes.len() as u64,
        created_at,
        last_accessed: None,
        sources,
        privacy,
        provenance,
        trust,
        schema_metadata: serde_json::Value::Null,
        manifest_version: BlobManifest::CURRENT_VERSION,
    };

    // Seals + stamps the manifest marker for the private lane; passthrough
    // otherwise. The stored bytes may be sealed; the manifest's content_hash
    // stays the cleartext hash for the post-unseal integrity check.
    let stored = seal_payload_for_store(sealer, &mut manifest, bytes)?;
    store.put(&local_key, &stored).await?;
    save_manifest(store, &manifest).await?;
    Ok(id)
}

/// Every manifest in the Store tagged with `schema`.
pub async fn list_payloads(store: &mut dyn Store, schema: SchemaRef) -> Result<Vec<BlobManifest>> {
    list_manifests(store, Some(schema)).await
}

/// The bytes stored under `id`, unsealed and hash-verified, refusing a
/// manifest tagged with any schema but `schema`. `Ok(None)` if no manifest is
/// stored under `id`.
pub async fn load_payload_sealed(
    store: &mut dyn Store,
    fetcher: &mut dyn BlobFetcher,
    sealer: Option<&dyn PayloadSealer>,
    schema: SchemaRef,
    id: ManifestId,
) -> Result<Option<Vec<u8>>> {
    let Some(manifest) = load_manifest(store, id).await? else {
        return Ok(None);
    };

    if manifest.schema != schema {
        return Err(Error::new(format!(
            "schema mismatch loading manifest {}: declared {}, requested {}",
            id, manifest.schema, schema
        )));
    }

    resolve_sealed_blob(store, fetcher, sealer, &manifest)
        .await
        .map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::NoFetcher;
    use crate::schema::{ModerationState, ProvenanceOrigin, TrustLevel};
    use crate::{TypedPayload, list_typed, load_typed, save_typed_sealed};
    use muniment::{Backend, MemoryBackend};
    use serde::{Deserialize, Serialize};

    #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
    struct Note {
        text: String,
    }

    impl TypedPayload for Note {
        fn schema_ref() -> SchemaRef {
            schema(b"schema:test-note/v1")
        }
    }

    fn schema(id: &[u8]) -> SchemaRef {
        SchemaRef::from_id(ManifestId::of_blob(id))
    }

    fn provenance() -> ProvenanceRecord {
        ProvenanceRecord {
            origin: ProvenanceOrigin::Generated,
            upstream: Vec::new(),
            tooling: Some("eidetic-test/0.0.1".into()),
            generated_at: Timestamp(1_700_000_000_000),
        }
    }

    fn trust() -> TrustEnvelope {
        TrustEnvelope {
            level: TrustLevel::SelfAsserted,
            signatures: Vec::new(),
            moderation_state: ModerationState::Unreviewed,
        }
    }

    async fn save(store: &mut MemoryBackend, schema: SchemaRef, bytes: &[u8]) -> ManifestId {
        save_payload_sealed(
            store,
            None,
            schema,
            bytes,
            Vec::new(),
            PrivacyClass::LocalOnly,
            provenance(),
            trust(),
            Timestamp(7),
        )
        .await
        .unwrap()
    }

    async fn contents(store: &MemoryBackend) -> Vec<(String, Vec<u8>)> {
        let mut out = Vec::new();
        for key in store.list("").await.unwrap() {
            out.push((key.clone(), store.get(&key).await.unwrap().unwrap()));
        }
        out.sort();
        out
    }

    #[test]
    fn the_runtime_form_stores_exactly_what_the_typed_form_stores() {
        pollster::block_on(async {
            let note = Note {
                text: "runtime".into(),
            };
            let mut typed = MemoryBackend::default();
            let typed_id = save_typed_sealed(
                &mut typed,
                None,
                &note,
                Vec::new(),
                PrivacyClass::LocalOnly,
                provenance(),
                trust(),
                Timestamp(7),
            )
            .await
            .unwrap();
            let bytes = note.serialize_to_bytes().unwrap();
            let mut runtime = MemoryBackend::default();
            let runtime_id = save(&mut runtime, Note::schema_ref(), &bytes).await;
            assert_eq!(runtime_id, typed_id);
            assert_eq!(contents(&runtime).await, contents(&typed).await);
            // Each form reads what the other wrote.
            let mut fetcher = NoFetcher;
            let read: Note = load_typed(&mut runtime, &mut fetcher, runtime_id)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(read, note);
            assert_eq!(
                load_payload_sealed(&mut typed, &mut fetcher, None, Note::schema_ref(), typed_id)
                    .await
                    .unwrap(),
                Some(bytes.clone())
            );
            // Control: another schema tags a different manifest.
            let mut other = MemoryBackend::default();
            save(&mut other, schema(b"schema:test-other/v1"), &bytes).await;
            assert_ne!(contents(&other).await, contents(&typed).await);
        });
    }

    #[test]
    fn a_runtime_load_refuses_another_schema_and_lists_only_its_own() {
        pollster::block_on(async {
            let mut store = MemoryBackend::default();
            let (a, b) = (schema(b"schema:test-a/v1"), schema(b"schema:test-b/v1"));
            let id = save(&mut store, a.clone(), b"payload a").await;
            let other = save(&mut store, b.clone(), b"payload b").await;
            let mut fetcher = NoFetcher;
            assert_eq!(
                load_payload_sealed(&mut store, &mut fetcher, None, a.clone(), id)
                    .await
                    .unwrap(),
                Some(b"payload a".to_vec())
            );
            let refused = load_payload_sealed(&mut store, &mut fetcher, None, b.clone(), id)
                .await
                .unwrap_err();
            assert!(refused.to_string().contains("schema mismatch"), "{refused}");
            let missing = ManifestId::of_blob(b"never saved");
            assert_eq!(
                load_payload_sealed(&mut store, &mut fetcher, None, a.clone(), missing)
                    .await
                    .unwrap(),
                None
            );
            let listed: Vec<ManifestId> = list_payloads(&mut store, a.clone())
                .await
                .unwrap()
                .into_iter()
                .map(|manifest| manifest.id)
                .collect();
            assert_eq!(listed, [id]);
            assert_eq!(list_payloads(&mut store, b).await.unwrap().len(), 1);
            assert!(list_typed::<Note>(&mut store).await.unwrap().is_empty());
            assert_ne!(id, other);
        });
    }
}
