// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Inherent twins of the id-keyed [`OperationStore`] methods.
///
/// Those methods key by operation hash, never by log id, yet their trait
/// signatures still carry the `L` type parameter, so a bare `store.get_operation(id)`
/// can't infer `L`. These same-name inherent methods shadow the trait ones for
/// direct calls (inherent resolution wins), so callers and the trait impl below
/// both reach them without pinning a log-id type.
impl<B, E> MunimentStore<B, E>
where
    B: Backend,
    E: Extensions,
{
    /// Atomically associate an author's log with a topic and insert one operation.
    ///
    /// The topic association, log entry, and hash pointer reach the backend in
    /// one [`Backend::apply`] batch. Domain stores should prefer this helper over
    /// manually composing [`TopicStore::associate`] and
    /// [`OperationStore::insert_operation`].
    pub async fn insert_indexed_operation<L: LogId>(
        &self,
        topic: &Topic,
        operation: &Operation<E>,
        log_id: &L,
    ) -> Result<bool, StoreError> {
        let (inserted, _) = self
            .insert_indexed_operation_with_prune(topic, operation, log_id, false)
            .await?;
        Ok(inserted)
    }

    /// Atomically index several admitted operations and caller-supplied trailing
    /// writes.
    ///
    /// The processor performs policy and continuity checks first. This method
    /// rechecks that every operation is still absent, then submits all topic,
    /// log, pointer, staging-cleanup, and receipt writes in one backend batch.
    pub(crate) async fn apply_indexed_operation_batch<L: LogId>(
        &self,
        operations: &[IndexedOperation<'_, E, L>],
        leading_writes: &[WriteOp],
        trailing_writes: &[WriteOp],
    ) -> Result<(), StoreError> {
        let mut pointers = BTreeSet::new();
        let mut plan = BatchPlan::<E>::default();
        for write in leading_writes {
            plan.record(write.clone());
        }
        for entry in operations {
            let pointer = op_ptr(&entry.operation.hash);
            if !pointers.insert(pointer.clone()) {
                return Err(codec("atomic operation batch contains a duplicate id"));
            }
            if self.backend.get(&pointer).await?.is_some() {
                return Err(codec(
                    "operation store changed after atomic batch validation",
                ));
            }
            let topic = topic_key(
                entry.topic,
                &entry.operation.header.verifying_key,
                entry.log_id,
            )?;
            let prefix = log_prefix(&entry.operation.header.verifying_key, entry.log_id)?;
            let log_key = format!("{prefix}{:016x}", entry.operation.header.seq_num);

            if entry.prune_before_current {
                self.plan_prune_writes(&mut plan, entry, &prefix).await?;
            }
            for id in entry.erase_payloads {
                self.plan_payload_erasure(&mut plan, id).await?;
            }
            plan.plan_operation(entry, topic, prefix, log_key, pointer)?;
        }
        for write in trailing_writes {
            plan.record(write.clone());
        }
        let writes = plan.into_writes();
        self.backend.apply(&writes).await
    }

    /// Plan the removal of one log's entries below `entry`'s sequence number:
    /// those already durable, and those an earlier entry of this same batch
    /// planned but has not written yet.
    pub(super) async fn plan_prune_writes<L: LogId>(
        &self,
        plan: &mut BatchPlan<E>,
        entry: &IndexedOperation<'_, E, L>,
        prefix: &str,
    ) -> Result<u64, StoreError> {
        let (live_prune, pruned_entries) = self
            .prefix_prune_writes(
                &entry.operation.header.verifying_key,
                entry.log_id,
                entry.operation.header.seq_num,
            )
            .await?;
        for write in live_prune {
            plan.record(write);
        }
        plan.prune_planned(prefix, entry.operation.header.seq_num);
        Ok(pruned_entries)
    }

    /// Plan the erasure of one operation's payload, keeping its signed header.
    ///
    /// This reads through the batch's own pending writes before the backend, so
    /// an operation this batch is still assembling is erased in place rather
    /// than read back stale. That overlay is why the single-operation path in
    /// [`Self::insert_indexed_operation_with_effects`] uses this same overlay,
    /// so pruning and erasure compose with identical last-write semantics.
    pub(super) async fn plan_payload_erasure(
        &self,
        plan: &mut BatchPlan<E>,
        id: &Hash,
    ) -> Result<bool, StoreError> {
        let id_hex = id.to_hex();
        if let Some(operation) = plan.planned.get_mut(&id_hex) {
            if let Some(payload_ref) = operation.payload_ref.as_ref() {
                plan.writes.insert(payload_ref.clone(), None);
            }
            if operation.has_body {
                plan.writes.insert(
                    operation.log_key.clone(),
                    Some(encode_blob(&operation.header, None)?),
                );
                operation.has_body = false;
                return Ok(true);
            }
            return Ok(false);
        }
        let payload_pointer = op_ptr(id);
        if matches!(plan.writes.get(&payload_pointer), Some(None)) {
            return Ok(false);
        }
        let Some(payload_log_key) = self.log_key_for(id).await? else {
            return Ok(false);
        };
        let payload_blob = match plan.writes.get(&payload_log_key) {
            Some(Some(bytes)) => Some(bytes.clone()),
            Some(None) => None,
            None => self.backend.get(&payload_log_key).await?,
        };
        let Some(payload_blob) = payload_blob else {
            return Ok(false);
        };
        let (header, body) = decode_blob::<E>(&payload_blob[..])?;
        if let Some(payload_hash) = header.payload_hash.as_ref() {
            plan.writes.insert(payload_ref_key(payload_hash, id), None);
        }
        let erased = body.is_some();
        if erased {
            plan.writes
                .insert(payload_log_key.clone(), Some(encode_blob(&header, None)?));
        }
        Ok(erased)
    }

    /// Atomically index one operation and optionally remove its preceding log.
    pub(crate) async fn insert_indexed_operation_with_prune<L: LogId>(
        &self,
        topic: &Topic,
        operation: &Operation<E>,
        log_id: &L,
        prune_before_current: bool,
    ) -> Result<(bool, u64), StoreError> {
        let (inserted, pruned, _) = self
            .insert_indexed_operation_with_effects(
                topic,
                operation,
                log_id,
                prune_before_current,
                &[],
            )
            .await?;
        Ok((inserted, pruned))
    }

    /// Atomically index one operation, remove an authorized prefix, and erase
    /// authorized payloads while retaining their signed headers.
    pub(crate) async fn insert_indexed_operation_with_effects<L: LogId>(
        &self,
        topic: &Topic,
        operation: &Operation<E>,
        log_id: &L,
        prune_before_current: bool,
        erase_payloads: &[Hash],
    ) -> Result<(bool, u64, u64), StoreError> {
        let ptr = op_ptr(&operation.hash);
        if self.backend.get(&ptr).await?.is_some() {
            return Ok((false, 0, 0));
        }
        let topic_key = topic_key(topic, &operation.header.verifying_key, log_id)?;
        let prefix = log_prefix(&operation.header.verifying_key, log_id)?;
        let log_key = format!("{prefix}{:016x}", operation.header.seq_num);
        let indexed = IndexedOperation {
            topic,
            operation,
            log_id,
            prune_before_current,
            erase_payloads,
        };
        let mut plan = BatchPlan::<E>::default();
        let pruned_entries = if prune_before_current {
            self.plan_prune_writes(&mut plan, &indexed, &prefix).await?
        } else {
            0
        };
        let mut erased_payloads = 0;
        for id in erase_payloads {
            erased_payloads += u64::from(self.plan_payload_erasure(&mut plan, id).await?);
        }
        plan.plan_operation(&indexed, topic_key, prefix, log_key, ptr)?;
        self.backend.apply(&plan.into_writes()).await?;
        Ok((true, pruned_entries, erased_payloads))
    }

    pub(super) async fn prefix_prune_writes<L: LogId>(
        &self,
        author: &VerifyingKey,
        log_id: &L,
        until: u32,
    ) -> Result<(Vec<WriteOp>, u64), StoreError> {
        let prefix = log_prefix(author, log_id)?;
        let keys = self.backend.scan(&prefix, &scan_end(&prefix)).await?;
        let mut writes = Vec::new();
        let mut pruned = 0;
        for key in keys {
            if seq_from_key(&key, &prefix)? >= until {
                continue;
            }
            if let Some(blob) = self.backend.get(&key).await? {
                let (header, _) = decode_blob::<E>(&blob[..])?;
                let operation_hash = header.hash();
                writes.push(WriteOp::Delete {
                    key: op_ptr(&operation_hash),
                });
                if let Some(payload_hash) = header.payload_hash.as_ref() {
                    writes.push(WriteOp::Delete {
                        key: payload_ref_key(payload_hash, &operation_hash),
                    });
                }
            }
            writes.push(WriteOp::Delete { key });
            pruned += 1;
        }
        Ok((writes, pruned))
    }

    /// Resolve an operation id to its `log/...` key via the `op/<hash>` pointer.
    pub(super) async fn log_key_for(&self, id: &Hash) -> Result<Option<String>, StoreError> {
        match self.backend.get(&op_ptr(id)).await? {
            Some(bytes) => Ok(Some(String::from_utf8(bytes).map_err(codec)?)),
            None => Ok(None),
        }
    }

    /// Fetch an operation by id.
    pub async fn get_operation(&self, id: &Hash) -> Result<Option<Operation<E>>, StoreError> {
        let Some(log_key) = self.log_key_for(id).await? else {
            return Ok(None);
        };
        match self.backend.get(&log_key).await? {
            Some(blob) => Ok(Some(decode_op(&blob)?)),
            None => Ok(None),
        }
    }

    /// Whether an operation id is present.
    pub async fn has_operation(&self, id: &Hash) -> Result<bool, StoreError> {
        Ok(self.backend.get(&op_ptr(id)).await?.is_some())
    }

    /// Delete an operation and its log entry.
    pub async fn delete_operation(&self, id: &Hash) -> Result<bool, StoreError> {
        let ptr = op_ptr(id);
        let Some(log_key) = self.log_key_for(id).await? else {
            return Ok(false);
        };
        let mut writes = vec![
            WriteOp::Delete { key: ptr },
            WriteOp::Delete {
                key: log_key.clone(),
            },
        ];
        if let Some(blob) = self.backend.get(&log_key).await? {
            let (header, _) = decode_blob::<E>(&blob[..])?;
            if let Some(payload_hash) = header.payload_hash.as_ref() {
                writes.push(WriteOp::Delete {
                    key: payload_ref_key(payload_hash, id),
                });
            }
        }
        self.backend.apply(&writes).await?;
        Ok(true)
    }

    /// Drop an operation's payload, keeping its header (and so its log entry).
    pub async fn delete_operation_payload(&self, id: &Hash) -> Result<bool, StoreError> {
        let Some(log_key) = self.log_key_for(id).await? else {
            return Ok(false);
        };
        let Some(blob) = self.backend.get(&log_key).await? else {
            return Ok(false);
        };
        let (header, _body) = decode_blob::<E>(&blob[..])?;
        let stripped = encode_blob(&header, None)?;
        let mut writes = vec![WriteOp::Put {
            key: log_key,
            value: stripped,
        }];
        if let Some(payload_hash) = header.payload_hash.as_ref() {
            writes.push(WriteOp::Delete {
                key: payload_ref_key(payload_hash, id),
            });
        }
        self.backend.apply(&writes).await?;
        Ok(true)
    }

    /// Read a payload chunk-set retained until its signed header arrives.
    pub(crate) async fn pending_payload(
        &self,
        payload_hash: &Hash,
    ) -> Result<Option<Vec<u8>>, StoreError> {
        self.backend.get(&pending_payload_key(payload_hash)).await
    }

    /// Prepare writes that attach verified bytes to every eligible retained
    /// header naming this payload hash.
    ///
    /// Retention erasure removes the `payload-ref` key, so a later chunk cannot
    /// resurrect an intentionally erased body.
    pub(crate) async fn payload_attachment_writes(
        &self,
        payload_hash: &Hash,
        bytes: &[u8],
    ) -> Result<(Vec<WriteOp>, u64), StoreError> {
        if Hash::digest(bytes) != *payload_hash {
            return Err(codec("payload chunk-set has a false digest"));
        }
        self.attachment_writes(payload_hash, bytes).await
    }

    /// As [`Self::payload_attachment_writes`], for bytes that were hashed
    /// against `payload_hash` when they were assembled.
    ///
    /// Drop import walks multi-megabyte chunk-sets; hashing each one a second
    /// time here bought nothing that [`VerifiedBytes`] does not already state.
    pub(crate) async fn verified_payload_attachment_writes(
        &self,
        payload_hash: &Hash,
        bytes: &VerifiedBytes,
    ) -> Result<(Vec<WriteOp>, u64), StoreError> {
        self.attachment_writes(payload_hash, bytes.as_slice()).await
    }

    pub(super) async fn attachment_writes(
        &self,
        payload_hash: &Hash,
        bytes: &[u8],
    ) -> Result<(Vec<WriteOp>, u64), StoreError> {
        let mut writes = Vec::new();
        let mut attached = 0;
        for reference_key in self.backend.list(&payload_ref_prefix(payload_hash)).await? {
            let Some(log_key) = self.backend.get(&reference_key).await? else {
                writes.push(WriteOp::Delete { key: reference_key });
                continue;
            };
            let log_key = String::from_utf8(log_key).map_err(codec)?;
            let Some(blob) = self.backend.get(&log_key).await? else {
                writes.push(WriteOp::Delete { key: reference_key });
                continue;
            };
            let (header, body) = decode_blob::<E>(&blob[..])?;
            if header.payload_hash.as_ref() != Some(payload_hash) {
                return Err(codec("payload reference points at another digest"));
            }
            if header.payload_size != bytes.len() as u32 {
                return Err(codec("payload bytes do not match the signed size"));
            }
            if body.is_none() {
                writes.push(WriteOp::Put {
                    key: log_key,
                    value: encode_blob(&header, Some(bytes))?,
                });
                attached += 1;
            }
        }
        Ok((writes, attached))
    }

    /// Attach verified payload bytes to eligible retained headers atomically.
    ///
    /// A removed payload-reference key is the retention barrier: this returns
    /// zero instead of recreating a body that policy has erased.
    pub(crate) async fn attach_payload(
        &self,
        payload_hash: &Hash,
        bytes: &[u8],
    ) -> Result<u64, StoreError> {
        let (writes, attached) = self.payload_attachment_writes(payload_hash, bytes).await?;
        if !writes.is_empty() {
            self.backend.apply(&writes).await?;
        }
        Ok(attached)
    }

    /// Prepare one verified content-addressed blob write for an import batch.
    ///
    /// The digest check happened where the chunk-set was assembled, which is
    /// what [`VerifiedBytes`] records; the bytes then move straight into the
    /// write instead of being hashed and copied again.
    pub(crate) fn imported_blob_write(blob_hash: [u8; 32], bytes: VerifiedBytes) -> WriteOp {
        WriteOp::Put {
            key: format!("blob/{}", hex::encode(blob_hash)),
            value: bytes.into_inner(),
        }
    }

    /// Store content-addressed bytes in the shared muniment backend.
    pub async fn put_blob(&self, bytes: &[u8]) -> Result<BlobRef, StoreError> {
        let reference = BlobRef::blake3(bytes);
        let digest = hex::encode(reference.digest.as_32().map_err(codec)?);
        self.backend.put(&format!("blob/{digest}"), bytes).await?;
        Ok(reference)
    }

    /// Fetch content-addressed bytes after checking the typed reference.
    pub async fn get_blob(&self, reference: &BlobRef) -> Result<Option<Vec<u8>>, StoreError> {
        if reference.digest.alg != DigestAlg::Blake3 {
            return Err(codec("muniment blobs require a BLAKE3 reference"));
        }
        let digest = hex::encode(reference.digest.as_32().map_err(codec)?);
        let bytes = self.backend.get(&format!("blob/{digest}")).await?;
        match bytes {
            Some(bytes) if reference.verifies(&bytes) => Ok(Some(bytes)),
            Some(_) => Err(codec("stored blob does not match its typed reference")),
            None => Ok(None),
        }
    }

    /// Collect blobs absent from the domain-supplied retained reference set.
    ///
    /// Replication owns the atomic delete mechanics. The domain remains
    /// responsible for tracing every live-state and checkpoint reference.
    pub async fn collect_unreferenced_blobs<'a>(
        &self,
        retained: impl IntoIterator<Item = &'a BlobRef>,
    ) -> Result<BlobGcReport, StoreError> {
        let mut retained_keys = BTreeSet::new();
        for reference in retained {
            if reference.digest.alg != DigestAlg::Blake3 {
                return Err(codec("muniment blobs require a BLAKE3 reference"));
            }
            retained_keys.insert(format!(
                "blob/{}",
                hex::encode(reference.digest.as_32().map_err(codec)?)
            ));
        }
        let keys = self.backend.list("blob/").await?;
        let writes: Vec<_> = keys
            .iter()
            .filter(|key| !retained_keys.contains(*key))
            .cloned()
            .map(|key| WriteOp::Delete { key })
            .collect();
        self.backend.apply(&writes).await?;
        Ok(BlobGcReport {
            examined: keys.len() as u64,
            collected: writes.len() as u64,
        })
    }

    /// The number of distinct operations stored (one `op/<hash>` pointer each).
    pub async fn operation_count(&self) -> Result<usize, StoreError> {
        Ok(self.backend.list("op/").await?.len())
    }

    /// Whether the store holds no operations.
    pub async fn is_empty(&self) -> Result<bool, StoreError> {
        Ok(self.operation_count().await? == 0)
    }
}
