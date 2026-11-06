// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Strict typed structured-research payloads in the sealed owner.
//!
//! Property definitions and records deposit sealed and local-only, and reads
//! check stored manifest and ciphertext sizes against a ceiling before storage
//! materializes them ([`BoundedRead`]). Frozen-result deposits sit in
//! `eidetic_search::records` because replaying a result needs the evaluator.

use super::{BoundedRead, CaptureError, sealed};
use crate::records::{PropertyDefinitionV1, PropertyRecordV1};
use crate::{ManifestId, PayloadSealer, Store, TypedPayload};

/// Load a sealed typed payload under a byte ceiling, returning it with the
/// clear size used. Checks the manifest role, seal marker, ciphertext size,
/// digest and the canonical-JSON identity of the payload.
///
/// # Errors
/// A [`CaptureError`] when anything is missing, oversized, unsealed,
/// conflicting, refused by custody, or not canonically encoded.
pub async fn load_bounded<T: TypedPayload>(
    store: &mut dyn BoundedRead,
    sealer: &dyn PayloadSealer,
    id: ManifestId,
    max_bytes: usize,
) -> Result<(T, usize), CaptureError> {
    let manifest_bytes = store
        .get_limited(&id.store_key(), max_bytes)
        .await?
        .ok_or("Structured manifest missing")?;
    if manifest_bytes.len() > max_bytes {
        return Err("Structured manifest byte limit exceeded".into());
    }
    let manifest: crate::BlobManifest =
        serde_json::from_slice(&manifest_bytes).map_err(|_| "Structured manifest invalid")?;
    let key = format!("blob:{}", id.0.to_hex());
    let marker = crate::seal::seal_marker(&manifest)
        .map_err(|_| "Structured seal marker invalid")?
        .ok_or("Structured artifact is not sealed")?;
    if manifest.id != id
        || manifest.content_hash != id.0
        || manifest.schema != T::schema_ref()
        || manifest.privacy != crate::PrivacyClass::LocalOnly
        || manifest.sources.len() != 1
        || !matches!(&manifest.sources[0], crate::BlobSource::Local { key: held } if held == &key)
        || manifest.byte_size > max_bytes as u64
    {
        return Err("Structured artifact metadata or byte limit refused".into());
    }
    let ciphertext = store
        .get_limited(&key, max_bytes)
        .await?
        .ok_or("Structured ciphertext missing")?;
    if ciphertext.len() > max_bytes {
        return Err("Structured ciphertext byte limit exceeded".into());
    }
    let clear = sealer
        .unseal(&manifest.content_hash, &marker, &ciphertext)
        .map_err(|_| "Structured custody or integrity refusal")?;
    if clear.len() > max_bytes
        || clear.len() as u64 != manifest.byte_size
        || crate::Hash::of(&clear) != id.0
    {
        return Err("Structured payload size or digest mismatch".into());
    }
    let value = T::deserialize_from_bytes(&clear).map_err(|_| "Structured payload invalid")?;
    // These payloads use TypedPayload's canonical JSON encoding; compare the
    // re-encoding to the stored bytes without materializing a second copy.
    struct Compare<'a> {
        expected: &'a [u8],
        offset: usize,
    }
    impl std::io::Write for Compare<'_> {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            let end = self
                .offset
                .checked_add(bytes.len())
                .ok_or_else(|| std::io::Error::other("identity overflow"))?;
            if self.expected.get(self.offset..end) != Some(bytes) {
                return Err(std::io::Error::other("identity mismatch"));
            }
            self.offset = end;
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut compare = Compare {
        expected: &clear,
        offset: 0,
    };
    serde_json::to_writer(&mut compare, &value)
        .map_err(|_| "Structured payload canonical identity mismatch")?;
    if compare.offset != clear.len() {
        return Err("Structured payload canonical identity mismatch".into());
    }
    Ok((value, clear.len()))
}

/// Require the payload's schema to be present in the store.
///
/// # Errors
/// The store's or schema's refusal.
pub async fn validate_schema<T: TypedPayload>(
    store: &mut dyn Store,
    value: &T,
) -> Result<(), CaptureError> {
    crate::records::validate_required_schema(store, &mut crate::NoFetcher, value)
        .await
        .map_err(CaptureError::from)
}

