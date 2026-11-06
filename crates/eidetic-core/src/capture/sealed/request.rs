// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Capturing, verifying and acknowledging one sealed request.

use super::receipt::{key, put_receipt, read_receipt};
use super::{SEALED_TAG, load, parse_id, save};
use crate::PayloadSealer;
use crate::capture::{
    CaptureError, CaptureRequest, DepositReceipt, FetchedDocument, SourceAnnotator, SourceIdentity,
    StoredSourceDocument, is_html,
};
use crate::models::OpaqueBlob;

/// Oversized custody input is refused before extraction or any write.
const MAX_CUSTODY_PAYLOAD: usize = 512 * 1024;

fn check_document<A: SourceAnnotator>(
    annotator: &A,
    document: &FetchedDocument,
) -> Result<A::Record, CaptureError> {
    if document.acquired_at_ms == 0 || !is_html(document.content_type.as_deref()) {
        return Err("sealed capture needs observed HTML acquisition".into());
    }
    if document.effective_url.is_none() {
        return Err("sealed capture needs final URL".into());
    }
    annotator.annotate(document)
}

/// The live document when it matches the request's source; otherwise, only
/// once a receipt exists, the document rebuilt from the sealed raw and body
/// artifacts a prior attempt left.
async fn resolve_document<'a>(
    store: &mut dyn crate::Store,
    sealer: &dyn PayloadSealer,
    source: &SourceIdentity,
    have_receipt: bool,
    document: Option<&'a FetchedDocument>,
) -> Result<std::borrow::Cow<'a, FetchedDocument>, CaptureError> {
    match document {
        Some(doc) if SourceIdentity::of(doc) == *source => Ok(std::borrow::Cow::Borrowed(doc)),
        _ if have_receipt => {
            let raw: OpaqueBlob = load(store, sealer, parse_id(&source.hash)?).await?;
            let body: OpaqueBlob = load(store, sealer, parse_id(&source.body_hash)?).await?;
            let recovered = FetchedDocument {
                bytes: raw.0,
                body: String::from_utf8(body.0)
                    .map_err(|_| "sealed capture decoded body invalid")?,
                effective_url: source.effective_url.clone(),
                content_type: source.content_type.clone(),
                acquired_at_ms: source.acquired_at_ms,
            };
            if SourceIdentity::of(&recovered) != *source {
                return Err("sealed capture recovery identity mismatch".into());
            }
            Ok(std::borrow::Cow::Owned(recovered))
        }
        _ => Err("sealed capture request does not match live source".into()),
    }
}

/// Capture one request's source into the sealed namespace, or recover and
/// re-verify the capture a prior (possibly interrupted) attempt left.
///
/// Write order is the recovery contract: pending receipt, raw bytes, decoded
/// body, annotation, verify, completed receipt. Every step is idempotent at
/// its content address, so a retry after a failure at any single write
/// converges on the same stored identities.
///
/// # Errors
/// A [`CaptureError`] on a conflicting request, a live source that does not
/// match the request, custody refusal, oversized input, a role collision, or
/// a store failure.
pub async fn capture_request<A: SourceAnnotator>(
    store: &mut dyn crate::Store,
    sealer: &dyn PayloadSealer,
    annotator: &A,
    request: &CaptureRequest,
    document: Option<&FetchedDocument>,
) -> Result<StoredSourceDocument, CaptureError> {
    let source = request
        .source
        .as_ref()
        .ok_or("sealed capture missing source identity")?;
    let existing = read_receipt(store, sealer, &key(request.id)).await?;
    if let Some(held) = &existing {
        if held.request != *request {
            return Err("sealed capture request identity conflict".into());
        }
        if let Some(stored) = &held.stored {
            verify(store, sealer, annotator, request, stored).await?;
            return Ok(stored.clone());
        }
    }
    let document = resolve_document(store, sealer, source, existing.is_some(), document).await?;
    let document: &FetchedDocument = &document;
    if document.bytes.len() > MAX_CUSTODY_PAYLOAD || document.body.len() > MAX_CUSTODY_PAYLOAD {
        return Err("Private source exceeds the custody payload limit".into());
    }
    let record = check_document(annotator, document)?;
    let annotation_bytes = crate::TypedPayload::serialize_to_bytes(&record)
        .map_err(|_| "sealed annotation encoding")?;
    if annotation_bytes.len() > MAX_CUSTODY_PAYLOAD {
        return Err("Private source exceeds the custody payload limit".into());
    }
    let raw = crate::ManifestId::of_blob(&document.bytes);
    let annotation_id = crate::ManifestId::of_blob(&annotation_bytes);
    let body_id = crate::ManifestId::of_blob(document.body.as_bytes());
    let schema = annotator.schema_manifest();
    if raw == annotation_id
        || body_id == annotation_id
        || raw == schema
        || body_id == schema
        || annotation_id == schema
    {
        return Err("sealed capture artifact roles collide".into());
    }
    if existing.is_none() {
        put_receipt(
            store,
            sealer,
            &DepositReceipt {
                request: request.clone(),
                stored: None,
                saved: false,
            },
        )
        .await?;
    }
    save(
        store,
        sealer,
        &OpaqueBlob(document.bytes.clone()),
        document.acquired_at_ms,
    )
    .await?;
    save(
        store,
        sealer,
        &OpaqueBlob(document.body.as_bytes().to_vec()),
        document.acquired_at_ms,
    )
    .await?;
    save(store, sealer, &record, document.acquired_at_ms).await?;
    let stored = StoredSourceDocument {
        raw_manifest: raw.to_string(),
        annotation_manifest: format!("{SEALED_TAG}{annotation_id}"),
    };
    verify(store, sealer, annotator, request, &stored).await?;
    put_receipt(
        store,
        sealer,
        &DepositReceipt {
            request: request.clone(),
            stored: Some(stored.clone()),
            saved: false,
        },
    )
    .await?;
    Ok(stored)
}

