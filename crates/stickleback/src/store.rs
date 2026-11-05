// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! A muniment-backed p2panda operation store.
//!
//! [`MunimentStore`] implements the three traits p2panda-net's `LogSync`
//! reconciles against, [`OperationStore`], [`LogStore`] and [`TopicStore`], over
//! a muniment [`Backend`]. One adapter, so a moot's log, a murmur and mesh's
//! job queue can all sit on the same store family (redb on desktop, IndexedDB
//! in the browser) instead of a hand-rolled backend each.
//!
//! `LogSync` needs only `LogStore` + `TopicStore`, but the full trio is provided
//! so the adapter is a drop-in for p2panda's own `SqliteStore`. The `?Send`
//! backend is no obstacle: `LogSync` drives the store from a single-threaded
//! ractor actor, so the handle must be `Send` (it is, over a `Send` backend) but
//! its method futures need not be.
//!
//! ## Key schema
//!
//! muniment exposes a flat string key space. Three namespaces live in it:
//!
//! - `log/<author>/<log>/<seq>` holds the operation, CBOR `(Header, body)`. The
//!   author and log id are hex; `seq` is a zero-padded 16-hex sequence number so
//!   keys sort in log order and a single [`scan`](Backend::scan) walks a log.
//! - `op/<hash>` points at the `log/...` key above, so a lookup by operation id
//!   finds the one blob without a scan.
//! - `topic/<topic>/<author>/<log>` records a topic association, value empty.
//!
//! The log id segment is `hex(CBOR(log_id))`, which works for any `L: LogId`
//! (`u64` for direct exchange and mesh's per-space logs, `[u8; 32]` and friends
//! alike). The
//! trailing `/` after it keeps one log's key range from bleeding into another's
//! whose encoding shares a prefix.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::marker::PhantomData;
use std::sync::Arc;

use futures_util::{StreamExt, stream, stream::BoxStream};
use muniment::{Backend, StoreError, WriteOp};
use p2panda_core::cbor::{decode_cbor, encode_cbor};
use p2panda_core::{
    AnyOperation, Body, Extensions, Hash, Header, LogId, Operation, Topic, VerifyingKey,
};
use p2panda_store::logs::{LogEntry, LogStore};
use p2panda_store::operations::OperationStore;
use p2panda_store::topics::TopicStore;
use proofs::{BlobRef, DigestAlg};
use tokio::sync::Mutex;

/// Result of one domain-driven content collection pass.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BlobGcReport {
    pub examined: u64,
    pub collected: u64,
}

/// One already-authorized operation prepared for an atomic batch insert.
pub(crate) struct IndexedOperation<'a, E, L> {
    pub topic: &'a Topic,
    pub operation: &'a Operation<E>,
    pub log_id: &'a L,
    pub prune_before_current: bool,
    pub erase_payloads: &'a [Hash],
}

/// Bytes whose BLAKE3 digest the producer has already checked against the
/// content address they will be stored under.
///
/// Drop import hashes each assembled chunk-set once, as part of checking the
/// chunk layout. Carrying that proof in the type lets the store skip a second
/// pass over the same multi-megabyte buffer without weakening the check for
/// callers that arrive with bytes of unproven provenance: those still go
/// through the hashing entry points, which are the only way in without one of
/// these.
pub(crate) struct VerifiedBytes(Vec<u8>);

impl VerifiedBytes {
    /// Claim a digest the caller has just computed over exactly these bytes.
    pub(crate) fn assume_verified(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }

    pub(crate) fn as_slice(&self) -> &[u8] {
        &self.0
    }

    pub(crate) fn into_inner(self) -> Vec<u8> {
        self.0
    }
}

/// One operation a batch has planned but not yet submitted.
///
/// Its position in the log lives in [`BatchPlan::by_log`], which is what prunes
/// query, so it is deliberately not repeated here.
struct PlannedOperation<E> {
    pointer: String,
    log_key: String,
    payload_ref: Option<String>,
    header: Header<E>,
    has_body: bool,
}

/// The writes one atomic batch is accumulating, plus the operations it has
/// already planned but not yet submitted.
///
/// A later entry in the same batch may prune or erase an earlier one, so the
/// two have to be tracked together: `writes` is keyed so a revision replaces
/// the earlier decision for that key, and `planned` is what a prune or erasure
/// looks at before falling back to the durable store. `by_log` indexes
/// `planned` by `(log prefix, sequence number, id)` purely so pruning a prefix
/// is a range query — scanning `planned` for every pruning entry made the
/// batch quadratic in its own size, and this is the chokepoint both drop import
/// and LogSync push their whole backlog through.
struct BatchPlan<E> {
    writes: BTreeMap<String, Option<Vec<u8>>>,
    planned: BTreeMap<String, PlannedOperation<E>>,
    by_log: BTreeSet<(String, u32, String)>,
}

