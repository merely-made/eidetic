// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Shared by the capture integration tests: Turnstone's fixture sealer, the
//! golden-fixture reader, and a stand-in for Fleece's annotation record.
#![allow(dead_code)]

use eidetic::capture::{AcquisitionEvidence, CaptureError, FetchedDocument, SourceAnnotator};
use eidetic::{Hash, ManifestId, PayloadSealer, SchemaRef, Store, Timestamp, TypedPayload};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

/// Turnstone's own test sealer: XOR over the cleartext behind the hash hex.
pub struct FixtureSealer(pub AtomicBool);

impl FixtureSealer {
    pub fn open() -> Self {
        Self(AtomicBool::new(true))
    }
    pub fn set_open(&self, open: bool) {
        self.0.store(open, Ordering::SeqCst);
    }
}

impl PayloadSealer for FixtureSealer {
    fn seal(
        &self,
        hash: &Hash,
        clear: &[u8],
    ) -> eidetic::Result<(Vec<u8>, eidetic::SealedBlobRef)> {
        if !self.0.load(Ordering::SeqCst) {
            return Err(eidetic::Error::new("locked"));
        }
        let mut ciphertext = hash.to_hex().into_bytes();
        ciphertext.extend(clear.iter().map(|b| b ^ 0x5a));
        Ok((
            ciphertext,
            eidetic::SealedBlobRef {
                epoch: eidetic::SealEpochId([7; 16]),
                format: "fixture".into(),
            },
        ))
    }

    fn unseal(
        &self,
        hash: &Hash,
        marker: &eidetic::SealedBlobRef,
        cipher: &[u8],
    ) -> eidetic::Result<Vec<u8>> {
        if !self.0.load(Ordering::SeqCst)
            || marker.format != "fixture"
            || !cipher.starts_with(hash.to_hex().as_bytes())
        {
            return Err(eidetic::Error::new("refused"));
        }
        let clear: Vec<u8> = cipher[64..].iter().map(|b| b ^ 0x5a).collect();
        if Hash::of(&clear) != *hash {
            return Err(eidetic::Error::new("tampered"));
        }
        Ok(clear)
    }
}

pub fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/capture_golden")
        .join(name)
}

/// The fixture meta file as JSON.
pub fn fixture_json(name: &str) -> Value {
    serde_json::from_slice(&std::fs::read(fixture_path(name)).unwrap()).unwrap()
}

/// A `*.kv.json` dump as sorted `(key, bytes)` pairs.
pub fn fixture_kv(name: &str) -> Vec<(String, Vec<u8>)> {
    let entries: Vec<Value> =
        serde_json::from_slice(&std::fs::read(fixture_path(name)).unwrap()).unwrap();
    entries
        .into_iter()
        .map(|entry| {
            let hex = entry["hex"].as_str().unwrap();
            let bytes = (0..hex.len() / 2)
                .map(|i| u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).unwrap())
                .collect();
            (entry["key"].as_str().unwrap().to_owned(), bytes)
        })
        .collect()
}

pub async fn load_into(store: &dyn Store, kv: &[(String, Vec<u8>)]) {
    for (key, value) in kv {
        store.put(key, value).await.unwrap();
    }
}

/// Every key and value in a store, sorted by key.
pub async fn dump(store: &dyn Store) -> Vec<(String, Vec<u8>)> {
    let mut keys = store.list("").await.unwrap();
    keys.sort();
    let mut out = Vec::new();
    for key in keys {
        let value = store.get(&key).await.unwrap().unwrap();
        out.push((key, value));
    }
    out
}

/// The golden document the sealed and unsealed captures were taken from.
pub fn golden_document() -> FetchedDocument {
    let meta = fixture_json("sealed_meta.json");
    let doc = &meta["document"];
    let body = doc["body"].as_str().unwrap().to_owned();
    FetchedDocument {
        bytes: body.as_bytes().to_vec(),
        body,
        effective_url: doc["effective_url"].as_str().map(str::to_owned),
        content_type: doc["content_type"].as_str().map(str::to_owned),
        acquired_at_ms: doc["acquired_at_ms"].as_u64().unwrap(),
    }
}

pub fn manifest_id(value: &str) -> ManifestId {
    serde_json::from_value(Value::String(value.to_owned())).unwrap()
}

/// The annotation bytes exactly as stored, standing in for Fleece's
/// `FleeceAnnotationRecord`, which Eidetic cannot name. Serialization is the
/// stored bytes, so a record read from a fixture re-encodes to itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Annotation(pub Vec<u8>);

