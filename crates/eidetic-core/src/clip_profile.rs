// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The first consumer draft of a portable clip. Private source backing is a
//! separate retained record, never part of the portable record by default.
//! Storage/disclosure authority, graph admission and live operations belong to
//! the caller. Unknown selector and evidence vocabulary is preserved as data.

use crate::{Hash, ManifestId, SchemaRef, TypedPayload};
use serde::{Deserialize, Serialize};

pub const CLIP_SCHEMA_ID: &str = "mere.clip-draft/v1";
pub const CLIP_SCHEMA: &[u8] = br#"{"format":"mere-native","schema_id":"mere.clip-draft/v1","body":{"version":1,"required":["version","clip_id","capture_id","title","source","source_evidence","parts","anchors","tags"],"fields":{"version":{"type":"u64"},"clip_id":{"type":"string"},"capture_id":{"type":"string"},"title":{"type":"string"},"source":{"type":"object"},"parts":{"type":"array"},"anchors":{"type":"array"},"tags":{"type":"array"}}}}"#;
pub const RETAINED_SCHEMA: &[u8] = br#"{"format":"mere-native","schema_id":"mere.retained-clip-draft/v1","body":{"version":1,"required":["clip","private_backing"],"fields":{"clip":{"type":"object"},"private_backing":{"type":"array"}}}}"#;

/// Durable domain identity; no scene instance, playback token or grant.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClipSourceV1 {
    pub owner: Option<String>,
    pub entity: String,
    pub revision: Option<String>,
}

/// One declared part. Dependencies name parts, rather than arbitrary graph
/// neighbors. Role/media/schema vocabulary belongs to the producing domain.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClipPartV1 {
    pub name: String,
    pub role: String,
    pub media_type: String,
    pub schema: Option<SchemaRef>,
    pub manifest: ManifestId,
    pub dependencies: Vec<String>,
}

/// Selector basis and units are explicit. The original native anchor remains
/// source evidence even when complete source bytes are not disclosed.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClipAnchorV1 {
    pub source: String,
    pub representation: Option<String>,
    pub units: String,
    pub selector: serde_json::Value,
    pub evidence: serde_json::Value,
}

/// A tag assertion is not a label alone. Unknown historical attribution remains
/// unknown; importing a record never attributes it to the receiving user.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClipTagV1 {
    pub concept: String,
    pub owner: String,
    pub label: String,
    pub target: String,
    pub statement_id: String,
    pub asserter: Option<String>,
    pub scope: serde_json::Value,
    pub asserted_at_ms: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PortableClipV1 {
    pub version: u32,
    pub clip_id: String,
    pub capture_id: String,
    pub title: String,
    pub source: ClipSourceV1,
    /// Native source facts with unknown attribution remain evidence, not tags.
    pub source_evidence: serde_json::Value,
    pub parts: Vec<ClipPartV1>,
    pub anchors: Vec<ClipAnchorV1>,
    pub tags: Vec<ClipTagV1>,
}

impl TypedPayload for PortableClipV1 {
    fn schema_ref() -> SchemaRef {
        SchemaRef::from_id(ManifestId::from_hash(Hash::of(CLIP_SCHEMA)))
    }
}

/// Local custody of the selected content and optional source backing. Export
/// takes the `clip` field; backing requires a distinct, explicit disclosure.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RetainedClipV1 {
    pub clip: PortableClipV1,
    pub private_backing: Vec<ClipPartV1>,
}

impl TypedPayload for RetainedClipV1 {
    fn schema_ref() -> SchemaRef {
        SchemaRef::from_id(ManifestId::from_hash(Hash::of(RETAINED_SCHEMA)))
    }
}

/// Seed the two built-in draft definitions without reserializing canonical
/// bytes. This is not a registry or permission to interpret foreign formats.
pub async fn bootstrap_clip_schemas(store: &mut dyn crate::Store) -> crate::Result<()> {
    crate::bootstrap_meta_schema(store).await?;
    for bytes in [CLIP_SCHEMA, RETAINED_SCHEMA] {
        let hash = Hash::of(bytes);
        let id = ManifestId::from_hash(hash);
        if crate::manifest::load_manifest(store, id).await?.is_some() {
            continue;
        }
        let key = format!("blob:{}", hash.to_hex());
        store.put(&key, bytes).await?;
        crate::manifest::save_manifest(
            store,
            &crate::BlobManifest {
                id,
                schema: *crate::META_SCHEMA_REF,
                content_hash: hash,
                byte_size: bytes.len() as u64,
                created_at: crate::Timestamp::ZERO,
                last_accessed: None,
                sources: vec![crate::BlobSource::Local { key }],
                privacy: crate::PrivacyClass::PublicPortable,
                provenance: crate::ProvenanceRecord::self_imported(
                    "eidetic built-in clip draft schemas",
                ),
                trust: crate::TrustEnvelope::self_asserted(),
                schema_metadata: serde_json::Value::Null,
                manifest_version: crate::BlobManifest::CURRENT_VERSION,
            },
        )
        .await?;
    }
    Ok(())
}
