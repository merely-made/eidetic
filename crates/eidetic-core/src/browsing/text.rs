// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Durable page text — the body the host extracted, kept where the page table
//! and the lexical index can read it back (wiring plan W6c).
//!
//! A thin adapter over the two muniment stores eidetic already sits on: the
//! **blob** is the storage (content-addressed, so one body under two addresses
//! is stored once and a re-visit that changed nothing writes nothing), and one
//! **slot per address** is the index. Nothing else is invented here — no
//! second authority, no cache format.
//!
//! Keys: text bytes at muniment's own `blob/<blake3-hex>`; the index at
//! `page-text/<blake3-hex of the canonical URL>`. The slot key is hashed
//! rather than spelled because a URL carries `/`, `?` and `#`, which a
//! path-shaped backend would read as structure.
//!
//! What may be stored is a consent question (capture plan C4), not this
//! module's: it stores what it is handed.

use std::collections::BTreeMap;

use muniment::{Backend, BlobStore, Hash, JsonSlots};
use serde::{Deserialize, Serialize};

use super::page::canonical_url;
use crate::Result;

/// Slot namespace for the address index.
const SLOT_PREFIX: &str = "page-text/";

fn slot_key(canonical: &str) -> String {
    format!("{SLOT_PREFIX}{}", Hash::of(canonical.as_bytes()).to_hex())
}

/// One address's index entry: which blob holds its text, and when it landed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageTextRef {
    /// The [`canonical_url`] this entry indexes. Stored in the slot because
    /// the key is a hash: a listing has to recover the address from somewhere.
    pub url: String,
    /// blake3 hex of the stored text bytes — the blob address.
    pub blob: String,
    pub stored_at_ms: u64,
}

/// Page text over a [`Backend`]: blob for the bytes, slot for the address.
///
/// `B` is cloned once per store at construction, which is what muniment's
/// backends are for (a shared handle). A borrowed backend is a backend, so
/// `PageTextStore::new(&store)` layers this over a store the caller owns.
pub struct PageTextStore<B> {
    blobs: BlobStore<B>,
    slots: JsonSlots<B>,
}

impl<B: Backend + Clone> PageTextStore<B> {
    pub fn new(backend: B) -> Self {
        Self {
            blobs: BlobStore::new(backend.clone()),
            slots: JsonSlots::new(backend),
        }
    }

    /// Store `text` as the body of the page at `url`, returning its blob hash.
    /// Idempotent: an unchanged body under a known address writes nothing.
    pub async fn put(&self, url: &str, text: &str, stored_at_ms: u64) -> Result<Hash> {
        let canonical = canonical_url(url);
        let hash = Hash::of(text.as_bytes());
        let key = slot_key(&canonical);
        let current: Option<PageTextRef> = self.slots.load(&key).await?;
        if current.is_some_and(|entry| entry.blob == hash.to_hex()) && self.blobs.has(&hash).await?
        {
            return Ok(hash);
        }
        self.blobs.put(text.as_bytes()).await?;
        self.slots
            .save(
                &key,
                &PageTextRef {
                    url: canonical,
                    blob: hash.to_hex(),
                    stored_at_ms,
                },
            )
            .await?;
        Ok(hash)
    }

    /// The stored body of the page at `url`, or `None` if none was kept. The
    /// address is canonicalized here, so a caller may pass a raw one.
    pub async fn text_for(&self, url: &str) -> Result<Option<String>> {
        let Some(entry) = self.reference(url).await? else {
            return Ok(None);
        };
        self.text_of(&entry).await
    }

    /// The index entry for `url`, without reading the body.
    pub async fn reference(&self, url: &str) -> Result<Option<PageTextRef>> {
        Ok(self.slots.load(&slot_key(&canonical_url(url))).await?)
    }

    /// Forget the page at `url`. The blob stays for the GC pass, the way a
    /// deleted manifest's bytes do — another address may share it.
    pub async fn forget(&self, url: &str) -> Result<()> {
        Ok(self.slots.delete(&slot_key(&canonical_url(url))).await?)
    }

    /// Every stored body, canonical URL to text — the snapshot a projection
    /// reads once and then closes over. One store round trip per address, so
    /// a caller that wants many reads this rather than calling
    /// [`text_for`](Self::text_for) in a loop.
    pub async fn load_all(&self) -> Result<PageTexts> {
        let mut texts = BTreeMap::new();
        for key in self.slots.keys(SLOT_PREFIX).await? {
            let Some(entry): Option<PageTextRef> = self.slots.load(&key).await? else {
                continue;
            };
            if let Some(text) = self.text_of(&entry).await? {
                texts.insert(entry.url, text);
            }
        }
        Ok(PageTexts(texts))
    }

    /// The body an index entry points at. A missing blob is `None`, not an
    /// error: the index outlives a GC'd body, and a page with no text reads
    /// the same as a page never captured.
    async fn text_of(&self, entry: &PageTextRef) -> Result<Option<String>> {
        let Some(hash) = Hash::from_hex(&entry.blob) else {
            return Ok(None);
        };
        Ok(self
            .blobs
            .get(&hash)
            .await?
            .and_then(|bytes| String::from_utf8(bytes).ok()))
    }
}

/// Every stored page body, keyed by [`canonical_url`] — the read-once
/// snapshot the projections take their text from.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PageTexts(BTreeMap<String, String>);