impl Serialize for Annotation {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_bytes(&self.0)
    }
}
impl<'de> Deserialize<'de> for Annotation {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Vec::<u8>::deserialize(deserializer).map(Self)
    }
}

pub const ANNOTATION_SCHEMA: &str =
    "blake3:ce05fe6b2faf7ea7aa7c5255fb8b40afcaee199bce6d88e1960180733acc13c0";

impl TypedPayload for Annotation {
    fn schema_ref() -> SchemaRef {
        SchemaRef::from_id(manifest_id(ANNOTATION_SCHEMA))
    }
    fn serialize_to_bytes(&self) -> eidetic::Result<Vec<u8>> {
        Ok(self.0.clone())
    }
    fn deserialize_from_bytes(bytes: &[u8]) -> eidetic::Result<Self> {
        Ok(Self(bytes.to_vec()))
    }
}

impl Annotation {
    fn json(&self) -> Value {
        serde_json::from_slice(&self.0).unwrap_or(Value::Null)
    }
}

/// Reads and builds Fleece-shaped annotations over `serde_json`. For the golden
/// document it returns the fixture's exact bytes; for any other document it
/// builds a record of the same shape.
pub struct JsonAnnotator {
    golden: Annotation,
    schema_kv: Vec<(String, Vec<u8>)>,
}

impl JsonAnnotator {
    pub fn new() -> Self {
        let kv = fixture_kv("source_document.kv.json");
        let get = |key: &str| kv.iter().find(|(k, _)| k == key).unwrap().1.clone();
        let sd = fixture_json("source_document_meta.json");
        let annotation_id = sd["stored"]["annotation_manifest"]
            .as_str()
            .unwrap()
            .to_owned();
        let hex = annotation_id.strip_prefix("blake3:").unwrap();
        let golden = Annotation(get(&format!("blob:{hex}")));
        let schema_hex = ANNOTATION_SCHEMA.strip_prefix("blake3:").unwrap();
        let schema_kv = ["blob", "manifest"]
            .iter()
            .map(|p| {
                let key = format!("{p}:{schema_hex}");
                let value = get(&key);
                (key, value)
            })
            .collect();
        Self { golden, schema_kv }
    }
}

impl Default for JsonAnnotator {
    fn default() -> Self {
        Self::new()
    }
}

fn text_of(body: &str) -> String {
    // Enough of a projection for tests: the text between tags.
    let mut out = String::new();
    let mut in_tag = false;
    for c in body.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    out
}

#[async_trait::async_trait(?Send)]
impl SourceAnnotator for JsonAnnotator {
    type Record = Annotation;

    fn schema_manifest(&self) -> ManifestId {
        manifest_id(ANNOTATION_SCHEMA)
    }

    fn annotate(&self, document: &FetchedDocument) -> Result<Annotation, CaptureError> {
        if document.body == golden_document().body {
            return Ok(self.golden.clone());
        }
        let raw = ManifestId::of_blob(&document.bytes);
        let text = text_of(&document.body);
        let url = document.effective_url.clone().ok_or("no url")?;
        let record = json!({
            "extraction": {
                "capture": {
                    "canonical_source": url,
                    "capture_hash": raw.to_string(),
                    "evidence": {
                        "version": 1,
                        "final_source": url,
                        "capture_hash": raw.to_string(),
                        "captured_at": document.acquired_at_ms,
                        "response_content_type": document.content_type,
                        "dom_mode": "source",
                        "raw_manifest": raw.to_string(),
                        "replay_manifest": null,
                    },
                },
                "canonical_text_hash": Hash::of(text.as_bytes()).to_string(),
                "canonical_text_record": { "canonical_text": text },
            },
        });
        Ok(Annotation(serde_json::to_vec(&record).unwrap()))
    }

    fn validate(&self, record: &Annotation) -> Result<(), CaptureError> {
        let value = record.json();
        let text = value["extraction"]["canonical_text_record"]["canonical_text"]
            .as_str()
            .ok_or("no canonical text")?;
        if value["extraction"]["canonical_text_hash"].as_str()
            != Some(Hash::of(text.as_bytes()).to_string().as_str())
        {
            return Err("canonical text hash mismatch".into());
        }
        Ok(())
    }

