// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Kept page text over `MemoryBackend` standing in for IndexedDB (both are the
//! `Backend` seam): the records are `PageTextStore`'s, bounded, idempotent per
//! version. The goal-surface read-back cases stay in the Graphshell app.

use crate::kept_text::*;
use crate::{CaptureContent, Hash, PageTextStore, ResourceCaptureStore};
use muniment::{Backend, MemoryBackend, StoreError, TransactFn, WriteOp};
use std::collections::BTreeSet;

const PAGE: &str = "https://garden.example/compost?utm_source=feed#top";
const CANON: &str = "https://garden.example/compost";
const BODY: &str = "Turn the pile weekly. Browns balance greens, and a damp, warm heap \
    breaks down faster than a dry one.";

fn policy() -> KeepTextPolicy {
    KeepTextPolicy {
        enabled: true,
        max_bytes: 0,
        excluded_origins: BTreeSet::new(),
    }
}
fn page(url: &str, text: &str, at_ms: u64) -> KeptPage {
    KeptPage {
        url: url.into(),
        text: text.into(),
        at_ms,
        private: false,
    }
}
fn batch(pages: Vec<KeptPage>) -> KeptBatch {
    KeptBatch {
        policy: policy(),
        pages,
    }
}
async fn record(backend: &MemoryBackend, pages: Vec<KeptPage>) -> KeptSummary {
    record_kept_pages(backend, &batch(pages)).await
}

#[test]
fn records_are_page_text_stores_and_the_version_is_the_blake3_of_the_text() {
    pollster::block_on(async {
        let backend = MemoryBackend::new();
        let summary = record(&backend, vec![page(PAGE, BODY, 7)]).await;
        assert_eq!(
            (
                summary.kept,
                summary.unchanged,
                summary.refused,
                summary.error
            ),
            (1, 0, 0, None)
        );

        let version = Hash::of(BODY.as_bytes()).to_hex();
        let store = PageTextStore::new(backend.clone());
        assert_eq!(store.text_for(CANON).await.unwrap().as_deref(), Some(BODY));
        let entry = store.reference(CANON).await.unwrap().unwrap();
        assert_eq!(
            (entry.url.as_str(), entry.blob.as_str(), entry.stored_at_ms),
            (CANON, version.as_str(), 7)
        );

        // The key layout is PageTextStore's and ResourceCaptureStore's, nothing else.
        assert_eq!(
            backend.list("blob/").await.unwrap(),
            vec![format!("blob/{version}")]
        );
        let slots = backend.list("page-text/").await.unwrap();
        assert_eq!(slots.len(), 1);
        assert_eq!(
            slots[0],
            format!("page-text/{}", Hash::of(CANON.as_bytes()).to_hex())
        );
        let versions = ResourceCaptureStore::new(&backend)
            .for_url(CANON)
            .await
            .unwrap();
        assert_eq!(versions.len(), 1);
        assert_eq!(versions[0].content, CaptureContent::PageText);
        assert_eq!(versions[0].content_hash.to_hex(), version);
        let resource = chartulary::resource_id(CANON).to_string();
        assert_eq!(
            backend.list("resource-capture/").await.unwrap(),
            vec![format!("resource-capture/{resource}/page-text/{version}")]
        );
        assert_eq!(backend.len(), 3, "one blob, one address slot, one version");
    });
}