// Derived `Default` would demand `E: Default`; the three maps need nothing of `E`.
impl<E> Default for BatchPlan<E> {
    fn default() -> Self {
        Self {
            writes: BTreeMap::new(),
            planned: BTreeMap::new(),
            by_log: BTreeSet::new(),
        }
    }
}

impl<E: Extensions> BatchPlan<E> {
    /// Lower the final overlay into one backend write per key.
    fn into_writes(self) -> Vec<WriteOp> {
        self.writes
            .into_iter()
            .map(|(key, value)| match value {
                Some(value) => WriteOp::Put { key, value },
                None => WriteOp::Delete { key },
            })
            .collect()
    }

    /// Fold one caller-supplied write into the plan, last decision winning.
    fn record(&mut self, write: WriteOp) {
        match write {
            WriteOp::Put { key, value } => self.writes.insert(key, Some(value)),
            WriteOp::Delete { key } => self.writes.insert(key, None),
        };
    }

    /// Undo everything this batch planned for `prefix` below `until`.
    fn prune_planned(&mut self, prefix: &str, until: u32) {
        let stale: Vec<_> = self
            .by_log
            .range((prefix.to_owned(), 0, String::new())..(prefix.to_owned(), until, String::new()))
            .cloned()
            .collect();
        for key in stale {
            self.by_log.remove(&key);
            let Some(operation) = self.planned.remove(&key.2) else {
                continue;
            };
            self.writes.insert(operation.pointer, None);
            self.writes.insert(operation.log_key, None);
            if let Some(payload_ref) = operation.payload_ref {
                self.writes.insert(payload_ref, None);
            }
        }
    }

    /// Record the topic, log, pointer and payload-reference writes that index
    /// one operation, and remember it as prunable by later entries.
    fn plan_operation<L: LogId>(
        &mut self,
        entry: &IndexedOperation<'_, E, L>,
        topic: String,
        log_prefix: String,
        log_key: String,
        pointer: String,
    ) -> Result<(), StoreError> {
        self.writes.insert(topic, Some(Vec::new()));
        self.writes
            .insert(log_key.clone(), Some(encode_op(entry.operation)?));
        self.writes
            .insert(pointer.clone(), Some(log_key.clone().into_bytes()));
        let payload_ref = entry.operation.header.payload_hash.as_ref().map(|hash| {
            (
                payload_ref_key(hash, &entry.operation.hash),
                log_key.clone().into_bytes(),
            )
        });
        if let Some((key, value)) = payload_ref.as_ref() {
            self.writes.insert(key.clone(), Some(value.clone()));
        }
        let id_hex = entry.operation.hash.to_hex();
        self.by_log
            .insert((log_prefix, entry.operation.header.seq_num, id_hex.clone()));
        self.planned.insert(
            id_hex,
            PlannedOperation {
                pointer,
                log_key,
                payload_ref: payload_ref.map(|(key, _)| key),
                header: entry.operation.header.clone(),
                has_body: entry.operation.body.is_some(),
            },
        );
        Ok(())
    }
}

/// A p2panda operation store over a muniment [`Backend`].
///
/// Generic over the backend `B` and the operation extensions `E`. Cheap to clone
/// (over a `Clone` backend), so a handle can be handed to `LogSync`.
pub struct MunimentStore<B, E> {
    backend: B,
    /// Process-local serialization for one author/log frontier. Clones share
    /// this table, which covers LogSync drains and domain authoring handles
    /// built from the same store.
    ingress: Arc<Mutex<BTreeMap<String, Arc<Mutex<()>>>>>,
    // `fn() -> E` marks the extension type without owning it, keeping the handle
    // `Send`/`Sync` and covariant regardless of `E`.
    _ext: PhantomData<fn() -> E>,
}

impl<B: Clone, E> Clone for MunimentStore<B, E> {
    fn clone(&self) -> Self {
        Self {
            backend: self.backend.clone(),
            ingress: Arc::clone(&self.ingress),
            _ext: PhantomData,
        }
    }
}

impl<B, E> MunimentStore<B, E> {
    /// Wrap a backend.
    pub fn new(backend: B) -> Self {
        Self {
            backend,
            ingress: Arc::new(Mutex::new(BTreeMap::new())),
            _ext: PhantomData,
        }
    }

