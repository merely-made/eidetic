// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Frozen research results in the sealed owner.
//!
//! A result deposits only if replaying its frozen inputs reproduces it, and
//! reads again check the replay against the stored rows under the reader's
//! current limits. The sealed save, the bounded load and the schema checks are
//! `eidetic::capture`'s; only the evaluator is this crate's.

use super::{RecordResult, RecordSettings, evaluate};
use eidetic::capture::{BoundedRead, CaptureError, load_bounded, sealed, validate_schema};
use eidetic::{ManifestId, PayloadSealer};

/// Deposit a result after replaying its frozen inputs.
///
/// # Errors
/// A [`CaptureError`] when the schema is missing, the inputs are refused, the
/// replay differs from the result, custody refuses, or readback mismatches.
pub async fn deposit_result(
    store: &mut dyn BoundedRead,
    sealer: &dyn PayloadSealer,
    result: &RecordResult,
) -> Result<ManifestId, CaptureError> {
    eidetic::records::bootstrap_record_schemas(store).await?;
    validate_schema(store, result).await?;
    let replay = evaluate(&result.snapshot, &result.query, &result.settings)
        .map_err(|e| format!("Saved result input refused: {e:?}"))?;
    if replay != *result {
        return Err("Saved result does not match its frozen inputs".into());
    }
    let id = sealed::save(store, sealer, result, 0).await?;
    if load_result(store, sealer, id).await? != *result {
        return Err("Saved result readback mismatch".into());
    }
    Ok(id)
}

/// Load a sealed result and re-check its replay.
///
/// # Errors
/// A [`CaptureError`] as [`load_result_bounded`] with no ceiling.
pub async fn load_result(
    store: &mut dyn BoundedRead,
    sealer: &dyn PayloadSealer,
    id: ManifestId,
) -> Result<RecordResult, CaptureError> {
    load_result_bounded(store, sealer, id, usize::MAX, None).await
}

/// [`load_result`] under a byte ceiling, replaying under the tighter of the
/// result's own settings and the reader's `current` ones.
///
/// # Errors
/// A [`CaptureError`] when the load is refused, or the replay's operation
/// version or rows differ from the stored result.
pub async fn load_result_bounded(
    store: &mut dyn BoundedRead,
    sealer: &dyn PayloadSealer,
    id: ManifestId,
    max_bytes: usize,
    current: Option<&RecordSettings>,
) -> Result<RecordResult, CaptureError> {
    let (result, _): (RecordResult, _) = load_bounded(store, sealer, id, max_bytes).await?;
    validate_schema(store, &result).await?;
    let mut effective = result.settings.clone();
    if let Some(current) = current {
        effective.max_sources = effective.max_sources.min(current.max_sources);
        effective.max_records = effective.max_records.min(current.max_records);
        effective.max_work = effective.max_work.min(current.max_work);
        effective.max_validation_bytes = effective
            .max_validation_bytes
            .min(current.max_validation_bytes);
        effective.max_rows = effective.max_rows.min(current.max_rows);
        effective.max_input_bytes = effective.max_input_bytes.min(current.max_input_bytes);
        effective.max_output_bytes = effective.max_output_bytes.min(current.max_output_bytes);
    }
    let replay = evaluate(&result.snapshot, &result.query, &effective)
        .map_err(|e| format!("Saved result input refused: {e:?}"))?;
    if replay.operation_version != result.operation_version || replay.rows != result.rows {
        return Err("Saved result does not match its frozen inputs".into());
    }
    Ok(result)
}