#[test]
fn keeping_the_same_version_again_writes_nothing_and_a_new_version_adds_one() {
    pollster::block_on(async {
        let backend = MemoryBackend::new();
        record(&backend, vec![page(PAGE, BODY, 1)]).await;
        let before = backend.len();
        // Loaded again later, and under another spelling of the address.
        let again = record(&backend, vec![page(CANON, BODY, 99)]).await;
        assert_eq!((again.kept, again.unchanged), (0, 1));
        assert_eq!(backend.len(), before);
        let store = PageTextStore::new(backend.clone());
        assert_eq!(
            store.reference(CANON).await.unwrap().unwrap().stored_at_ms,
            1
        );

        // Redelivery of one delivery (a retry after a lost acknowledgement).
        let twice = record(&backend, vec![page(PAGE, BODY, 1), page(PAGE, BODY, 1)]).await;
        assert_eq!((twice.kept, twice.unchanged), (0, 2));
        assert_eq!(backend.len(), before);

        let changed = record(
            &backend,
            vec![page(PAGE, "The pile was turned; it steams.", 5)],
        )
        .await;
        assert_eq!((changed.kept, changed.unchanged), (1, 0));
        assert_eq!(
            ResourceCaptureStore::new(&backend)
                .for_url(CANON)
                .await
                .unwrap()
                .len(),
            2
        );
        assert_eq!(
            store.text_for(CANON).await.unwrap().as_deref(),
            Some("The pile was turned; it steams."),
            "the address slot follows the latest version"
        );
    });
}

#[test]
fn size_is_bounded_by_the_policy_and_by_the_hard_cap() {
    let mut small = policy();
    small.max_bytes = 64;
    let at_bound = page(CANON, &"a".repeat(64), 1);
    let over = page(CANON, &"a".repeat(65), 1);
    assert_eq!(judge(&small, &at_bound), Ok(()));
    assert_eq!(judge(&small, &over), Err(KeptRefusal::TooLarge));
    // Unset means the default bound; a larger ask is clamped to the hard cap.
    let default = policy();
    assert_eq!(
        judge(&default, &page(CANON, &"a".repeat(DEFAULT_KEPT_BYTES), 1)),
        Ok(())
    );
    assert_eq!(
        judge(
            &default,
            &page(CANON, &"a".repeat(DEFAULT_KEPT_BYTES + 1), 1)
        ),
        Err(KeptRefusal::TooLarge)
    );
    let mut huge = policy();
    huge.max_bytes = usize::MAX;
    assert_eq!(
        judge(&huge, &page(CANON, &"a".repeat(MAX_KEPT_BYTES), 1)),
        Ok(())
    );
    assert_eq!(
        judge(&huge, &page(CANON, &"a".repeat(MAX_KEPT_BYTES + 1), 1)),
        Err(KeptRefusal::TooLarge)
    );
    // Multi-byte text counts bytes, not characters.
    assert_eq!(
        judge(&small, &page(CANON, &"é".repeat(33), 1)),
        Err(KeptRefusal::TooLarge)
    );

    pollster::block_on(async {
        let backend = MemoryBackend::new();
        let pages: Vec<_> = (0..MAX_PAGES_PER_DELIVERY + 3)
            .map(|n| {
                page(
                    &format!("https://garden.example/p{n}"),
                    &format!("page {n} text"),
                    n as u64,
                )
            })
            .collect();
        let summary = record(&backend, pages).await;
        assert_eq!((summary.kept, summary.refused), (MAX_PAGES_PER_DELIVERY, 3));
        let refused = record(
            &backend,
            vec![page(CANON, &"a".repeat(DEFAULT_KEPT_BYTES + 1), 1)],
        )
        .await;
        assert_eq!((refused.kept, refused.refused), (0, 1));
    });
}

