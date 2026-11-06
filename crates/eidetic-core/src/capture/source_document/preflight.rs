// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Preserve an existing typed role exactly.

use crate::capture::CaptureError;

/// Check that anything already at `id` is the exact artifact a capture would
/// write. Returns whether a manifest exists. The caller serializes these
/// checks with the writes that follow; no transactional backend guarantee is
/// assumed.
///
/// # Errors
/// A [`CaptureError`] on an incompatible manifest role, a payload that fails
/// its address or differs from `expected`, or an inconsistent local payload.
pub async fn preflight_source_artifact(
    store: &mut dyn crate::Store,
    id: crate::ManifestId,
    schema: crate::SchemaRef,
    privacy: crate::PrivacyClass,
    expected: Option<&[u8]>,
) -> Result<bool, CaptureError> {
    let manifest = crate::manifest::load_manifest(store, id)
        .await
        .map_err(|error| CaptureError::from(error).context("source artifact preflight manifest"))?;
    if let Some(manifest) = &manifest
        && (manifest.id != id
            || manifest.content_hash != id.0
            || manifest.schema != schema
            || manifest.privacy != privacy)
    {
        return Err(format!(
            "source artifact preflight: incompatible existing manifest role at {id}"
        )
        .into());
    }
    let key = format!("blob:{}", id.0.to_hex());
    let bytes = store
        .get(&key)
        .await
        .map_err(|error| CaptureError::from(error).context("source artifact preflight payload"))?;
    if let Some(bytes) = &bytes
        && (crate::Hash::of(bytes) != id.0 || expected.is_some_and(|expected| expected != bytes))
    {
        return Err(
            format!("source artifact preflight: payload integrity mismatch at {id}").into(),
        );
    }
    if let Some(manifest) = &manifest {
        let bytes = bytes
            .as_ref()
            .ok_or_else(|| format!("source artifact preflight: missing local payload at {id}"))?;
        if manifest.byte_size != bytes.len() as u64
            || !manifest.sources.iter().any(
                |source| matches!(source, crate::BlobSource::Local { key: local } if local == &key),
            )
        {
            return Err(
                format!("source artifact preflight: inconsistent local payload at {id}").into(),
            );
        }
    }
    Ok(manifest.is_some())
}
