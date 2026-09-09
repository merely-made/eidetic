// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Positive-control tests for [`Backend::transact`](crate::Backend::transact),
//! S10's custody-lifecycle ruling. `Backend::apply` commits a batch
//! atomically but has no read half, so a caller that reads first (is any
//! reference to this blob still live?) and applies second (delete it) leaves
//! a gap a competing claim, transfer, or collection can land in. `transact`
//! closes that gap: the read and the write it decides on share one
//! transaction.
//!
//! Ported from the actual-backend custody probe
//! (`design_docs/mere_docs/testing/receipts/2026-09-08_stack_pillar_probes/custody-backend/RECEIPT.md`,
//! `src/lib.rs`), which ran the same shapes against real Muniment/redb through
//! a research adapter that serialized commands itself. These versions run
//! directly against `Backend::transact`, no adapter, and turn the probe's
//! negative control into a passing positive control: the collection recheck
//! and the delete now share one transaction instead of being two calls with a
//! gap between them.
#![cfg(test)]

use crate::backend::{Backend, MemoryBackend, TransactionReader, WriteOp};
use crate::error::StoreError;

fn blob_key() -> String {
    "blob/probe".to_string()
}

fn ref_key(owner: &str) -> String {
    format!("probe/ref/{owner}")
}

const REF_PREFIX: &str = "probe/ref/";

/// A collection proposal built inside `transact`, rechecking the reference
/// prefix at commit time, refuses to delete once a competing claim has
/// landed — even one written before this transaction started.
async fn collection_refuses_a_claim_that_landed_before_the_transaction<B: Backend + Sync>(backend: B) {
    backend.put(&blob_key(), b"bytes").await.unwrap();
    // The competing claim lands first, before `transact` is ever called.
    backend.put(&ref_key("new-owner"), b"1").await.unwrap();

    backend
        .transact(Box::new(|reader: &dyn TransactionReader| {
            if !reader.list(REF_PREFIX).unwrap().is_empty() {
                return vec![]; // a live reference exists: refuse
            }
            vec![WriteOp::Delete { key: blob_key() }]
        }))
        .await
        .unwrap();

    assert!(
        backend.get(&blob_key()).await.unwrap().is_some(),
        "transact must refuse collection once a reference is live"
    );
    assert!(backend.get(&ref_key("new-owner")).await.unwrap().is_some());
}

/// The crucial scenario: a collection with no live references commits inside
/// `transact`, and a claim that lands *after* that commit sees the blob gone
/// and so cannot write a dangling reference to it.
async fn claim_after_commit_sees_the_blob_gone_with_no_dangling_reference<B: Backend + Sync>(backend: B) {
    backend.put(&blob_key(), b"bytes").await.unwrap();
    // No live references: the transaction commits the collection.
    backend
        .transact(Box::new(|reader: &dyn TransactionReader| {
            if !reader.list(REF_PREFIX).unwrap().is_empty() {
                return vec![];
            }
            vec![WriteOp::Delete { key: blob_key() }]
        }))
        .await
        .unwrap();

    // A late claim, exactly like the probe's `Custody::claim`, checks the blob
    // is present before writing its reference.
    let blob_present = backend.get(&blob_key()).await.unwrap().is_some();
    assert!(!blob_present, "collection committed: the blob is gone");
    if blob_present {
        backend.put(&ref_key("late-owner"), b"1").await.unwrap();
    }
    assert!(
        backend.get(&ref_key("late-owner")).await.unwrap().is_none(),
        "no dangling reference: a late claim must refuse against a gone blob"
    );
}

/// The probe's original negative control, kept as a documenting regression: a
/// precomputed delete batch applied through `apply` alone (no `transact`)
/// still does not recheck a competing claim that landed after the read that
/// produced the batch. `apply` remains insufficient on its own; only
/// `transact` closes the gap, which is why claims, transfers, and collection
/// must all go through it.
async fn negative_control_apply_alone_does_not_recheck_a_new_owner<B: Backend + Sync>(backend: B) {
    backend.put(&blob_key(), b"bytes").await.unwrap();
    // The batch is computed from a stale read: no live reference observed.
    let proposed = vec![WriteOp::Delete { key: blob_key() }];
    // A claim lands after that read but before the batch applies.
    backend.put(&ref_key("new-owner"), b"1").await.unwrap();
    // `apply` commits the stale batch wholesale: it has no read half to
    // recheck against.
    backend.apply(&proposed).await.unwrap();

    assert!(
        backend.get(&blob_key()).await.unwrap().is_none(),
        "negative control must expose the loss: apply alone deletes the blob"
    );
    assert!(
        backend.get(&ref_key("new-owner")).await.unwrap().is_some(),
        "and leaves the new owner's reference dangling"
    );
}

