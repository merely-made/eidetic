// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The `SearchIndexSpec` codicil — the index's hand-off contract.
//!
//! Names what a `TrailIndex` is made of: the logical field set (and its
//! version), the tokenizer, and the **scoring engine** that wrote the
//! projection. The engine string travels beside the data and a reader
//! rejects-or-re-mints on mismatch (re-minting is cheap: the trace corpus is
//! itself codicils). A directory left by the retired tantivy build carries
//! its own engine string, so it refuses cleanly rather than being misread.
//!
//! Locally the spec rides a JSON sidecar in the index directory
//! ([`SPEC_SIDECAR`]) so `open` can refuse before reading the projection; as a
//! typed payload it is mintable into the eidetic store ([`save_spec`]) — the
//! shape the deferred consume half verifies before merging a shared index.

use serde::{Deserialize, Serialize};
use std::path::Path;

use eidetic::Store;
use eidetic::schema::{
    Hash, ManifestId, ModerationState, PrivacyClass, ProvenanceOrigin, ProvenanceRecord, SchemaRef,
    Timestamp, TrustEnvelope, TrustLevel,
};
use eidetic::typed::{TypedPayload, save_typed};

use crate::{Result, SearchError};

/// The logical field set's version (bump when fields/columns change shape).
pub const FIELDS_V1: u32 = 1;

/// v2 added the `text` (page main-text) field for BM25 body recall (C5); an
/// index written at v1 mismatches and re-mints from the corpus.
pub const FIELDS_V2: u32 = 2;

/// v3 added a tokenized, non-stored `url_text` field while retaining the
/// canonical `url` field as an exact stored string. Indexes written at v2
/// re-mint so URL path and host components become recallable.
pub const FIELDS_V3: u32 = 3;

/// v4 is the in-tree engine's field set: three scored fields (URL tokens,
/// title, page text) with per-field weights, and domain/owner/at_ms/transition
/// as stored columns the reports read directly. Everything older re-mints.
pub const FIELDS_V4: u32 = 4;

/// The scoring engine that writes the projection. Bumped when the ranking or
/// the projection's shape changes, not when the crate's version does.
pub const ENGINE: &str = "mere-bm25/1";

/// The spec sidecar's file name inside an index directory.
pub const SPEC_SIDECAR: &str = "mere-search-spec.json";

/// Canonical bytes of the `SearchIndexSpec` schema codicil's payload.
const SEARCH_INDEX_SCHEMA_PAYLOAD: &[u8] = br#"{"format":"mere-native","schema_id":"eidetic.SearchIndexSpec/v2","body":{"version":2,"description":"Contract for a lexical trail index: field set, tokenizer, and scoring engine.","required":["engine_version","fields_version","tokenizer"],"fields":{"engine_version":{"type":"string"},"fields_version":{"type":"integer"},"tokenizer":{"type":"string"}}}}"#;

/// The well-known schema reference for [`SearchIndexSpec`] payloads.
pub static SEARCH_INDEX_SCHEMA_REF: std::sync::LazyLock<SchemaRef> =
    std::sync::LazyLock::new(|| {
        SchemaRef::from_id(ManifestId::from_hash(Hash::of(SEARCH_INDEX_SCHEMA_PAYLOAD)))
    });

/// Idempotently seed the `SearchIndexSpec` schema codicil into a Store.
pub async fn bootstrap_search_schema(store: &mut dyn Store) -> eidetic::Result<()> {
    let id = SEARCH_INDEX_SCHEMA_REF.0;
    if eidetic::manifest::load_manifest(store, id).await?.is_some() {
        return Ok(());
    }
    let local_key = format!("blob:{}", Hash::of(SEARCH_INDEX_SCHEMA_PAYLOAD).to_hex());
    store.put(&local_key, SEARCH_INDEX_SCHEMA_PAYLOAD).await?;
    let manifest = eidetic::manifest::BlobManifest {
        id,
        schema: *eidetic::schema_def::META_SCHEMA_REF,
        content_hash: Hash::of(SEARCH_INDEX_SCHEMA_PAYLOAD),
        byte_size: SEARCH_INDEX_SCHEMA_PAYLOAD.len() as u64,
        created_at: Timestamp::ZERO,
        last_accessed: None,
        sources: vec![eidetic::manifest::BlobSource::Local { key: local_key }],
        privacy: PrivacyClass::PublicPortable,
        provenance: ProvenanceRecord {
            origin: ProvenanceOrigin::Generated,
            upstream: Vec::new(),
            tooling: Some(format!("eidetic-search/{}", env!("CARGO_PKG_VERSION"))),
            generated_at: Timestamp::ZERO,
        },
        trust: TrustEnvelope {
            level: TrustLevel::CheckpointAccepted,
            signatures: Vec::new(),
            moderation_state: ModerationState::Accepted,
        },
        schema_metadata: serde_json::Value::Null,
        manifest_version: eidetic::manifest::BlobManifest::CURRENT_VERSION,
    };
    eidetic::manifest::save_manifest(store, &manifest).await
}

