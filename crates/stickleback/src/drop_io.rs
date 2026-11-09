// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Operation export and staged native-drop import.
//!
//! The codec verifies the entire carrier before this module decodes operations.
//! Import then preflights every operation for structural and domain admission,
//! stages the verified records by semantic drop id, and commits operations,
//! authorized retention effects, stage cleanup, and a receipt in one batch.

use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use muniment::{Backend, StoreError, WriteOp};
use p2panda_core::cbor::{decode_cbor, encode_cbor};
use p2panda_core::{Body, Extensions, Hash, Header, LogId, Operation, Topic, VerifyingKey};
use p2panda_store::topics::TopicStore;
use serde::{Deserialize, Serialize};

use crate::drop::{
    DropId, DropLimits, DropProtector, DropRecord, DropWriteReceipt, NativeDropError,
    read_plain_drop, read_protected_drop, write_plain_drop, write_protected_drop,
};
use crate::store::{VerifiedBytes, pending_payload_key};
use crate::{
    DropReceipt, MunimentStore, OperationPolicy, OperationProcessor, ProcessError, ReceiptPeer,
};

/// Settings-controlled operation export selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DropExportProfile {
    pub include_operation_bodies: bool,
}

impl Default for DropExportProfile {
    fn default() -> Self {
        Self {
            include_operation_bodies: true,
        }
    }
}

/// Domain-owned decision for one retained operation.
///
/// The domain maps its privacy classes and profile settings into this small
/// carrier-neutral vocabulary. Higher priorities are selected first.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DropExportDecision {
    Omit,
    Header { priority: u32 },
    Full { priority: u32 },
}

/// Privacy and priority authority for one export.
pub trait DropExportSelector<E: Extensions> {
    fn select(&self, operation: &Operation<E>) -> DropExportDecision;
}

impl<E: Extensions> DropExportSelector<E> for DropExportProfile {
    fn select(&self, _operation: &Operation<E>) -> DropExportDecision {
        if self.include_operation_bodies {
            DropExportDecision::Full { priority: 0 }
        } else {
            DropExportDecision::Header { priority: 0 }
        }
    }
}

/// Settings-controlled semantic record budget.
///
/// This counts canonical record bytes. Carrier framing and protection overhead
/// remain subject to [`DropLimits`] and the selected transport's own limit.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DropExportBudget {
    pub max_selected_record_bytes: Option<u64>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DropExportStats {
    pub selected_records: u64,
    pub selected_record_bytes: u64,
    pub policy_omitted: u64,
    pub budget_omitted: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DropFileExportReport {
    pub drop: DropWriteReceipt,
    pub selection: DropExportStats,
}

/// Per-drop operation import counts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DropImportReport {
    pub drop_id: DropId,
    pub accepted: u64,
    pub duplicate: u64,
    pub hydrated_payloads: u64,
    pub non_operation_records: u64,
    pub receipt_hit: bool,
}

/// One verified drop corpus left staged after a continuity or storage failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StagedDrop {
    pub drop_id: DropId,
    pub record_count: u64,
}

static NEXT_EXPORT_TEMP_ID: AtomicU64 = AtomicU64::new(0);