    /// The backend this store reads and writes through.
    pub fn backend(&self) -> &B {
        &self.backend
    }

    pub(crate) async fn ingress_lock<L: LogId>(
        &self,
        author: &VerifyingKey,
        log_id: &L,
    ) -> Result<Arc<Mutex<()>>, StoreError> {
        let key = format!("{}/{}", author.to_hex(), log_seg(log_id)?);
        let mut locks = self.ingress.lock().await;
        Ok(Arc::clone(
            locks.entry(key).or_insert_with(|| Arc::new(Mutex::new(()))),
        ))
    }
}

// These typed conveniences keep existing domain readers unambiguous now that
// the same store also implements p2panda 0.7.3's `LogStore<AnyOperation>`.
impl<B, E> MunimentStore<B, E>
where
    B: Backend,
    E: Extensions,
{
    pub async fn get_latest_entry<L: LogId>(
        &self,
        author: &VerifyingKey,
        log_id: &L,
    ) -> Result<Option<Operation<E>>, StoreError> {
        let prefix = log_prefix(author, log_id)?;
        let keys = self.backend.scan(&prefix, &scan_end(&prefix)).await?;
        match keys.last() {
            Some(key) => match self.backend.get(key).await? {
                Some(blob) => Ok(Some(decode_op(&blob)?)),
                None => Ok(None),
            },
            None => Ok(None),
        }
    }

    pub fn log_entries<L: LogId>(
        &self,
        author: &VerifyingKey,
        log_id: &L,
        after: Option<u32>,
        until: Option<u32>,
    ) -> Result<BoxStream<'static, Result<LogEntry<Operation<E>, L>, StoreError>>, StoreError>
    where
        B: Clone + Send + 'static,
        E: Send + 'static,
        L: Send + 'static,
    {
        <Self as LogStore<Operation<E>, VerifyingKey, L, u32, Hash>>::log_entries(
            self, author, log_id, after, until,
        )
    }

    /// Compatibility collector for domain code that wants an owned typed
    /// history. The p2panda trait itself is now streaming; this helper keeps
    /// the materialized view explicit at the application boundary.
    pub async fn get_log_entries<L: LogId>(
        &self,
        author: &VerifyingKey,
        log_id: &L,
        after: Option<u32>,
        until: Option<u32>,
    ) -> Result<Option<Vec<(Operation<E>, Vec<u8>)>>, StoreError> {
        let prefix = log_prefix(author, log_id)?;
        let keys = self.backend.scan(&prefix, &scan_end(&prefix)).await?;
        let mut out = Vec::new();
        for key in &keys {
            if !in_range(seq_from_key(key, &prefix)?, after, until) {
                continue;
            }
            if let Some(blob) = self.backend.get(key).await? {
                let op = decode_op::<E>(&blob)?;
                let header = op.header.encode();
                out.push((op, header));
            }
        }
        Ok((!out.is_empty()).then_some(out))
    }
}

// ── key helpers ───────────────────────────────────────────────────────────────

// ── operation encode / decode ─────────────────────────────────────────────────

/// The `(header-bytes, Option<body-bytes>)` blob stored at a log key.
///
/// p2panda 0.7.1 dropped the serde impls on `Header` and made its CBOR cache,
/// size and digest private, so a header can only be rebuilt through
/// `Header::decode`. The blob therefore carries the header as its own encoded
/// byte string instead of inline. The header bytes are byte-identical to 0.7.0
/// — `Header::encode` emits the same CBOR array the old serde impl did — so
/// only the nesting changed.
///
/// The body arrives borrowed: a `&[u8]` serialises to the same CBOR array a
/// `Vec<u8>` does, so encoding a retained body no longer needs a private copy
/// of it first — payload attachment encodes multi-megabyte bodies.
fn encode_blob<E: Extensions>(
    header: &Header<E>,
    body: Option<&[u8]>,
) -> Result<Vec<u8>, StoreError> {
    // `serde_bytes` so the header travels as a CBOR byte string. A bare
    // `Vec<u8>` serialises as an array of integers, which costs roughly two
    // bytes per byte.
    encode_cbor(&(serde_bytes::Bytes::new(&header.encode()), body)).map_err(codec)
}

/// Decode a stored blob into its header and body.
fn decode_blob<E: Extensions>(bytes: &[u8]) -> Result<(Header<E>, Option<Vec<u8>>), StoreError> {
    let (header_bytes, body): (serde_bytes::ByteBuf, Option<Vec<u8>>) =
        decode_cbor(bytes).map_err(stale_blob)?;
    let header = Header::decode(&header_bytes).map_err(stale_blob)?;
    Ok((header, body))
}