mod memory_backend {
    use super::*;

    #[test]
    fn collection_refuses_a_claim_that_landed_before_the_transaction() {
        pollster::block_on(super::collection_refuses_a_claim_that_landed_before_the_transaction(
            MemoryBackend::new(),
        ));
    }

    #[test]
    fn claim_after_commit_sees_the_blob_gone_with_no_dangling_reference() {
        pollster::block_on(
            super::claim_after_commit_sees_the_blob_gone_with_no_dangling_reference(
                MemoryBackend::new(),
            ),
        );
    }

    #[test]
    fn negative_control_apply_alone_does_not_recheck_a_new_owner() {
        pollster::block_on(super::negative_control_apply_alone_does_not_recheck_a_new_owner(
            MemoryBackend::new(),
        ));
    }
}

#[cfg(feature = "redb")]
mod redb_backend {
    #[allow(unused_imports)]
    use super::*;
    use crate::RedbBackend;

    fn temp_backend() -> (tempfile::TempDir, RedbBackend) {
        let dir = tempfile::tempdir().unwrap();
        let backend = RedbBackend::open(dir.path().join("custody.redb")).unwrap();
        (dir, backend)
    }

    #[test]
    fn collection_refuses_a_claim_that_landed_before_the_transaction() {
        let (_dir, backend) = temp_backend();
        pollster::block_on(super::collection_refuses_a_claim_that_landed_before_the_transaction(
            backend,
        ));
    }

    #[test]
    fn claim_after_commit_sees_the_blob_gone_with_no_dangling_reference() {
        let (_dir, backend) = temp_backend();
        pollster::block_on(
            super::claim_after_commit_sees_the_blob_gone_with_no_dangling_reference(backend),
        );
    }

    #[test]
    fn negative_control_apply_alone_does_not_recheck_a_new_owner() {
        let (_dir, backend) = temp_backend();
        pollster::block_on(super::negative_control_apply_alone_does_not_recheck_a_new_owner(
            backend,
        ));
    }
}

/// A `Backend` that does not override `transact`, so it falls back to the
/// default body — the refusal every external implementor gets for free until
/// it adds a real one.
struct NoTransact(MemoryBackend);

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl Backend for NoTransact {
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, StoreError> {
        self.0.get(key).await
    }

    async fn put(&self, key: &str, bytes: &[u8]) -> Result<(), StoreError> {
        self.0.put(key, bytes).await
    }

    async fn delete(&self, key: &str) -> Result<(), StoreError> {
        self.0.delete(key).await
    }

    async fn list(&self, prefix: &str) -> Result<Vec<String>, StoreError> {
        self.0.list(prefix).await
    }

    async fn scan(&self, start: &str, end: &str) -> Result<Vec<String>, StoreError> {
        self.0.scan(start, end).await
    }

    async fn apply(&self, ops: &[WriteOp]) -> Result<(), StoreError> {
        self.0.apply(ops).await
    }
}

#[test]
fn default_refusal_is_typed() {
    pollster::block_on(async {
        let backend = NoTransact(MemoryBackend::new());
        let result = backend
            .transact(Box::new(|_reader: &dyn TransactionReader| Vec::new()))
            .await;
        assert_eq!(result, Err(StoreError::NotTransactional));
    });
}

#[cfg(feature = "zip")]
#[test]
fn zip_backend_transact_refusal_is_typed() {
    pollster::block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let backend = crate::ZipBackend::open(dir.path().join("store.zip")).unwrap();
        let result = backend
            .transact(Box::new(|_reader: &dyn TransactionReader| Vec::new()))
            .await;
        assert_eq!(result, Err(StoreError::NotTransactional));
    });
}
