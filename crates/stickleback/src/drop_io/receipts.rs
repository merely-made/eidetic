// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Read this device's durable completion receipt for peer exchange.
///
/// Receiving peers must keep exchanged receipts scoped to the authenticated
/// peer. They must not write them into the local `drop-receipt/` namespace.
pub async fn local_drop_receipt<B, E>(
    drop_id: DropId,
    store: &MunimentStore<B, E>,
) -> Result<Option<DropReceipt>, DropIoError>
where
    B: Backend,
    E: Extensions,
{
    let Some(bytes) = store.backend().get(&receipt_key(drop_id)).await? else {
        return Ok(None);
    };
    let stored: StoredDropReceipt =
        decode_cbor(&bytes[..]).map_err(|error| DropIoError::StagingCodec(error.to_string()))?;
    Ok(Some(DropReceipt::new(
        drop_id,
        stored.operation_records,
        stored.non_operation_records,
    )))
}

/// Remember an advisory receipt from one authenticated peer.
///
/// This namespace is deliberately separate from local import receipts. A peer
/// claim can suppress redundant sends to that same peer, but cannot make an
/// import local, authorize retention, or bypass the operation processor.
pub async fn store_peer_drop_receipt<B, E>(
    peer: ReceiptPeer,
    receipt: DropReceipt,
    store: &MunimentStore<B, E>,
) -> Result<(), DropIoError>
where
    B: Backend,
    E: Extensions,
{
    store
        .backend()
        .apply(&[WriteOp::Put {
            key: peer_receipt_key(peer, receipt.drop_id),
            value: receipt.to_canonical_bytes()?,
        }])
        .await?;
    Ok(())
}

/// Read one peer-scoped advisory receipt for resend coordination.
pub async fn peer_drop_receipt<B, E>(
    peer: ReceiptPeer,
    drop_id: DropId,
    store: &MunimentStore<B, E>,
) -> Result<Option<DropReceipt>, DropIoError>
where
    B: Backend,
    E: Extensions,
{
    let Some(bytes) = store
        .backend()
        .get(&peer_receipt_key(peer, drop_id))
        .await?
    else {
        return Ok(None);
    };
    let receipt = DropReceipt::from_canonical_bytes(&bytes)?;
    if receipt.drop_id != drop_id {
        return Err(DropIoError::StagingCodec(
            "peer drop receipt does not match its storage namespace".into(),
        ));
    }
    Ok(Some(receipt))
}

/// Remove every advisory receipt for one peer according to caller retention
/// settings or identity lifecycle.
pub async fn discard_peer_drop_receipts<B, E>(
    peer: ReceiptPeer,
    store: &MunimentStore<B, E>,
) -> Result<u64, DropIoError>
where
    B: Backend,
    E: Extensions,
{
    let keys = store.backend().list(&peer_receipt_prefix(peer)).await?;
    let writes: Vec<_> = keys
        .iter()
        .cloned()
        .map(|key| WriteOp::Delete { key })
        .collect();
    store.backend().apply(&writes).await?;
    Ok(keys.len() as u64)
}

pub(super) fn receipt_key(drop_id: DropId) -> String {
    format!("drop-receipt/{}", drop_hex(drop_id))
}

pub(super) fn peer_receipt_prefix(peer: ReceiptPeer) -> String {
    format!("drop-peer-receipt/{}/", hex::encode(peer.0))
}

pub(super) fn peer_receipt_key(peer: ReceiptPeer, drop_id: DropId) -> String {
    format!("{}{}", peer_receipt_prefix(peer), drop_hex(drop_id))
}
