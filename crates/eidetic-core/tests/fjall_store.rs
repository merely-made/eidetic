// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

#![cfg(not(target_arch = "wasm32"))]

use eidetic::manifest::{load_manifest, save_manifest};
use eidetic::{
    BlobManifest, BlobSource, Hash, ManifestId, ModerationState, PrivacyClass, ProvenanceOrigin,
    ProvenanceRecord, SchemaRef, Timestamp, TrustEnvelope, TrustLevel,
};
use muniment::FjallStore;
use tempfile::TempDir;

#[test]
fn manifest_round_trips_through_fjall_store() {
    pollster::block_on(async {
        let dir = TempDir::new().expect("tempdir");
        let blob = b"test-payload";
        let manifest = BlobManifest {
            id: ManifestId::of_blob(blob),
            schema: SchemaRef::from_id(ManifestId::of_blob(b"test-schema")),
            content_hash: Hash::of(blob),
            byte_size: blob.len() as u64,
            created_at: Timestamp(1_700_000_000_000),
            last_accessed: None,
            sources: vec![BlobSource::Local {
                key: format!("blob:{}", Hash::of(blob).to_hex()),
            }],
            privacy: PrivacyClass::LocalOnly,
            provenance: ProvenanceRecord {
                origin: ProvenanceOrigin::Generated,
                upstream: Vec::new(),
                tooling: Some("eidetic-fjall-test".to_string()),
                generated_at: Timestamp(1_700_000_000_000),
            },
            trust: TrustEnvelope {
                level: TrustLevel::SelfAsserted,
                signatures: Vec::new(),
                moderation_state: ModerationState::Unreviewed,
            },
            schema_metadata: serde_json::Value::Null,
            manifest_version: BlobManifest::CURRENT_VERSION,
        };

        {
            let mut store = FjallStore::open(dir.path()).expect("fjall open");
            save_manifest(&mut store, &manifest).await.unwrap();
            let loaded = load_manifest(&mut store, manifest.id)
                .await
                .unwrap()
                .expect("manifest present after save");
            assert_eq!(loaded, manifest);
        }

        let mut reopened = FjallStore::open(dir.path()).expect("fjall reopen");
        let loaded = load_manifest(&mut reopened, manifest.id)
            .await
            .unwrap()
            .expect("manifest present after reopen");
        assert_eq!(loaded, manifest);
    });
}