/// Verify a stored sealed capture against its request: raw identity, the
/// annotation's acquisition evidence, and that both payloads unseal.
///
/// # Errors
/// A [`CaptureError`] naming the first mismatch or custody refusal.
pub async fn verify<A: SourceAnnotator>(
    store: &mut dyn crate::Store,
    sealer: &dyn PayloadSealer,
    annotator: &A,
    request: &CaptureRequest,
    stored: &StoredSourceDocument,
) -> Result<(), CaptureError> {
    let source = request
        .source
        .as_ref()
        .ok_or("sealed capture source missing")?;
    if stored.raw_manifest != source.hash {
        return Err("sealed capture raw identity mismatch".into());
    }
    let id = parse_id(
        stored
            .annotation_manifest
            .strip_prefix(SEALED_TAG)
            .ok_or("sealed source marker missing")?,
    )?;
    let record: A::Record = load(store, sealer, id).await?;
    annotator
        .validate(&record)
        .map_err(|_| "sealed source annotation invalid")?;
    let evidence = annotator
        .evidence(&record)
        .ok_or("sealed source acquisition evidence missing")?;
    let raw_id = parse_id(&source.hash)?;
    if !evidence.authoritative
        || !evidence.source_dom
        || evidence.has_replay_manifest
        || !evidence.is_html()
        || evidence.raw_manifest != Some(raw_id)
        || evidence.capture_hash != raw_id.0
        || source.effective_url.as_deref() != Some(&evidence.final_source)
        || source.acquired_at_ms != evidence.captured_at.0
        || source.content_type != evidence.response_content_type
    {
        return Err("sealed capture acquisition mismatch".into());
    }
    let _: OpaqueBlob = load(store, sealer, raw_id).await?;
    let _: OpaqueBlob = load(store, sealer, parse_id(&source.body_hash)?).await?;
    Ok(())
}

/// Mark a verified deposit as saved by the host. Idempotent.
///
/// # Errors
/// A [`CaptureError`] when the receipt is missing, belongs to a different
/// request, the deposit is incomplete, or verification fails.
pub async fn acknowledge<A: SourceAnnotator>(
    store: &mut dyn crate::Store,
    sealer: &dyn PayloadSealer,
    annotator: &A,
    request: &CaptureRequest,
) -> Result<(), CaptureError> {
    let mut receipt = read_receipt(store, sealer, &key(request.id))
        .await?
        .ok_or("sealed capture receipt missing")?;
    if receipt.request != *request {
        return Err("sealed capture acknowledgement conflict".into());
    }
    verify(
        store,
        sealer,
        annotator,
        request,
        receipt
            .stored
            .as_ref()
            .ok_or("sealed capture deposit incomplete")?,
    )
    .await?;
    if !receipt.saved {
        receipt.saved = true;
        put_receipt(store, sealer, &receipt).await?;
    }
    Ok(())
}
