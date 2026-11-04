// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// List verified corpora awaiting retry or caller-directed disposal.
pub async fn list_staged_drops<B, E>(
    store: &MunimentStore<B, E>,
) -> Result<Vec<StagedDrop>, DropIoError>
where
    B: Backend,
    E: Extensions,
{
    let mut staged = Vec::new();
    for key in store.backend().list("drop-stage/").await? {
        if !key.ends_with("/meta") {
            continue;
        }
        let Some(bytes) = store.backend().get(&key).await? else {
            continue;
        };
        let (drop_id, record_count): (DropId, u64) = decode_cbor(&bytes[..])
            .map_err(|error| DropIoError::StagingCodec(error.to_string()))?;
        if key != stage_meta_key(drop_id) {
            return Err(DropIoError::FalseStage);
        }
        staged.push(StagedDrop {
            drop_id,
            record_count,
        });
    }
    staged.sort_by_key(|stage| stage.drop_id.0);
    Ok(staged)
}

/// Retry one verified staged corpus through current domain policy and frontier
/// state.
pub async fn resume_staged_drop<B, E, P>(
    drop_id: DropId,
    limits: DropLimits,
    processor: &OperationProcessor<B, E, P>,
) -> Result<DropImportReport, DropIoError>
where
    B: Backend,
    E: Extensions,
    P: OperationPolicy<E>,
{
    let meta_key = stage_meta_key(drop_id);
    let Some(meta) = processor.store().backend().get(&meta_key).await? else {
        return Err(DropIoError::MissingStage(drop_hex(drop_id)));
    };
    let (stored_id, record_count): (DropId, u64) =
        decode_cbor(&meta[..]).map_err(|error| DropIoError::StagingCodec(error.to_string()))?;
    if stored_id != drop_id {
        return Err(DropIoError::FalseStage);
    }
    if record_count > limits.max_records {
        return Err(NativeDropError::Limit {
            limit: "record count",
            actual: record_count,
            maximum: limits.max_records,
        }
        .into());
    }
    let mut records = Vec::new();
    let prefix = stage_prefix(drop_id);
    for index in 0..record_count {
        let key = format!("{prefix}{index:016x}");
        let Some(bytes) = processor.store().backend().get(&key).await? else {
            return Err(DropIoError::FalseStage);
        };
        if bytes.len() as u64 > limits.max_record_bytes {
            return Err(NativeDropError::Limit {
                limit: "record bytes",
                actual: bytes.len() as u64,
                maximum: limits.max_record_bytes,
            }
            .into());
        }
        records.push(
            decode_cbor(&bytes[..])
                .map_err(|error| DropIoError::StagingCodec(error.to_string()))?,
        );
    }
    import_drop_records(drop_id, records, processor).await
}

/// Remove a staged corpus selected by caller policy or settings.
pub async fn discard_staged_drop<B, E>(
    drop_id: DropId,
    store: &MunimentStore<B, E>,
) -> Result<u64, DropIoError>
where
    B: Backend,
    E: Extensions,
{
    let keys = store.backend().list(&stage_prefix(drop_id)).await?;
    let writes: Vec<_> = keys
        .iter()
        .cloned()
        .map(|key| WriteOp::Delete { key })
        .collect();
    store.backend().apply(&writes).await?;
    Ok(keys.len() as u64)
}

pub(super) fn assemble_content(records: &[DropRecord]) -> Result<AssembledContent, DropIoError> {
    let mut payload_chunks: BTreeMap<[u8; 32], Vec<(u64, &[u8])>> = BTreeMap::new();
    let mut blob_chunks: BTreeMap<[u8; 32], Vec<(u64, &[u8])>> = BTreeMap::new();
    for record in records {
        match record {
            DropRecord::PayloadChunk {
                payload_hash,
                offset,
                bytes,
            } => payload_chunks
                .entry(*payload_hash)
                .or_default()
                .push((*offset, bytes)),
            DropRecord::BlobChunk {
                blob_hash,
                offset,
                bytes,
            } => blob_chunks
                .entry(*blob_hash)
                .or_default()
                .push((*offset, bytes)),
            _ => {}
        }
    }
    let payloads = payload_chunks
        .into_iter()
        .map(|(digest, chunks)| {
            assemble_chunks("payload", digest, chunks).map(|bytes| (digest, bytes))
        })
        .collect::<Result<_, _>>()?;
    let blobs = blob_chunks
        .into_iter()
        .map(|(digest, chunks)| {
            assemble_chunks("blob", digest, chunks).map(|bytes| (digest, bytes))
        })
        .collect::<Result<_, _>>()?;
    Ok(AssembledContent { payloads, blobs })
}

pub(super) fn assemble_chunks(
    kind: &'static str,
    digest: [u8; 32],
    mut chunks: Vec<(u64, &[u8])>,
) -> Result<VerifiedBytes, DropIoError> {
    chunks.sort_by_key(|(offset, _)| *offset);
    let mut bytes = Vec::new();
    let mut expected = 0u64;
    for (offset, chunk) in chunks {
        if offset != expected {
            return Err(DropIoError::ChunkLayout {
                kind,
                digest: hex::encode(digest),
                offset,
            });
        }
        bytes.extend_from_slice(chunk);
        expected = expected
            .checked_add(chunk.len() as u64)
            .ok_or(DropIoError::ChunkLayout {
                kind,
                digest: hex::encode(digest),
                offset,
            })?;
    }
    if *blake3::hash(&bytes).as_bytes() != digest {
        return Err(DropIoError::FalseChunkDigest {
            kind,
            digest: hex::encode(digest),
        });
    }
    // This is the one hash of the assembled buffer; the store trusts it rather
    // than walking these (often multi-megabyte) bytes again on the way in.
    Ok(VerifiedBytes::assume_verified(bytes))
}

pub(super) async fn stage_records<B: Backend>(
    drop_id: DropId,
    records: &[DropRecord],
    backend: &B,
) -> Result<Vec<String>, DropIoError> {
    let prefix = stage_prefix(drop_id);
    let old_keys = backend.list(&prefix).await?;
    let mut writes: Vec<_> = old_keys
        .into_iter()
        .map(|key| WriteOp::Delete { key })
        .collect();
    let meta_key = stage_meta_key(drop_id);
    writes.push(WriteOp::Put {
        key: meta_key.clone(),
        value: encode_cbor(&(drop_id, records.len() as u64))
            .map_err(|error| DropIoError::StagingCodec(error.to_string()))?,
    });
    let mut staged_keys = vec![meta_key];
    for (index, record) in records.iter().enumerate() {
        let key = format!("{prefix}{index:016x}");
        writes.push(WriteOp::Put {
            key: key.clone(),
            value: encode_cbor(record)
                .map_err(|error| DropIoError::StagingCodec(error.to_string()))?,
        });
        staged_keys.push(key);
    }
    backend.apply(&writes).await?;
    Ok(staged_keys)
}