/// Deposit a validated property definition, sealed.
///
/// # Errors
/// A [`CaptureError`] on an invalid definition, an identity or readback
/// mismatch, custody refusal or a store failure.
pub async fn deposit_definition(
    store: &mut dyn BoundedRead,
    sealer: &dyn PayloadSealer,
    definition: &PropertyDefinitionV1,
) -> Result<ManifestId, CaptureError> {
    definition
        .validate()
        .map_err(|e| format!("Invalid property definition: {e:?}"))?;
    crate::records::bootstrap_record_schemas(store).await?;
    validate_schema(store, definition).await?;
    let id = sealed::save(store, sealer, definition, 0).await?;
    if id != definition.manifest_id().map_err(|e| e.to_string())? {
        return Err("Property definition identity mismatch".into());
    }
    if load_definition(store, sealer, id).await? != *definition {
        return Err("Property definition readback mismatch".into());
    }
    Ok(id)
}

/// Load a sealed property definition.
///
/// # Errors
/// A [`CaptureError`] as [`load_bounded`], or an invalid definition.
pub async fn load_definition(
    store: &mut dyn BoundedRead,
    sealer: &dyn PayloadSealer,
    id: ManifestId,
) -> Result<PropertyDefinitionV1, CaptureError> {
    load_definition_bounded(store, sealer, id, usize::MAX).await
}

async fn load_definition_bounded(
    store: &mut dyn BoundedRead,
    sealer: &dyn PayloadSealer,
    id: ManifestId,
    max_bytes: usize,
) -> Result<PropertyDefinitionV1, CaptureError> {
    let (value, _): (PropertyDefinitionV1, _) = load_bounded(store, sealer, id, max_bytes).await?;
    validate_schema(store, &value).await?;
    value
        .validate()
        .map_err(|e| format!("Invalid property definition: {e:?}"))?;
    if value.manifest_id().map_err(|e| e.to_string())? != id {
        return Err("Property definition identity mismatch".into());
    }
    Ok(value)
}

/// Deposit a property record against its already-deposited definition.
///
/// # Errors
/// A [`CaptureError`] on a missing definition, an invalid record, a readback
/// mismatch, custody refusal or a store failure.
pub async fn deposit_record(
    store: &mut dyn BoundedRead,
    sealer: &dyn PayloadSealer,
    record: &PropertyRecordV1,
) -> Result<ManifestId, CaptureError> {
    let definition = load_definition(store, sealer, record.definition_manifest).await?;
    record
        .validate(&definition)
        .map_err(|e| format!("Invalid property record: {e:?}"))?;
    validate_schema(store, record).await?;
    let id = sealed::save(store, sealer, record, 0).await?;
    if load_record(store, sealer, id).await?.0 != *record {
        return Err("Property record readback mismatch".into());
    }
    Ok(id)
}

/// Load a sealed property record with its definition.
///
/// # Errors
/// A [`CaptureError`] as [`load_bounded`], or a failed validation.
pub async fn load_record(
    store: &mut dyn BoundedRead,
    sealer: &dyn PayloadSealer,
    id: ManifestId,
) -> Result<(PropertyRecordV1, PropertyDefinitionV1), CaptureError> {
    load_record_bounded(store, sealer, id, usize::MAX, usize::MAX).await
}

/// [`load_record`] under byte ceilings for the record and its validation.
///
/// # Errors
/// A [`CaptureError`] as [`load_bounded`], or a failed validation.
pub async fn load_record_bounded(
    store: &mut dyn BoundedRead,
    sealer: &dyn PayloadSealer,
    id: ManifestId,
    max_bytes: usize,
    max_validation_bytes: usize,
) -> Result<(PropertyRecordV1, PropertyDefinitionV1), CaptureError> {
    let (record, used): (PropertyRecordV1, _) = load_bounded(store, sealer, id, max_bytes).await?;
    validate_schema(store, &record).await?;
    let definition = load_definition_bounded(
        store,
        sealer,
        record.definition_manifest,
        max_bytes
            .checked_sub(used)
            .ok_or("Structured record byte limit exceeded")?
            .min(max_validation_bytes),
    )
    .await?;
    record
        .validate(&definition)
        .map_err(|e| format!("Invalid property record: {e:?}"))?;
    Ok((record, definition))
}