    fn evidence(&self, record: &Annotation) -> Option<AcquisitionEvidence> {
        let value = record.json();
        let e = &value["extraction"]["capture"]["evidence"];
        if e.is_null() {
            return None;
        }
        let id = |v: &Value| v.as_str().map(manifest_id);
        let raw_manifest = id(&e["raw_manifest"]);
        let capture_hash = id(&e["capture_hash"])?.0;
        Some(AcquisitionEvidence {
            final_source: e["final_source"].as_str()?.to_owned(),
            capture_hash,
            captured_at: Timestamp(e["captured_at"].as_u64()?),
            response_content_type: e["response_content_type"].as_str().map(str::to_owned),
            raw_manifest,
            source_dom: e["dom_mode"] == "source",
            has_replay_manifest: !e["replay_manifest"].is_null(),
            authoritative: e["version"] == 1 && e["captured_at"].as_u64().unwrap_or(0) > 0,
        })
    }

    fn association(&self, record: &Annotation) -> (String, Hash) {
        let value = record.json();
        let capture = &value["extraction"]["capture"];
        (
            capture["canonical_source"]
                .as_str()
                .unwrap_or_default()
                .to_owned(),
            manifest_id(capture["capture_hash"].as_str().unwrap_or_default()).0,
        )
    }

    async fn bootstrap_schema(&self, store: &mut dyn Store) -> Result<(), CaptureError> {
        for (key, value) in &self.schema_kv {
            store.put(key, value).await?;
        }
        Ok(())
    }

    async fn save(
        &self,
        store: &mut dyn Store,
        record: &Annotation,
        at_ms: u64,
    ) -> Result<(), CaptureError> {
        let ts = Timestamp(at_ms);
        eidetic::save_typed(
            store,
            record,
            Vec::new(),
            eidetic::PrivacyClass::LocalOnly,
            eidetic::ProvenanceRecord {
                origin: eidetic::ProvenanceOrigin::Generated,
                upstream: Vec::new(),
                tooling: Some("mere-document-lanes-fleece/0.0.1".into()),
                generated_at: ts,
            },
            eidetic::TrustEnvelope {
                level: eidetic::TrustLevel::SelfAsserted,
                signatures: Vec::new(),
                moderation_state: eidetic::ModerationState::Unreviewed,
            },
            ts,
        )
        .await?;
        let (source, hash) = self.association(record);
        eidetic::ResourceCaptureStore::new(&*store)
            .record(&source, hash, eidetic::CaptureContent::Acquired, at_ms)
            .await?;
        Ok(())
    }

    async fn load(
        &self,
        store: &mut dyn Store,
        id: ManifestId,
    ) -> Result<Option<Annotation>, CaptureError> {
        let record = eidetic::load_typed::<Annotation>(store, &mut eidetic::NoFetcher, id).await?;
        if let Some(record) = &record {
            self.validate(record)?;
        }
        Ok(record)
    }
}

/// Backend wrapper: counts writes and can fail immediately before, or after,
/// one real write (the latter models a write that committed before its
/// acknowledgement was lost). Also supplies the bounded-read seam.
pub struct FaultStore<B> {
    pub inner: B,
    pub writes: std::sync::atomic::AtomicUsize,
    pub fail_at: Option<usize>,
    pub after_write: bool,
}

impl<B> FaultStore<B> {
    pub fn new(inner: B, fail_at: Option<usize>, after_write: bool) -> Self {
        Self {
            inner,
            writes: std::sync::atomic::AtomicUsize::new(0),
            fail_at,
            after_write,
        }
    }
    pub fn writes(&self) -> usize {
        self.writes.load(Ordering::SeqCst)
    }
}

#[async_trait::async_trait]
impl<B: eidetic::Backend + Send + Sync> eidetic::Backend for FaultStore<B> {
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, muniment::StoreError> {
        self.inner.get(key).await
    }
    async fn put(&self, key: &str, bytes: &[u8]) -> Result<(), muniment::StoreError> {
        let write = self.writes.fetch_add(1, Ordering::SeqCst) + 1;
        if self.fail_at == Some(write) && !self.after_write {
            return Err(muniment::StoreError::Backend(format!(
                "injected deposit write {write}: {key}"
            )));
        }
        self.inner.put(key, bytes).await?;
        if self.fail_at == Some(write) && self.after_write {
            return Err(muniment::StoreError::Backend(format!(
                "injected deposit write {write} acknowledged after commit: {key}"
            )));
        }
        Ok(())
    }
    async fn delete(&self, key: &str) -> Result<(), muniment::StoreError> {
        self.inner.delete(key).await
    }
    async fn list(&self, prefix: &str) -> Result<Vec<String>, muniment::StoreError> {
        self.inner.list(prefix).await
    }
    async fn scan(&self, start: &str, end: &str) -> Result<Vec<String>, muniment::StoreError> {
        self.inner.scan(start, end).await
    }
    async fn apply(&self, ops: &[eidetic::WriteOp]) -> Result<(), muniment::StoreError> {
        for op in ops {
            match op {
                eidetic::WriteOp::Put { key, value } => self.put(key, value).await?,
                eidetic::WriteOp::Delete { key } => self.delete(key).await?,
            }
        }
        Ok(())
    }
}

