// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Admit only the exact locally addressed raw/annotation pair.

use crate::capture::{CaptureError, SourceAnnotator, StoredSourceDocument};
use crate::{ManifestId, PrivacyClass, ProvenanceOrigin};

fn manifest_id(value: &str) -> Result<ManifestId, String> {
    serde_json::from_value(serde_json::Value::String(value.to_owned()))
        .map_err(|error| format!("source manifest identity: {error}"))
}

async fn checked_manifest(
    store: &mut dyn crate::Store,
    id: ManifestId,
) -> Result<crate::BlobManifest, CaptureError> {
    let manifest = crate::manifest::load_manifest(store, id)
        .await
        .map_err(|error| CaptureError::from(error).context("source manifest read"))?
        .ok_or_else(|| format!("source manifest {id} is missing"))?;
    if manifest.id != id || manifest.content_hash != id.0 {
        return Err(format!("source manifest identity mismatch at {id}").into());
    }
    if manifest.privacy != PrivacyClass::LocalOnly {
        return Err(format!("source manifest {id} is not private local evidence").into());
    }
    let key = format!("blob:{}", id.0.to_hex());
    if !manifest
        .sources
        .iter()
        .any(|source| matches!(source, crate::BlobSource::Local { key: local } if local == &key))
    {
        return Err(format!("source manifest {id} has no exact local payload").into());
    }
    let bytes = store
        .get(&key)
        .await
        .map_err(|error| CaptureError::from(error).context("source payload read"))?
        .ok_or_else(|| format!("source payload {id} is missing"))?;
    if crate::Hash::of(&bytes) != id.0 || bytes.len() as u64 != manifest.byte_size {
        return Err(format!("source payload integrity mismatch at {id}").into());
    }
    Ok(manifest)
}

/// [`verify_source_document_typed`] with the error rendered to a string.
///
/// # Errors
/// The rendered refusal.
pub async fn verify_source_document<A: SourceAnnotator>(
    store: &mut dyn crate::Store,
    annotator: &A,
    stored: &StoredSourceDocument,
) -> Result<(), String> {
    verify_source_document_typed(store, annotator, stored)
        .await
        .map_err(|error| error.to_string())
}

/// Admit only the exact locally addressed raw/annotation pair. Eidetic checks
/// typed payload hashes and the annotator checks annotation facts; this binds
/// the requested key to the manifest identity and the authoritative raw
/// source. No fetcher can repair missing or damaged saved evidence here.
///
/// # Errors
/// A [`CaptureError`] naming the first missing, mismatched or inconsistent
/// part.
pub async fn verify_source_document_typed<A: SourceAnnotator>(
    store: &mut dyn crate::Store,
    annotator: &A,
    stored: &StoredSourceDocument,
) -> Result<(), CaptureError> {
    let raw_id = manifest_id(&stored.raw_manifest)?;
    let annotation_id = manifest_id(&stored.annotation_manifest)?;
    let raw_manifest = checked_manifest(store, raw_id).await?;
    let annotation_manifest = checked_manifest(store, annotation_id).await?;
    let record = annotator
        .load(store, annotation_id)
        .await
        .map_err(|error| error.context("source annotation validation"))?
        .ok_or_else(|| "source annotation is missing".to_string())?;
    let evidence = annotator
        .evidence(&record)
        .ok_or_else(|| "source annotation lacks acquisition evidence".to_string())?;
    if !evidence.authoritative
        || !evidence.source_dom
        || !evidence.is_html()
        || evidence.raw_manifest != Some(raw_id)
        || evidence.capture_hash != raw_id.0
        || evidence.has_replay_manifest
        // A raw blob is content addressed: identical bytes acquired from two
        // URLs/times share it. Acquisition identity belongs to the annotation,
        // not the mutable metadata of that shared raw manifest.
        || !matches!(raw_manifest.provenance.origin, ProvenanceOrigin::Imported { .. })
        || annotation_manifest.created_at != evidence.captured_at
        || annotation_manifest.provenance.generated_at != evidence.captured_at
        || annotation_manifest.provenance.origin != ProvenanceOrigin::Generated
    {
        return Err("source annotation and raw acquisition provenance disagree".into());
    }
    crate::load_typed::<crate::models::OpaqueBlob>(store, &mut crate::NoFetcher, raw_id)
        .await
        .map_err(|error| CaptureError::from(error).context("raw source validation"))?
        .ok_or_else(|| "raw source is missing".to_string())?;
    Ok(())
}