/// The index's contract: what wrote it, and in what shape.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchIndexSpec {
    /// The scoring engine that wrote the projection ([`ENGINE`]). The alias
    /// reads a sidecar from the retired tantivy build so it refuses as a
    /// format mismatch rather than a parse error.
    #[serde(alias = "tantivy_version")]
    pub engine_version: String,
    /// The logical field set's version ([`FIELDS_V4`]).
    pub fields_version: u32,
    /// Tokenizer name for the text fields.
    pub tokenizer: String,
}

impl SearchIndexSpec {
    /// The spec this build of the crate writes.
    pub fn current() -> Self {
        Self {
            engine_version: ENGINE.to_string(),
            fields_version: FIELDS_V4,
            tokenizer: crate::tokenize::TOKENIZER_NAME.to_string(),
        }
    }

    /// Whether an on-disk spec is readable by this build.
    pub fn matches_current(&self) -> bool {
        *self == Self::current()
    }

    /// One line for a mismatch message.
    pub fn describe(&self) -> String {
        format!("{} (fields v{})", self.engine_version, self.fields_version)
    }

    /// Write the sidecar into an index directory.
    pub fn write_sidecar(&self, dir: &Path) -> Result<()> {
        let bytes = serde_json::to_vec_pretty(self)
            .map_err(|e| SearchError::Io(format!("spec serialize: {e}")))?;
        std::fs::write(dir.join(SPEC_SIDECAR), bytes)?;
        Ok(())
    }

    /// Read the sidecar from an index directory; `Missing` when there is
    /// none (no index, or one minted before specs existed — re-mint).
    pub fn read_sidecar(dir: &Path) -> Result<Self> {
        let path = dir.join(SPEC_SIDECAR);
        let bytes =
            std::fs::read(&path).map_err(|_| SearchError::Missing(dir.display().to_string()))?;
        serde_json::from_slice(&bytes).map_err(|e| SearchError::Io(format!("spec parse: {e}")))
    }
}

impl TypedPayload for SearchIndexSpec {
    fn schema_ref() -> SchemaRef {
        *SEARCH_INDEX_SCHEMA_REF
    }
}

/// Mint the spec into the eidetic store as a typed payload (LocalOnly; the
/// consume half's sharing promotion is deferred with it).
pub async fn save_spec(
    store: &mut dyn Store,
    spec: &SearchIndexSpec,
    now_ms: u64,
) -> eidetic::Result<ManifestId> {
    save_typed(
        store,
        spec,
        Vec::new(),
        PrivacyClass::LocalOnly,
        ProvenanceRecord {
            origin: ProvenanceOrigin::Generated,
            upstream: Vec::new(),
            tooling: Some(format!("eidetic-search/{}", env!("CARGO_PKG_VERSION"))),
            generated_at: Timestamp(now_ms),
        },
        TrustEnvelope {
            level: TrustLevel::SelfAsserted,
            signatures: Vec::new(),
            moderation_state: ModerationState::Unreviewed,
        },
        Timestamp(now_ms),
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_current_spec_matches_itself_and_rejects_a_drifted_one() {
        let current = SearchIndexSpec::current();
        assert!(current.matches_current());
        assert!(!current.engine_version.is_empty());
        assert_eq!(current.fields_version, FIELDS_V4);

        let drifted = SearchIndexSpec {
            engine_version: "tantivy 0.1.0".to_string(),
            ..current.clone()
        };
        assert!(!drifted.matches_current());

        for older in [FIELDS_V1, FIELDS_V2, FIELDS_V3] {
            let stale = SearchIndexSpec {
                fields_version: older,
                ..current.clone()
            };
            assert!(!stale.matches_current());
        }
    }

    #[test]
    fn a_tantivy_era_sidecar_parses_into_a_mismatching_spec() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join(SPEC_SIDECAR),
            br#"{"tantivy_version":"tantivy 0.26.0","fields_version":3,"tokenizer":"default"}"#,
        )
        .unwrap();
        let read = SearchIndexSpec::read_sidecar(dir.path()).unwrap();
        assert_eq!(read.engine_version, "tantivy 0.26.0");
        assert!(!read.matches_current());
        assert!(read.describe().contains("fields v3"));
    }

    #[test]
    fn the_sidecar_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let spec = SearchIndexSpec::current();
        spec.write_sidecar(dir.path()).unwrap();
        let read = SearchIndexSpec::read_sidecar(dir.path()).unwrap();
        assert_eq!(read, spec);
    }

    #[test]
    fn a_missing_sidecar_reads_as_missing() {
        let dir = tempfile::tempdir().unwrap();
        assert!(matches!(
            SearchIndexSpec::read_sidecar(dir.path()),
            Err(SearchError::Missing(_))
        ));
    }
}