impl PageTexts {
    /// Adopt an already-built map. The keys must be canonical URLs.
    pub fn from_canonical(texts: BTreeMap<String, String>) -> Self {
        Self(texts)
    }

    /// The body for an address, raw or canonical.
    pub fn get(&self, url: &str) -> Option<&str> {
        self.0.get(&canonical_url(url)).map(String::as_str)
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The `text_for` closure [`page_table`](super::page::page_table) and
    /// `TrailIndex::rebuild_with_text` take.
    pub fn lookup(&self) -> impl Fn(&str) -> Option<String> + '_ {
        move |url: &str| self.get(url).map(str::to_string)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::browsing::page::{FingerprintSource, page_table};
    use crate::browsing::{BrowsingTrace, PageRef, TraceEvent, TraceTransition};
    use muniment::MemoryBackend;

    const BODY: &str = "The kestrel hovers on a fixed point of air, head still while the \
        wings work, and drops only when the vole below commits to a run.";

    fn store() -> PageTextStore<MemoryBackend> {
        PageTextStore::new(MemoryBackend::new())
    }

    #[test]
    fn put_then_text_for_round_trips_through_the_canonical_key() {
        pollster::block_on(async {
            let store = store();
            store
                .put("https://Bird.example/kestrel?utm_source=x#top", BODY, 7)
                .await
                .unwrap();
            // Read back under a different spelling of the same page.
            assert_eq!(
                store
                    .text_for("https://bird.example/kestrel")
                    .await
                    .unwrap(),
                Some(BODY.to_string())
            );
            let entry = store
                .reference("https://bird.example/kestrel")
                .await
                .unwrap()
                .unwrap();
            assert_eq!(entry.url, "https://bird.example/kestrel");
            assert_eq!(entry.blob, Hash::of(BODY.as_bytes()).to_hex());
            assert_eq!(entry.stored_at_ms, 7);
            assert_eq!(
                store.text_for("https://other.example/").await.unwrap(),
                None
            );
        });
    }

    /// The blob is the storage: one body under two addresses is stored once,
    /// and a re-put of the same body writes nothing new.
    #[test]
    fn the_body_is_stored_once_however_many_addresses_reach_it() {
        pollster::block_on(async {
            let backend = MemoryBackend::new();
            let store = PageTextStore::new(backend.clone());
            store
                .put("https://origin.example/post", BODY, 1)
                .await
                .unwrap();
            store
                .put("https://mirror.example/post", BODY, 2)
                .await
                .unwrap();
            store
                .put("https://origin.example/post", BODY, 3)
                .await
                .unwrap();
            // One blob, two slots.
            assert_eq!(backend.len(), 3);
            let entry = store
                .reference("https://origin.example/post")
                .await
                .unwrap()
                .unwrap();
            assert_eq!(entry.stored_at_ms, 1, "an unchanged body is not rewritten");

            // A changed body is a new blob and a fresh stamp.
            store
                .put("https://origin.example/post", "different words", 4)
                .await
                .unwrap();
            assert_eq!(backend.len(), 4);
            let entry = store
                .reference("https://origin.example/post")
                .await
                .unwrap()
                .unwrap();
            assert_eq!(entry.stored_at_ms, 4);
            assert_eq!(
                store
                    .text_for("https://origin.example/post")
                    .await
                    .unwrap()
                    .as_deref(),
                Some("different words")
            );
        });
    }

    /// Per-URL slot writes stay per-URL: a second page does not rewrite the
    /// first one's index entry (the single-map-slot failure this avoids).
    #[test]
    fn a_second_page_leaves_the_first_slot_alone() {
        pollster::block_on(async {
            let store = store();
            store
                .put("https://a.example/", "alpha text", 1)
                .await
                .unwrap();
            store
                .put("https://b.example/", "beta text", 2)
                .await
                .unwrap();
            let texts = store.load_all().await.unwrap();
            assert_eq!(texts.len(), 2);
            assert_eq!(texts.get("https://a.example/"), Some("alpha text"));
            assert_eq!(texts.get("https://b.example/"), Some("beta text"));

            store.forget("https://a.example/").await.unwrap();
            assert_eq!(store.load_all().await.unwrap().len(), 1);
        });
    }

    /// The point of the whole module: the snapshot's closure is the shape
    /// `page_table` takes, and supplying it turns URL-keyed pages into
    /// content-keyed ones.
    #[test]
    fn the_snapshot_supplies_page_tables_text() {
        pollster::block_on(async {
            let store = store();
            store
                .put("https://bird.example/kestrel", BODY, 1)
                .await
                .unwrap();
            let texts = store.load_all().await.unwrap();

            let event = TraceEvent {
                from: None,
                to: PageRef {
                    url: "https://bird.example/kestrel?utm_source=news".to_string(),
                    title: None,
                },
                transition: TraceTransition::LinkClick,
                at_ms: 10,
                dwell_ms: None,
                candidates: Vec::new(),
            };
            let traces = vec![BrowsingTrace::from_events("p", vec![event])];

            let table = page_table(&traces, texts.lookup());
            let record = table.records.values().next().unwrap();
            assert_eq!(record.fingerprint.source, FingerprintSource::Text);
            assert_eq!(record.text.as_deref(), Some(BODY));

            let bare = page_table(&traces, PageTexts::default().lookup());
            assert_eq!(
                bare.records.values().next().unwrap().fingerprint.source,
                FingerprintSource::CanonicalUrl,
                "no stored text still yields the honest URL fallback"
            );
        });
    }
}
