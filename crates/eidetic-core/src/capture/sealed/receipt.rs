// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The sealed request receipt: an integrity-bound envelope per capture request.

use super::RECEIPT_PREFIX;
use crate::PayloadSealer;
use crate::capture::{CaptureError, DepositReceipt};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReceiptEnvelope {
    pub hash: crate::Hash,
    pub marker: crate::SealedBlobRef,
    pub ciphertext: Vec<u8>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BoundReceipt {
    pub namespace: String,
    pub receipt: DepositReceipt,
}

pub(crate) fn key(id: uuid::Uuid) -> String {
    format!("{RECEIPT_PREFIX}{id}")
}

pub(crate) async fn put_receipt(
    store: &mut dyn crate::Store,
    sealer: &dyn PayloadSealer,
    receipt: &DepositReceipt,
) -> Result<(), CaptureError> {
    let k = key(receipt.request.id);
    let clear = serde_json::to_vec(&BoundReceipt {
        namespace: k.clone(),
        receipt: receipt.clone(),
    })
    .map_err(|_| "sealed capture receipt encoding")?;
    let hash = crate::Hash::of(&clear);
    let (ciphertext, marker) = sealer
        .seal(&hash, &clear)
        .map_err(|_| "sealed capture custody refused")?;
    let value = serde_json::to_vec(&ReceiptEnvelope {
        hash,
        marker,
        ciphertext,
    })
    .map_err(|_| "sealed capture envelope encoding")?;
    store
        .put(&k, &value)
        .await
        .map_err(|error| CaptureError::from(error).context("sealed capture receipt write"))
}

pub(crate) async fn read_receipt(
    store: &mut dyn crate::Store,
    sealer: &dyn PayloadSealer,
    k: &str,
) -> Result<Option<DepositReceipt>, CaptureError> {
    let Some(bytes) = store
        .get(k)
        .await
        .map_err(|error| CaptureError::from(error).context("sealed capture receipt read"))?
    else {
        return Ok(None);
    };
    let envelope: ReceiptEnvelope =
        serde_json::from_slice(&bytes).map_err(|_| "sealed capture receipt envelope invalid")?;
    let clear = sealer
        .unseal(&envelope.hash, &envelope.marker, &envelope.ciphertext)
        .map_err(|_| "sealed capture custody refused")?;
    if crate::Hash::of(&clear) != envelope.hash {
        return Err("sealed capture receipt digest mismatch".into());
    }
    let held: BoundReceipt =
        serde_json::from_slice(&clear).map_err(|_| "sealed capture receipt invalid")?;
    if held.namespace != k
        || key(held.receipt.request.id) != k
        || held.receipt.request.source.is_none()
        || (held.receipt.saved && held.receipt.stored.is_none())
    {
        return Err("sealed capture receipt owner mismatch".into());
    }
    Ok(Some(held.receipt))
}

/// Every sealed request receipt, in key order.
///
/// # Errors
/// A [`CaptureError`] when the inventory or any receipt cannot be read or
/// unsealed (a locked custody refuses the whole listing).
pub async fn receipts(
    store: &mut dyn crate::Store,
    sealer: &dyn PayloadSealer,
) -> Result<Vec<DepositReceipt>, CaptureError> {
    let mut keys = store.list(RECEIPT_PREFIX).await.map_err(|error| {
        CaptureError::from(error).context("sealed capture inventory unavailable")
    })?;
    keys.sort();
    let mut out = Vec::with_capacity(keys.len());
    for key in keys {
        out.push(
            read_receipt(store, sealer, &key)
                .await?
                .ok_or_else(|| CaptureError::from("sealed capture receipt disappeared"))?,
        );
    }
    Ok(out)
}
