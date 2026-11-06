// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Unsealed capture-source documents: exact raw bytes plus an annotation.
//!
//! Eidetic ids address payload bytes, not (schema, bytes). Every typed role is
//! planned and checked before any deposit, so one role cannot replace another
//! valid artifact's mutable manifest at the same content address.

mod preflight;
mod verify;

use crate::capture::{
    CaptureError, FetchedDocument, SourceAnnotator, StoredSourceDocument, is_html,
};
use crate::models::OpaqueBlob;
use crate::{
    ModerationState, PrivacyClass, ProvenanceOrigin, ProvenanceRecord, Timestamp, TrustEnvelope,
    TrustLevel, TypedPayload,
};

pub use preflight::preflight_source_artifact;
pub use verify::{verify_source_document, verify_source_document_typed};

async fn require_acquisition_provenance(
    store: &mut dyn crate::Store,
    raw_manifest: crate::ManifestId,
) -> Result<(), CaptureError> {
    let manifest = crate::manifest::load_manifest(store, raw_manifest)
        .await
        .map_err(|error| CaptureError::from(error).context("raw source preflight"))?
        .ok_or("raw source preflight: manifest vanished")?;
    if !matches!(
        manifest.provenance.origin,
        ProvenanceOrigin::Imported { .. }
    ) {
        return Err("raw source preflight: incompatible existing acquisition provenance".into());
    }
    Ok(())
}

async fn deposit_raw(
    store: &mut dyn crate::Store,
    document: &FetchedDocument,
    effective_url: String,
    captured_at: Timestamp,
) -> Result<(), CaptureError> {
    crate::save_typed(
        store,
        &OpaqueBlob(document.bytes.clone()),
        Vec::new(),
        PrivacyClass::LocalOnly,
        ProvenanceRecord {
            origin: ProvenanceOrigin::Imported {
                source: effective_url,
            },
            upstream: Vec::new(),
            tooling: Some("turnstone-source-document-capture/v1".into()),
            generated_at: captured_at,
        },
        TrustEnvelope {
            level: TrustLevel::SelfAsserted,
            signatures: Vec::new(),
            moderation_state: ModerationState::Unreviewed,
        },
        captured_at,
    )
    .await
    .map_err(|error| CaptureError::from(error).context("raw source deposit"))?;
    Ok(())
}

/// Store the exact acquired bytes and the annotation that describes a
/// canonical-text projection of those bytes. Invoked only by an explicit
/// action path, never by fetch enrichment or recall.
///
/// A retry after a failure (or an acknowledgement lost after a write) at any
/// single write converges on the same identities and never invents a second
/// artifact: raw and annotation deposits are skipped when an exact artifact is
/// already present, and the resource association is rewritten idempotently.
///
/// # Errors
/// A [`CaptureError`] when the document is not an observed HTML response with
/// a final URL, roles would collide, an existing artifact is incompatible, a
/// write fails, or the stored pair does not verify.
pub async fn capture_source_document_in<A: SourceAnnotator>(
    store: &mut dyn crate::Store,
    annotator: &A,
    document: &FetchedDocument,
) -> Result<StoredSourceDocument, CaptureError> {
    if document.acquired_at_ms == 0 {
        return Err("source capture requires an observed acquisition time".into());
    }
    if !is_html(document.content_type.as_deref()) {
        return Err("source capture requires an observed text/html response content type".into());
    }
    let captured_at = Timestamp(document.acquired_at_ms);
    let effective_url = document
        .effective_url
        .clone()
        .ok_or_else(|| "source capture requires an observed final response URL".to_string())?;
    let raw_manifest = crate::ManifestId::of_blob(&document.bytes);
    let record = annotator.annotate(document)?;
    let annotation_bytes = record
        .serialize_to_bytes()
        .map_err(|error| format!("Fleece annotation encoding: {error}"))?;
    let annotation_manifest = crate::ManifestId::of_blob(&annotation_bytes);
    let schema_manifest = annotator.schema_manifest();
    if raw_manifest == annotation_manifest
        || raw_manifest == schema_manifest
        || annotation_manifest == schema_manifest
    {
        return Err("source capture has incompatible typed roles at one content address".into());
    }
    let raw_exists = preflight_source_artifact(
        store,
        raw_manifest,
        OpaqueBlob::schema_ref(),
        PrivacyClass::LocalOnly,
        Some(&document.bytes),
    )
    .await?;
    let annotation_exists = preflight_source_artifact(
        store,
        annotation_manifest,
        A::Record::schema_ref(),
        PrivacyClass::LocalOnly,
        Some(&annotation_bytes),
    )
    .await?;
    preflight_source_artifact(
        store,
        schema_manifest,
        *crate::META_SCHEMA_REF,
        PrivacyClass::PublicPortable,
        None,
    )
    .await?;
    if raw_exists {
        require_acquisition_provenance(store, raw_manifest).await?;
    }
    let stored = StoredSourceDocument {
        raw_manifest: raw_manifest.to_string(),
        annotation_manifest: annotation_manifest.to_string(),
    };
    if annotation_exists {
        verify_source_document_typed(store, annotator, &stored).await?;
    }
    if !raw_exists {
        deposit_raw(store, document, effective_url, captured_at).await?;
    }
    annotator
        .bootstrap_schema(store)
        .await
        .map_err(|error| error.context("Fleece schema bootstrap"))?;
    if annotation_exists {
        // Reusing a complete annotation preserves its exact manifest metadata.
        // Its resource association is independently idempotent and may have
        // been the interrupted final write of the previous deposit attempt.
        let (source, capture_hash) = annotator.association(&record);
        crate::ResourceCaptureStore::new(&*store)
            .record(
                &source,
                capture_hash,
                crate::CaptureContent::Acquired,
                document.acquired_at_ms,
            )
            .await
            .map_err(|error| CaptureError::from(error).context("Fleece capture association"))?;
    } else {
        annotator
            .save(store, &record, document.acquired_at_ms)
            .await
            .map_err(|error| error.context("Fleece annotation deposit"))?;
    }
    verify_source_document_typed(store, annotator, &stored).await?;
    Ok(stored)
}