#[test]
fn disabled_private_foreign_excluded_and_empty_pages_are_not_kept() {
    let mut excluded = policy();
    excluded
        .excluded_origins
        .insert("https://private.example".into());
    assert_eq!(
        judge(&excluded, &page("https://private.example/x?y=1", BODY, 1)),
        Err(KeptRefusal::Excluded)
    );
    assert_eq!(
        judge(&excluded, &page("https://garden.example/x", BODY, 1)),
        Ok(())
    );
    assert_eq!(
        judge(&policy(), &page("file:///etc/hosts", BODY, 1)),
        Err(KeptRefusal::Scheme)
    );
    assert_eq!(
        judge(&policy(), &page("chrome://settings", BODY, 1)),
        Err(KeptRefusal::Scheme)
    );
    assert_eq!(
        judge(&policy(), &page("not a url", BODY, 1)),
        Err(KeptRefusal::Scheme)
    );
    assert_eq!(
        judge(&policy(), &page(CANON, " \n\t ", 1)),
        Err(KeptRefusal::Empty)
    );
    let mut private = page(CANON, BODY, 1);
    private.private = true;
    assert_eq!(judge(&policy(), &private), Err(KeptRefusal::Private));
    let mut off = policy();
    off.enabled = false;
    assert_eq!(
        judge(&off, &page(CANON, BODY, 1)),
        Err(KeptRefusal::Disabled)
    );

    pollster::block_on(async {
        let backend = MemoryBackend::new();
        let summary = record_kept_pages(
            &backend,
            &KeptBatch {
                policy: off,
                pages: vec![page(CANON, BODY, 1)],
            },
        )
        .await;
        assert_eq!(
            (summary.active, summary.kept, summary.refused),
            (false, 0, 1)
        );
        assert_eq!(backend.len(), 0, "a disabled control writes nothing");
    });
}

/// A store that cannot take writes, as IndexedDB reports a full quota.
#[derive(Clone)]
struct Full(MemoryBackend);

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl Backend for Full {
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, StoreError> {
        self.0.get(key).await
    }
    async fn put(&self, _: &str, _: &[u8]) -> Result<(), StoreError> {
        Err(StoreError::Backend("QuotaExceededError".into()))
    }
    async fn delete(&self, _: &str) -> Result<(), StoreError> {
        Err(StoreError::Backend("QuotaExceededError".into()))
    }
    async fn list(&self, prefix: &str) -> Result<Vec<String>, StoreError> {
        self.0.list(prefix).await
    }
    async fn scan(&self, start: &str, end: &str) -> Result<Vec<String>, StoreError> {
        self.0.scan(start, end).await
    }
    async fn apply(&self, _: &[WriteOp]) -> Result<(), StoreError> {
        Err(StoreError::Backend("QuotaExceededError".into()))
    }
    async fn transact(&self, _: TransactFn) -> Result<(), StoreError> {
        Err(StoreError::Backend("QuotaExceededError".into()))
    }
}

#[test]
fn a_full_store_is_reported_not_swallowed_and_a_retry_lands() {
    pollster::block_on(async {
        let backend = MemoryBackend::new();
        let full = Full(backend.clone());
        let pages = vec![
            page(PAGE, BODY, 1),
            page("https://garden.example/two", "second", 2),
        ];
        let failed = record_kept_pages(&full, &batch(pages.clone())).await;
        let error = failed.error.expect("the quota failure is reported");
        assert!(error.contains("QuotaExceeded"), "{error}");
        assert_eq!((failed.kept, failed.unchanged), (0, 0));
        assert_eq!(
            PageTextStore::new(backend.clone())
                .load_all()
                .await
                .unwrap()
                .len(),
            0
        );

        let retried = record(&backend, pages).await;
        assert_eq!((retried.kept, retried.error), (2, None));
    });
}

#[test]
fn forgetting_an_address_forgets_its_kept_text_and_versions() {
    pollster::block_on(async {
        let backend = MemoryBackend::new();
        record(
            &backend,
            vec![
                page(PAGE, BODY, 1),
                page("https://garden.example/two", "second", 2),
            ],
        )
        .await;
        forget_kept_text(&backend, PAGE).await.unwrap();
        let store = PageTextStore::new(backend.clone());
        assert_eq!(store.text_for(CANON).await.unwrap(), None);
        assert!(
            ResourceCaptureStore::new(&backend)
                .for_url(CANON)
                .await
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            store
                .text_for("https://garden.example/two")
                .await
                .unwrap()
                .as_deref(),
            Some("second")
        );
    });
}