/// Every stale-format read funnels through one message.
fn stale_blob(error: impl std::fmt::Display) -> StoreError {
    StoreError::Codec(format!(
        "stored operation is not p2panda 0.7.1 format; export semantic events with the previous build and re-author them into a fresh store: {error}"
    ))
}

/// Encode an operation as the blob stored at its log key.
fn encode_op<E: Extensions>(op: &Operation<E>) -> Result<Vec<u8>, StoreError> {
    let body = op.body.as_ref().map(|b| b.to_bytes());
    encode_blob(&op.header, body.as_deref())
}

/// Decode a stored blob back into an operation, recomputing its id from the
/// header.
fn decode_op<E: Extensions>(bytes: &[u8]) -> Result<Operation<E>, StoreError> {
    let (header, body) = decode_blob(bytes)?;
    Ok(Operation {
        hash: header.hash(),
        header,
        body: body.map(Body::from),
    })
}

/// Decode a stored blob without knowing its application extension type.
fn decode_any_op(bytes: &[u8]) -> Result<AnyOperation, StoreError> {
    let (header_bytes, body): (serde_bytes::ByteBuf, Option<Vec<u8>>) =
        decode_cbor(bytes).map_err(stale_blob)?;
    let header = p2panda_core::AnyHeader::decode(&header_bytes).map_err(stale_blob)?;
    let hash = header.hash();
    Ok(AnyOperation {
        hash,
        header,
        body: body.map(Body::from),
    })
}

// ── OperationStore ────────────────────────────────────────────────────────────

impl<B, E> OperationStore<Operation<E>, Hash> for MunimentStore<B, E>
where
    B: Backend,
    E: Extensions,
{
    type Error = StoreError;

    async fn insert_operation<L: LogId>(
        &self,
        id: &Hash,
        operation: &Operation<E>,
        log_id: &L,
    ) -> Result<bool, StoreError> {
        let ptr = op_ptr(id);
        // Insert-or-ignore: an operation id already present is a no-op.
        if self.backend.get(&ptr).await?.is_some() {
            return Ok(false);
        }
        let prefix = log_prefix(&operation.header.verifying_key, log_id)?;
        let log_key = format!("{prefix}{:016x}", operation.header.seq_num);
        let blob = encode_op(operation)?;
        // The blob and its pointer land together so a reader never sees one
        // without the other.
        self.backend
            .apply(&{
                let mut writes = vec![
                    WriteOp::Put {
                        key: log_key.clone(),
                        value: blob,
                    },
                    WriteOp::Put {
                        key: ptr,
                        value: log_key.clone().into_bytes(),
                    },
                ];
                if let Some(payload_hash) = operation.header.payload_hash.as_ref() {
                    writes.push(WriteOp::Put {
                        key: payload_ref_key(payload_hash, &operation.hash),
                        value: log_key.into_bytes(),
                    });
                }
                writes
            })
            .await?;
        Ok(true)
    }

    // These forward to the inherent twins above (inherent resolution wins), which
    // hold the real logic and stay callable without pinning `L`.

    async fn get_operation(&self, id: &Hash) -> Result<Option<Operation<E>>, StoreError> {
        self.get_operation(id).await
    }

    async fn get_operation_tx(&self, id: &Hash) -> Result<Option<Operation<E>>, StoreError> {
        self.get_operation(id).await
    }

    async fn has_operation(&self, id: &Hash) -> Result<bool, StoreError> {
        self.has_operation(id).await
    }

    async fn has_operation_tx(&self, id: &Hash) -> Result<bool, StoreError> {
        self.has_operation(id).await
    }

    async fn delete_operation(&self, id: &Hash) -> Result<bool, StoreError> {
        self.delete_operation(id).await
    }

    async fn delete_operation_payload(&self, id: &Hash) -> Result<bool, StoreError> {
        self.delete_operation_payload(id).await
    }
}

// ── LogStore ──────────────────────────────────────────────────────────────────

// ── TopicStore ────────────────────────────────────────────────────────────────

#[cfg(test)]
#[path = "store/tests.rs"]
mod tests;

#[path = "store/commit.rs"]
mod commit;

#[path = "store/logs.rs"]
mod logs;

#[path = "store/topics.rs"]
mod topics;

#[path = "store/keys.rs"]
mod keys;

use keys::{
    codec, in_range, log_prefix, log_seg, op_ptr, payload_ref_key, payload_ref_prefix, scan_end,
    seq_from_key, topic_key,
};

pub(crate) use keys::pending_payload_key;
