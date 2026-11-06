// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Sealed capture primitives over a caller-owned store.
//!
//! The caller owns a separate store and serializes access (Turnstone: its
//! trail actor). Legacy unsealed stores are never opened here, and a sealed
//! namespace never adopts or overwrites an unsealed or partial role. The key
//! layout (`turnstone/sealed-source-capture/v1/<request id>` receipts, the
//! `blob:`/`manifest:` pairs, the `sealed-v1:` annotation tag) is the stored
//! format Turnstone's existing sessions carry; it does not change here.
//!
//! No call uses `transact`. Correctness rests on ordered writes,
//! content-addressed idempotent artifacts and receipt-then-recover, so the
//! module works unchanged over a backend without cross-key atomicity.

mod receipt;
mod request;

use crate::capture::CaptureError;
use crate::{PayloadSealer, TypedPayload};

pub use receipt::receipts;
pub use request::{acknowledge, capture_request, verify};

/// Receipt key prefix; the full key is the prefix plus the request id.
pub const RECEIPT_PREFIX: &str = "turnstone/sealed-source-capture/v1/";
/// The tag a sealed annotation manifest id carries in [`StoredSourceDocument`](crate::capture::StoredSourceDocument).
pub const SEALED_TAG: &str = "sealed-v1:";

/// Save a typed payload sealed and local-only, refusing any conflicting,
/// unsealed or partial artifact already at its content address.
///
/// # Errors
/// A [`CaptureError`] when custody refuses, an existing artifact conflicts or
/// is incomplete, or the store fails.
pub async fn save<T: TypedPayload>(
    store: &mut dyn crate::Store,
    sealer: &dyn PayloadSealer,
    value: &T,
    _at: u64,
) -> Result<crate::ManifestId, CaptureError> {
    let clear = value
        .serialize_to_bytes()
        .map_err(|_| "sealed capture payload encoding")?;
    let id = crate::ManifestId::of_blob(&clear);
    let manifest = crate::manifest::load_manifest(store, id)
        .await
        .map_err(|error| CaptureError::from(error).context("sealed capture manifest read"))?;
    let blob_key = format!("blob:{}", id.0.to_hex());
    let existing = store
        .get(&blob_key)
        .await
        .map_err(|error| CaptureError::from(error).context("sealed capture blob read"))?;
    match (manifest, existing) {
        (Some(m), Some(_)) => {
            if m.id != id
                || m.content_hash != id.0
                || m.schema != T::schema_ref()
                || m.privacy != crate::PrivacyClass::LocalOnly
                || m.byte_size != clear.len() as u64
                || m.sources.len() != 1
                || !matches!(&m.sources[0], crate::BlobSource::Local { key } if key == &blob_key)
                || crate::seal::seal_marker(&m)
                    .map_err(|_| "sealed capture marker invalid")?
                    .is_none()
            {
                return Err("sealed capture refuses a conflicting or unsealed artifact".into());
            }
            let value =
                crate::load_typed_sealed::<T>(store, &mut crate::NoFetcher, Some(sealer), id)
                    .await
                    .map_err(|error| {
                        CaptureError::from(error)
                            .context("sealed capture existing artifact refused")
                    })?
                    .ok_or("sealed capture existing artifact missing")?;
            if value
                .serialize_to_bytes()
                .map_err(|_| "sealed capture payload encoding")?
                != clear
            {
                return Err("sealed capture existing artifact differs".into());
            }
            return Ok(id);
        }
        (None, None) => {}
        _ => return Err("sealed capture incomplete artifact requires repair".into()),
    }
    crate::save_typed_sealed(
        store,
        Some(sealer),
        value,
        Vec::new(),
        crate::PrivacyClass::LocalOnly,
        crate::ProvenanceRecord {
            origin: crate::ProvenanceOrigin::Generated,
            upstream: Vec::new(),
            tooling: Some("turnstone-sealed-source/v1".into()),
            generated_at: crate::Timestamp(0),
        },
        crate::TrustEnvelope {
            level: crate::TrustLevel::SelfAsserted,
            signatures: Vec::new(),
            moderation_state: crate::ModerationState::Unreviewed,
        },
        crate::Timestamp(0),
    )
    .await
    .map_err(|error| CaptureError::from(error).context("sealed capture artifact write refused"))
}

/// Load a sealed local-only artifact, checking manifest role, marker, size and
/// digest before returning the value.
///
/// # Errors
/// A [`CaptureError`] when the artifact is missing, unsealed, conflicting,
/// refused by custody, or fails its size or digest check.
pub async fn load<T: TypedPayload>(
    store: &mut dyn crate::Store,
    sealer: &dyn PayloadSealer,
    id: crate::ManifestId,
) -> Result<T, CaptureError> {
    let m = crate::manifest::load_manifest(store, id)
        .await
        .map_err(|error| CaptureError::from(error).context("sealed source manifest read failed"))?
        .ok_or("sealed source manifest missing")?;
    let blob_key = format!("blob:{}", id.0.to_hex());
    if m.id != id
        || m.content_hash != id.0
        || m.schema != T::schema_ref()
        || m.privacy != crate::PrivacyClass::LocalOnly
        || m.sources.len() != 1
        || !matches!(&m.sources[0], crate::BlobSource::Local { key } if key == &blob_key)
        || crate::seal::seal_marker(&m)
            .map_err(|_| "sealed source marker invalid")?
            .is_none()
    {
        return Err("sealed source refuses unsealed or conflicting artifact".into());
    }
    let value = crate::load_typed_sealed::<T>(store, &mut crate::NoFetcher, Some(sealer), id)
        .await
        .map_err(|error| {
            CaptureError::from(error).context("sealed source custody or integrity refusal")
        })?
        .ok_or("sealed source payload missing")?;
    let encoded = value
        .serialize_to_bytes()
        .map_err(|_| "sealed source encoding invalid")?;
    if encoded.len() as u64 != m.byte_size || crate::Hash::of(&encoded) != id.0 {
        return Err("sealed source manifest size or digest mismatch".into());
    }
    Ok(value)
}

/// Parse a manifest id from its string form.
///
/// # Errors
/// When the string is not a manifest id.
pub fn parse_id(s: &str) -> Result<crate::ManifestId, CaptureError> {
    serde_json::from_value(serde_json::Value::String(s.to_owned()))
        .map_err(|_| "sealed source identity invalid".into())
}
