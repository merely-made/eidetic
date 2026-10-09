// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Immutable content references under the common resource identity.
//! Acquired representations and extracted text remain distinct evidence.

use muniment::Backend;
use serde::{Deserialize, Serialize};

use crate::{Error, Hash, Result};

const PREFIX: &str = "resource-capture/";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureContent {
    Acquired,
    PageText,
}

impl CaptureContent {
    fn key(self) -> &'static str {
        match self {
            Self::Acquired => "acquired",
            Self::PageText => "page-text",
        }
    }
}

/// A content version belongs to a resource, independently of its surfaces.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceCaptureRef {
    /// The common Chartulary UUIDv5, serialized in its canonical string form.
    pub resource_id: String,
    pub canonical_iri: String,
    pub content_hash: Hash,
    pub content: CaptureContent,
    pub stored_at_ms: u64,
}

impl ResourceCaptureRef {
    fn key(&self) -> String {
        format!(
            "{PREFIX}{}/{}/{}",
            self.resource_id,
            self.content.key(),
            self.content_hash.to_hex()
        )
    }

    fn validate(&self, key: &str) -> Result<()> {
        if self.canonical_iri != chartulary::canonical_url(&self.canonical_iri)
            || self.resource_id != chartulary::resource_id(&self.canonical_iri).to_string()
            || self.key() != key
        {
            return Err(Error::new(
                "capture reference does not match its resource/content key",
            ));
        }
        Ok(())
    }
}

/// Addressable references beside existing blobs/manifests; no bytes are copied.
pub struct ResourceCaptureStore<'a, B: ?Sized> {
    backend: &'a B,
}

impl<'a, B: Backend + ?Sized> ResourceCaptureStore<'a, B> {
    pub fn new(backend: &'a B) -> Self {
        Self { backend }
    }

    async fn load(&self, key: &str) -> Result<Option<ResourceCaptureRef>> {
        self.backend
            .get(key)
            .await?
            .map(|bytes| {
                serde_json::from_slice(&bytes).map_err(|error| Error::new(error.to_string()))
            })
            .transpose()
    }

    /// Retain an observation of this version, reusing an existing reference.
    /// The caller supplies a hash of the representation it actually retained.
    pub async fn record(
        &self,
        url: &str,
        hash: Hash,
        content: CaptureContent,
        stored_at_ms: u64,
    ) -> Result<ResourceCaptureRef> {
        let canonical_iri = chartulary::canonical_url(url);
        let entry = ResourceCaptureRef {
            resource_id: chartulary::resource_id(&canonical_iri).to_string(),
            canonical_iri,
            content_hash: hash,
            content,
            stored_at_ms,
        };
        let key = entry.key();
        if let Some(existing) = self.load(&key).await? {
            existing.validate(&key)?;
            return Ok(existing);
        }
        let bytes = serde_json::to_vec(&entry).map_err(|error| Error::new(error.to_string()))?;
        self.backend.put(&key, &bytes).await?;
        Ok(entry)
    }

    pub async fn for_resource(&self, resource_id: &str) -> Result<Vec<ResourceCaptureRef>> {
        let mut entries = Vec::new();
        for key in self
            .backend
            .list(&format!("{PREFIX}{resource_id}/"))
            .await?
        {
            if let Some(entry) = self.load(&key).await? {
                entry.validate(&key)?;
                entries.push(entry);
            }
        }
        entries.sort_by_key(|entry| (entry.stored_at_ms, entry.key()));
        Ok(entries)
    }

    pub async fn for_url(&self, url: &str) -> Result<Vec<ResourceCaptureRef>> {
        self.for_resource(&chartulary::resource_id(url).to_string())
            .await
    }

    /// Remove references of the selected content kind; retained bytes use the existing GC.
    pub async fn forget(&self, url: &str, content: CaptureContent) -> Result<()> {
        for entry in self.for_url(url).await? {
            if entry.content == content {
                self.backend.delete(&entry.key()).await?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use muniment::{JsonSlots, MemoryBackend};

    #[test]
    fn aliases_share_versions_and_reopen_keeps_earlier_resources() {
        pollster::block_on(async {
            let backend = MemoryBackend::new();
            let store = ResourceCaptureStore::new(&backend);
            let first = store
                .record(
                    "https://Example.test/page?utm_source=x#part",
                    Hash::of(b"first"),
                    CaptureContent::Acquired,
                    1,
                )
                .await
                .unwrap();
            assert_eq!(
                store
                    .record(
                        "https://example.test/page",
                        first.content_hash,
                        first.content,
                        9
                    )
                    .await
                    .unwrap(),
                first
            );
            store
                .record(
                    "https://example.test/page",
                    Hash::of(b"second"),
                    CaptureContent::Acquired,
                    2,
                )
                .await
                .unwrap();
            store
                .record(
                    "https://example.test/next",
                    first.content_hash,
                    CaptureContent::Acquired,
                    3,
                )
                .await
                .unwrap();
            let reopened = ResourceCaptureStore::new(&backend);
            let versions = reopened.for_resource(&first.resource_id).await.unwrap();
            assert_eq!(versions.len(), 2);
            assert_eq!(versions[0], first);
            assert_ne!(versions[0].content_hash, versions[1].content_hash);
            assert_eq!(
                reopened
                    .for_url("https://example.test/next")
                    .await
                    .unwrap()
                    .len(),
                1
            );
            assert!(
                reopened
                    .for_url("https://absent.test")
                    .await
                    .unwrap()
                    .is_empty()
            );
        });
    }

    #[test]
    fn text_is_distinct_evidence_and_damaged_reference_is_refused() {
        pollster::block_on(async {
            let backend = MemoryBackend::new();
            let store = ResourceCaptureStore::new(&backend);
            let hash = Hash::of(b"same bytes");
            let acquired = store
                .record("https://example.test", hash, CaptureContent::Acquired, 1)
                .await
                .unwrap();
            store
                .record("https://example.test", hash, CaptureContent::PageText, 1)
                .await
                .unwrap();
            assert_eq!(
                store.for_url("https://example.test").await.unwrap().len(),
                2
            );
            let mut damaged = acquired.clone();
            damaged.resource_id = chartulary::resource_id("https://other.test").to_string();
            JsonSlots::new(&backend)
                .save(&acquired.key(), &damaged)
                .await
                .unwrap();
            assert!(store.for_url("https://example.test").await.is_err());
            JsonSlots::new(&backend)
                .save(&acquired.key(), &acquired)
                .await
                .unwrap();
            assert_eq!(
                store.for_url("https://example.test").await.unwrap().len(),
                2
            );
        });
    }
}
