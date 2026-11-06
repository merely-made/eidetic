// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Frozen research results in the sealed owner, moved from Turnstone's
//! `sealed_records_tests`: deposit replays, reads re-check, limits refuse
//! before unseal.

use eidetic::capture::BoundedRead;
use eidetic::records::{
    RecordFilters, RecordOperation, RecordQuery, RecordSettings, RecordSnapshot,
};
use eidetic::{Backend, Hash, PayloadSealer, SealEpochId, SealedBlobRef};
use eidetic_search::records::{evaluate, sealed};
use muniment::{FjallStore, StoreError, WriteOp};
use std::sync::atomic::{AtomicUsize, Ordering};

struct Limited(FjallStore);

#[async_trait::async_trait]
impl Backend for Limited {
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

#[async_trait::async_trait(?Send)]
impl BoundedRead for Limited {
    async fn get_limited(&self, key: &str, max: usize) -> Result<Option<Vec<u8>>, StoreError> {
        let value = self.0.get(key).await?;
        if value.as_ref().is_some_and(|v| v.len() > max) {
            return Err(StoreError::LimitExceeded {
                kind: muniment::StoreLimitKind::ValueBytes,
                limit: max as u64,
            });
        }
        Ok(value)
    }
}

#[derive(Default)]
struct CountingSealer {
    unseals: AtomicUsize,
}

impl PayloadSealer for CountingSealer {
    fn seal(&self, hash: &Hash, clear: &[u8]) -> eidetic::Result<(Vec<u8>, SealedBlobRef)> {
        let mut data = hash.to_hex().into_bytes();
        data.extend(clear.iter().map(|v| v ^ 0x73));
        Ok((
            data,
            SealedBlobRef {
                epoch: SealEpochId([3; 16]),
                format: "fixture".into(),
            },
        ))
    }
    fn unseal(&self, hash: &Hash, marker: &SealedBlobRef, data: &[u8]) -> eidetic::Result<Vec<u8>> {
        self.unseals.fetch_add(1, Ordering::SeqCst);
        if marker.format != "fixture" || !data.starts_with(hash.to_hex().as_bytes()) {
            return Err(eidetic::Error::new("refused"));
        }
        let clear: Vec<_> = data[64..].iter().map(|v| v ^ 0x73).collect();
        if Hash::of(&clear) != *hash {
            return Err(eidetic::Error::new("invalid hash"));
        }
        Ok(clear)
    }
}

#[test]
fn saved_result_reopens_replays_and_refuses_forgery_and_oversize() {
    pollster::block_on(async {
        let root = tempfile::tempdir().unwrap();
        let sealer = CountingSealer::default();
        let mut store = Limited(FjallStore::open(root.path()).unwrap());
        let snapshot = RecordSnapshot {
            scope_generation: "session:revision".into(),
            complete: true,
            exclusions: vec![],
            sources: vec![],
            definitions: vec![],
            records: vec![],
            lifecycle: vec![],
        };
        let query = RecordQuery {
            operation: RecordOperation::SourceLookup,
            filters: RecordFilters::default(),
        };
        let result = evaluate(&snapshot, &query, &RecordSettings::default()).unwrap();
        let id = sealed::deposit_result(&mut store, &sealer, &result)
            .await
            .unwrap();
        drop(store);

        let mut store = Limited(FjallStore::open(root.path()).unwrap());
        assert_eq!(
            sealed::load_result(&mut store, &sealer, id).await.unwrap(),
            result
        );

        let mut forged = result.clone();
        forged.operation_version = "forged".into();
        assert!(
            sealed::deposit_result(&mut store, &sealer, &forged)
                .await
                .is_err()
        );

        let before = sealer.unseals.load(Ordering::SeqCst);
        let blob_key = format!("blob:{}", id.0.to_hex());
        store.put(&blob_key, &vec![0; 8192]).await.unwrap();
        let error = sealed::load_result_bounded(&mut store, &sealer, id, 4096, None)
            .await
            .unwrap_err();
        assert!(matches!(
            error.storage_error(),
            Some(StoreError::LimitExceeded { .. })
        ));
        assert_eq!(
            sealer.unseals.load(Ordering::SeqCst),
            before,
            "refused before unseal"
        );
    });
}