#[async_trait::async_trait(?Send)]
impl<B: eidetic::Backend + Send + Sync> eidetic::capture::BoundedRead for FaultStore<B> {
    async fn get_limited(
        &self,
        key: &str,
        max_value_bytes: usize,
    ) -> Result<Option<Vec<u8>>, muniment::StoreError> {
        let value = self.inner.get(key).await?;
        if value.as_ref().is_some_and(|v| v.len() > max_value_bytes) {
            return Err(muniment::StoreError::LimitExceeded {
                kind: muniment::StoreLimitKind::ValueBytes,
                limit: u64::try_from(max_value_bytes).unwrap_or(u64::MAX),
            });
        }
        Ok(value)
    }
}

/// Counts unseals, so a test can show a refusal came before custody was asked.
pub struct CountingSealer<S> {
    pub inner: S,
    pub unseals: std::sync::atomic::AtomicUsize,
}

impl<S> CountingSealer<S> {
    pub fn new(inner: S) -> Self {
        Self {
            inner,
            unseals: std::sync::atomic::AtomicUsize::new(0),
        }
    }
    pub fn unseals(&self) -> usize {
        self.unseals.load(Ordering::SeqCst)
    }
}

impl<S: PayloadSealer> PayloadSealer for CountingSealer<S> {
    fn seal(
        &self,
        hash: &Hash,
        clear: &[u8],
    ) -> eidetic::Result<(Vec<u8>, eidetic::SealedBlobRef)> {
        self.inner.seal(hash, clear)
    }
    fn unseal(
        &self,
        hash: &Hash,
        marker: &eidetic::SealedBlobRef,
        cipher: &[u8],
    ) -> eidetic::Result<Vec<u8>> {
        self.unseals.fetch_add(1, Ordering::SeqCst);
        self.inner.unseal(hash, marker, cipher)
    }
}

/// Refuses reads and listings like a store whose read budget is spent.
pub struct BudgetSpent<B> {
    pub inner: B,
    pub refuse_get: AtomicBool,
    pub refuse_list: AtomicBool,
}

impl<B> BudgetSpent<B> {
    pub fn new(inner: B) -> Self {
        Self {
            inner,
            refuse_get: AtomicBool::new(false),
            refuse_list: AtomicBool::new(false),
        }
    }
}

#[async_trait::async_trait]
impl<B: eidetic::Backend + Send + Sync> eidetic::Backend for BudgetSpent<B> {
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, muniment::StoreError> {
        if self.refuse_get.load(Ordering::SeqCst) {
            return Err(muniment::StoreError::LimitExceeded {
                kind: muniment::StoreLimitKind::ValueBytes,
                limit: 0,
            });
        }
        self.inner.get(key).await
    }
    async fn put(&self, key: &str, bytes: &[u8]) -> Result<(), muniment::StoreError> {
        self.inner.put(key, bytes).await
    }
    async fn delete(&self, key: &str) -> Result<(), muniment::StoreError> {
        self.inner.delete(key).await
    }
    async fn list(&self, prefix: &str) -> Result<Vec<String>, muniment::StoreError> {
        if self.refuse_list.load(Ordering::SeqCst) {
            return Err(muniment::StoreError::LimitExceeded {
                kind: muniment::StoreLimitKind::VisitedItems,
                limit: 0,
            });
        }
        self.inner.list(prefix).await
    }
    async fn scan(&self, start: &str, end: &str) -> Result<Vec<String>, muniment::StoreError> {
        self.inner.scan(start, end).await
    }
    async fn apply(&self, ops: &[eidetic::WriteOp]) -> Result<(), muniment::StoreError> {
        self.inner.apply(ops).await
    }
}