// Keep the local marker schema stable. The semantic DropId remains in its key;
// the portable peer statement adds that id only when it leaves this device.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
struct StoredDropReceipt {
    operation_records: u64,
    non_operation_records: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum DropIoError {
    #[error(transparent)]
    Drop(#[from] NativeDropError),
    #[error(transparent)]
    Process(#[from] ProcessError),
    #[error("drop operation codec: {0}")]
    OperationCodec(String),
    #[error("drop staging codec: {0}")]
    StagingCodec(String),
    #[error("native drop {0} is not staged")]
    MissingStage(String),
    #[error("native drop staging metadata does not match its namespace")]
    FalseStage,
    #[error("drop {kind} chunks for {digest} contain a gap or overlap at offset {offset}")]
    ChunkLayout {
        kind: &'static str,
        digest: String,
        offset: u64,
    },
    #[error("drop {kind} chunks for {digest} have a false digest")]
    FalseChunkDigest { kind: &'static str, digest: String },
    #[error("drop export store: {0}")]
    Store(#[from] StoreError),
    #[error("drop export record codec: {0}")]
    ExportCodec(String),
    #[error("drop export selected-byte count overflowed")]
    ExportSizeOverflow,
    #[error(transparent)]
    Receipt(#[from] crate::DropReceiptError),
}

#[derive(Default)]
struct AssembledContent {
    payloads: BTreeMap<[u8; 32], VerifiedBytes>,
    blobs: BTreeMap<[u8; 32], VerifiedBytes>,
}

/// Encode one p2panda operation as a native-drop record.
pub fn operation_record<E: Extensions>(operation: &Operation<E>, include_body: bool) -> DropRecord {
    DropRecord::Operation {
        header: operation.header.encode(),
        inline_body: include_body
            .then(|| operation.body.as_ref().map(|body| body.to_bytes()))
            .flatten(),
    }
}

/// Decode one operation record while retaining header-only carriage.
pub fn decode_operation_record<E: Extensions>(
    record: &DropRecord,
) -> Result<Option<Operation<E>>, DropIoError> {
    let DropRecord::Operation {
        header,
        inline_body,
    } = record
    else {
        return Ok(None);
    };
    let decoded: Header<E> = Header::decode(&header[..])
        .map_err(|error| DropIoError::OperationCodec(error.to_string()))?;
    if decoded.encode() != *header {
        return Err(DropIoError::OperationCodec(
            "operation header is not canonically encoded".into(),
        ));
    }
    let hash = decoded.hash();
    Ok(Some(Operation {
        hash,
        header: decoded,
        body: inline_body.clone().map(Body::from),
    }))
}

/// Select every operation under one topic using the domain's configured body
/// policy. Retained-frontier filtering has already happened in the store.
pub async fn export_topic_operations<B, E, L>(
    store: &MunimentStore<B, E>,
    topic: &Topic,
    profile: DropExportProfile,
) -> Result<Vec<DropRecord>, DropIoError>
where
    B: Backend,
    E: Extensions,
    L: LogId,
{
    let (records, _) = export_topic_operations_selected::<B, E, L, _>(
        store,
        topic,
        &profile,
        DropExportBudget::default(),
    )
    .await?;
    Ok(records)
}

/// Apply domain privacy and priority decisions, then fit selected records into
/// the configured semantic byte budget.
pub async fn export_topic_operations_selected<B, E, L, S>(
    store: &MunimentStore<B, E>,
    topic: &Topic,
    selector: &S,
    budget: DropExportBudget,
) -> Result<(Vec<DropRecord>, DropExportStats), DropIoError>
where
    B: Backend,
    E: Extensions,
    L: LogId,
    S: DropExportSelector<E>,
{
    let logs: BTreeMap<VerifyingKey, Vec<L>> = store.resolve(topic).await?;
    let mut candidates = Vec::new();
    let mut stats = DropExportStats::default();
    let mut ordinal = 0u64;
    for (author, log_ids) in logs {
        for log_id in log_ids {
            if let Some(entries) = store.get_log_entries(&author, &log_id, None, None).await? {
                for (operation, _) in entries {
                    let decision = selector.select(&operation);
                    let (include_body, priority) = match decision {
                        DropExportDecision::Omit => {
                            stats.policy_omitted += 1;
                            ordinal += 1;
                            continue;
                        }
                        DropExportDecision::Header { priority } => (false, priority),
                        DropExportDecision::Full { priority } => (true, priority),
                    };
                    let record = operation_record(&operation, include_body);
                    let bytes = encode_cbor(&record)
                        .map_err(|error| DropIoError::ExportCodec(error.to_string()))?
                        .len() as u64;
                    candidates.push((Reverse(priority), ordinal, bytes, record));
                    ordinal += 1;
                }
            }
        }
    }
    candidates.sort_by_key(|(priority, ordinal, _, _)| (*priority, *ordinal));

    let mut records = Vec::new();
    for (_, _, bytes, record) in candidates {
        let next = stats
            .selected_record_bytes
            .checked_add(bytes)
            .ok_or(DropIoError::ExportSizeOverflow)?;
        if budget
            .max_selected_record_bytes
            .is_some_and(|maximum| next > maximum)
        {
            stats.budget_omitted += 1;
            continue;
        }
        stats.selected_record_bytes = next;
        stats.selected_records += 1;
        records.push(record);
    }
    Ok((records, stats))
}

/// Verify a complete plaintext drop, preflight every operation, then apply the
/// corpus through the same processor used by local authoring and LogSync.
pub async fn import_plain_drop<B, E, P, R>(
    reader: R,
    limits: DropLimits,
    processor: &OperationProcessor<B, E, P>,
) -> Result<DropImportReport, DropIoError>
where
    B: Backend,
    E: Extensions,
    P: OperationPolicy<E>,
    R: Read,
{
    let (read, records) = read_plain_drop(reader, limits)?;
    import_drop_records(read.id, records, processor).await
}

/// Import a plaintext/public drop file through the shared operation processor.
pub async fn import_plain_drop_file<B, E, P>(
    path: impl AsRef<Path>,
    limits: DropLimits,
    processor: &OperationProcessor<B, E, P>,
) -> Result<DropImportReport, DropIoError>
where
    B: Backend,
    E: Extensions,
    P: OperationPolicy<E>,
{
    let file = File::open(path).map_err(NativeDropError::Io)?;
    import_plain_drop(BufReader::new(file), limits, processor).await
}

/// Protected counterpart of [`import_plain_drop`].
pub async fn import_protected_drop<B, E, P, R, D>(
    reader: R,
    limits: DropLimits,
    processor: &OperationProcessor<B, E, P>,
    protector: &D,
) -> Result<DropImportReport, DropIoError>
where
    B: Backend,
    E: Extensions,
    P: OperationPolicy<E>,
    R: Read,
    D: DropProtector,
{
    let (read, records) = read_protected_drop(reader, limits, protector)?;
    import_drop_records(read.id, records, processor).await
}

/// Import a protected drop file through its suite and the shared processor.
pub async fn import_protected_drop_file<B, E, P, D>(
    path: impl AsRef<Path>,
    limits: DropLimits,
    processor: &OperationProcessor<B, E, P>,
    protector: &D,
) -> Result<DropImportReport, DropIoError>
where
    B: Backend,
    E: Extensions,
    P: OperationPolicy<E>,
    D: DropProtector,
{
    let file = File::open(path).map_err(NativeDropError::Io)?;
    import_protected_drop(BufReader::new(file), limits, processor, protector).await
}

/// Import already integrity-verified native-drop records through the shared
/// processor. Aggregate domains use this after admitting their own critical
/// evidence records in dependency order.
pub async fn import_drop_records<B, E, P>(
    drop_id: DropId,
    records: Vec<DropRecord>,
    processor: &OperationProcessor<B, E, P>,
) -> Result<DropImportReport, DropIoError>
where
    B: Backend,
    E: Extensions,
    P: OperationPolicy<E>,
{
    let receipt_key = receipt_key(drop_id);
    if let Some(bytes) = processor.store().backend().get(&receipt_key).await? {
        let receipt: StoredDropReceipt = decode_cbor(&bytes[..])
            .map_err(|error| DropIoError::StagingCodec(error.to_string()))?;
        discard_staged_drop(drop_id, processor.store()).await?;
        return Ok(DropImportReport {
            drop_id,
            accepted: 0,
            duplicate: receipt.operation_records,
            hydrated_payloads: 0,
            non_operation_records: receipt.non_operation_records,
            receipt_hit: true,
        });
    }

    let content = assemble_content(&records)?;
    let mut operations = Vec::new();
    let mut non_operation_records = 0;
    for record in &records {
        match decode_operation_record(record)? {
            Some(operation) => operations.push(operation),
            None => non_operation_records += 1,
        }
    }

    let mut consumed_payloads = BTreeSet::new();
    let mut consumed_pending = BTreeSet::new();
    for operation in &mut operations {
        let Some(payload_hash) = operation.header.payload_hash else {
            continue;
        };
        let digest = *payload_hash.as_bytes();
        if operation.body.is_some() {
            if content.payloads.contains_key(&digest) {
                consumed_payloads.insert(digest);
            }
            continue;
        }
        if let Some(bytes) = content.payloads.get(&digest) {
            operation.body = Some(Body::from(bytes.as_slice().to_vec()));
            consumed_payloads.insert(digest);
            continue;
        }
        if let Some(bytes) = processor.store().pending_payload(&payload_hash).await? {
            if Hash::digest(&bytes) != payload_hash {
                return Err(DropIoError::FalseChunkDigest {
                    kind: "payload",
                    digest: payload_hash.to_hex(),
                });
            }
            operation.body = Some(Body::from(bytes));
            consumed_pending.insert(digest);
        }
    }

    // Authorization and structural validity are all-or-nothing and happen
    // before durable staging.
    for operation in &operations {
        processor.preflight(operation)?;
    }

    let mut leading_writes = Vec::new();
    for digest in consumed_pending {
        let payload_hash = Hash::from_bytes(digest);
        leading_writes.push(WriteOp::Delete {
            key: pending_payload_key(&payload_hash),
        });
    }
    // Both maps are consumed here rather than borrowed: the assembled bytes have
    // no further reader, so a retained payload moves into its pending write and a
    // blob moves into its store write instead of being copied one last time.
    for (digest, bytes) in content.payloads {
        let payload_hash = Hash::from_bytes(digest);
        let (writes, attached) = processor
            .store()
            .verified_payload_attachment_writes(&payload_hash, &bytes)
            .await?;
        leading_writes.extend(writes);
        let pending_key = pending_payload_key(&payload_hash);
        if consumed_payloads.contains(&digest) || attached > 0 {
            leading_writes.push(WriteOp::Delete { key: pending_key });
        } else {
            leading_writes.push(WriteOp::Put {
                key: pending_key,
                value: bytes.into_inner(),
            });
        }
    }
    for (digest, bytes) in content.blobs {
        leading_writes.push(MunimentStore::<B, E>::imported_blob_write(digest, bytes));
    }

    let stage_keys = stage_records(drop_id, &records, processor.store().backend()).await?;
    let stored_receipt = StoredDropReceipt {
        operation_records: operations.len() as u64,
        non_operation_records,
    };
    let mut final_writes: Vec<_> = stage_keys
        .into_iter()
        .map(|key| WriteOp::Delete { key })
        .collect();
    final_writes.push(WriteOp::Put {
        key: receipt_key,
        value: encode_cbor(&stored_receipt)
            .map_err(|error| DropIoError::StagingCodec(error.to_string()))?,
    });
    let outcome = processor
        .process_batch_atomic(&operations, &leading_writes, &final_writes)
        .await?;
    Ok(DropImportReport {
        drop_id,
        accepted: outcome.inserted,
        duplicate: outcome.duplicate,
        hydrated_payloads: outcome.hydrated_payloads,
        non_operation_records,
        receipt_hit: false,
    })
}

fn drop_hex(drop_id: DropId) -> String {
    hex::encode(drop_id.0)
}

fn stage_prefix(drop_id: DropId) -> String {
    format!("drop-stage/{}/", drop_hex(drop_id))
}

fn stage_meta_key(drop_id: DropId) -> String {
    format!("{}meta", stage_prefix(drop_id))
}

#[cfg(test)]
#[path = "drop_io/tests.rs"]
mod tests;

#[path = "drop_io/staging.rs"]
mod staging;

pub use staging::{discard_staged_drop, list_staged_drops, resume_staged_drop};

use staging::{assemble_content, stage_records};

#[path = "drop_io/receipts.rs"]
mod receipts;

pub use receipts::{
    discard_peer_drop_receipts, local_drop_receipt, peer_drop_receipt, store_peer_drop_receipt,
};

use receipts::receipt_key;

#[path = "drop_io/files.rs"]
mod files;

#[cfg(test)]
use files::write_staged_drop;

pub use files::{
    export_plain_topic_file, export_protected_topic_file, export_selected_plain_topic_file,
};
