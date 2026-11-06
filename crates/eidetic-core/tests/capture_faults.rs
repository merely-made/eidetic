// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Fault-at-each-write and integrity tests for the promoted capture core,
//! moved from Turnstone's `trail_memory` suites and run over a memory backend
//! and every durable muniment backend (directory, fjall, redb).
//!
//! The capture path uses ordered single `put`s and content-addressed
//! idempotent artifacts, never `transact`, so a failure (or a write whose
//! acknowledgement is lost) after any single write must leave the earlier
//! deposit verifiable. Unsealed captures then converge on retry. Sealed
//! captures converge too, except in the one window where a sealed blob landed
//! without its manifest (or the reverse): the sealed namespace never adopts a
//! half-written artifact, so the retry refuses with "requires repair" and
//! leaves the store as it found it.
//!
//! The directory backend is not exercised: it rejects `:` in keys, which every
//! Eidetic `blob:` and `manifest:` key carries.

#![allow(clippy::too_many_lines)]

mod support;

use eidetic::capture::{
    self, CaptureRequest, FetchedDocument, SourceIdentity, StoredSourceDocument, sealed,
};
use eidetic::{Backend, MemoryBackend};
use std::path::Path;
use support::*;

/// A backend that can be dropped and opened again over the same bytes.
trait Durability {
    type Store: Backend + Send + Sync;
    const NAME: &'static str;
    fn open(path: &Path) -> Self::Store;
}

struct Memory;
struct Fjall;
struct Redb;

/// Memory has no path: "reopening" carries committed bytes into a new store,
/// which is exactly what a restart preserves.
type Kv = Vec<(String, Vec<u8>)>;
static MEMORY_DISK: std::sync::Mutex<Vec<(std::path::PathBuf, Kv)>> =
    std::sync::Mutex::new(Vec::new());

impl Durability for Memory {
    type Store = MemoryBackend;
    const NAME: &'static str = "memory";
    fn open(path: &Path) -> MemoryBackend {
        let store = MemoryBackend::new();
        let saved = MEMORY_DISK
            .lock()
            .unwrap()
            .iter()
            .find(|(p, _)| p == path)
            .map(|(_, kv)| kv.clone());
        if let Some(kv) = saved {
            pollster::block_on(load_into(&store, &kv));
        }
        store
    }
}
impl Durability for Fjall {
    type Store = muniment::FjallStore;
    const NAME: &'static str = "fjall";
    fn open(path: &Path) -> Self::Store {
        muniment::FjallStore::open(path).unwrap()
    }
}
impl Durability for Redb {
    type Store = muniment::RedbBackend;
    const NAME: &'static str = "redb";
    fn open(path: &Path) -> Self::Store {
        muniment::RedbBackend::open(path.join("store.redb")).unwrap()
    }
}

/// Close a store: the memory variant snapshots its committed bytes first.
fn close<D: Durability>(path: &Path, store: D::Store) {
    if D::NAME == "memory" {
        let kv = pollster::block_on(dump(&store));
        let mut disk = MEMORY_DISK.lock().unwrap();
        disk.retain(|(p, _)| p != path);
        disk.push((path.to_path_buf(), kv));
    }
    drop(store);
}

fn document(label: &str) -> FetchedDocument {
    let body = format!("<html><main><p>{label} naïve kestrel</p></main></html>");
    FetchedDocument {
        bytes: body.as_bytes().to_vec(),
        body,
        content_type: Some("text/html; charset=utf-8".into()),
        effective_url: Some(format!("https://capture.test/{label}")),
        acquired_at_ms: 1_700_000_000_001,
    }
}

fn request(n: u128, document: &FetchedDocument) -> CaptureRequest {
    CaptureRequest {
        id: uuid::Uuid::from_u128(n),
        session: uuid::Uuid::from_u128(0x5e55),
        graph: uuid::Uuid::from_u128(0x97a9),
        node: uuid::Uuid::from_u128(n + 0x100),
        url: "https://capture.test/candidate".into(),
        source: Some(SourceIdentity::of(document)),
        keep: false,
    }
}

async fn capture_plain<S: Backend + Send + Sync>(
    store: S,
    annotator: &JsonAnnotator,
    document: &FetchedDocument,
) -> (Result<StoredSourceDocument, capture::CaptureError>, S) {
    let mut store = store;
    let out = capture::capture_source_document_in(&mut store, annotator, document).await;
    (out, store)
}

fn deposit_failure_at_each_write<D: Durability>() {
    let annotator = JsonAnnotator::new();
    let baseline = tempfile::tempdir().unwrap();
    let mut counted = FaultStore::new(D::open(baseline.path()), None, false);
    let previous = pollster::block_on(capture::capture_source_document_in(
        &mut counted,
        &annotator,
        &document("previous"),
    ))
    .unwrap();
    let writes_before = counted.writes();
    let expected = pollster::block_on(capture::capture_source_document_in(
        &mut counted,
        &annotator,
        &document("candidate"),
    ))
    .unwrap();
    let write_count = counted.writes() - writes_before;
    assert!(
        write_count >= 5,
        "{}: raw/annotation blobs, manifests, association",
        D::NAME
    );
    close::<D>(baseline.path(), counted.inner);

    for (fail_at, after_write) in (1..=write_count).flat_map(|w| [(w, false), (w, true)]) {
        let root = tempfile::tempdir().unwrap();
        let mut seed = D::open(root.path());
        let seeded = pollster::block_on(capture::capture_source_document_in(
            &mut seed,
            &annotator,
            &document("previous"),
        ))
        .unwrap();
        assert_eq!(seeded, previous);
        let failing = FaultStore::new(seed, Some(fail_at), after_write);
        let (outcome, failing) =
            pollster::block_on(capture_plain(failing, &annotator, &document("candidate")));
        let error = outcome.unwrap_err();
        assert!(
            error.to_string().contains("injected deposit write"),
            "{} write {fail_at}: {error}",
            D::NAME
        );
        close::<D>(root.path(), failing.inner);

        let mut reopened = D::open(root.path());
        pollster::block_on(async {
            capture::verify_source_document(&mut reopened, &annotator, &previous)
                .await
                .unwrap();
            let retried = capture::capture_source_document_in(
                &mut reopened,
                &annotator,
                &document("candidate"),
            )
            .await
            .unwrap();
            assert_eq!(
                retried,
                expected,
                "{} retry identity after write {fail_at}",
                D::NAME
            );
            assert_eq!(
                capture::capture_source_document_in(
                    &mut reopened,
                    &annotator,
                    &document("candidate")
                )
                .await
                .unwrap(),
                expected
            );
            capture::verify_source_document(&mut reopened, &annotator, &previous)
                .await
                .unwrap();
            let annotations = eidetic::list_typed::<Annotation>(&mut reopened)
                .await
                .unwrap();
            assert_eq!(
                annotations.len(),
                2,
                "{} retry invents no artifact after write {fail_at}",
                D::NAME
            );
        });
        close::<D>(root.path(), reopened);
        let mut fresh = D::open(root.path());
        pollster::block_on(async {
            capture::verify_source_document(&mut fresh, &annotator, &expected)
                .await
                .unwrap();
            capture::verify_source_document(&mut fresh, &annotator, &previous)
                .await
                .unwrap();
        });
    }
}

fn sealed_failure_at_each_write<D: Durability>() {
    let refused = std::cell::Cell::new(0usize);
    let (annotator, sealer) = (JsonAnnotator::new(), FixtureSealer::open());
    let (prev_doc, cand_doc) = (document("previous"), document("candidate"));
    let (prev_req, cand_req) = (request(1, &prev_doc), request(2, &cand_doc));
    let baseline = tempfile::tempdir().unwrap();
    let mut counted = FaultStore::new(D::open(baseline.path()), None, false);
    let previous = pollster::block_on(sealed::capture_request(
        &mut counted,
        &sealer,
        &annotator,
        &prev_req,
        Some(&prev_doc),
    ))
    .unwrap();
    let before = counted.writes();
    let expected = pollster::block_on(sealed::capture_request(
        &mut counted,
        &sealer,
        &annotator,
        &cand_req,
        Some(&cand_doc),
    ))
    .unwrap();
    let write_count = counted.writes() - before;
    assert!(
        write_count >= 5,
        "{}: pending receipt, three artifacts, final receipt",
        D::NAME
    );
    close::<D>(baseline.path(), counted.inner);

    for (fail_at, after_write) in (1..=write_count).flat_map(|w| [(w, false), (w, true)]) {
        let root = tempfile::tempdir().unwrap();
        let mut seed = D::open(root.path());
        pollster::block_on(sealed::capture_request(
            &mut seed,
            &sealer,
            &annotator,
            &prev_req,
            Some(&prev_doc),
        ))
        .unwrap();
        let mut failing = FaultStore::new(seed, Some(fail_at), after_write);
        let error = pollster::block_on(sealed::capture_request(
            &mut failing,
            &sealer,
            &annotator,
            &cand_req,
            Some(&cand_doc),
        ))
        .unwrap_err();
        assert!(
            error.to_string().contains("injected deposit write"),
            "{} write {fail_at}: {error}",
            D::NAME
        );
        close::<D>(root.path(), failing.inner);

        // A blob without its manifest (or the reverse) is the one window the
        // sealed namespace refuses to adopt.
        let key = error
            .to_string()
            .split(": ")
            .last()
            .unwrap_or_default()
            .to_owned();
        let half_written = (key.starts_with("blob:") && after_write)
            || (key.starts_with("manifest:") && !after_write);

        let mut reopened = D::open(root.path());
        pollster::block_on(async {
            sealed::verify(&mut reopened, &sealer, &annotator, &prev_req, &previous)
                .await
                .unwrap();
            let before = dump(&reopened).await;
            let retried = sealed::capture_request(
                &mut reopened,
                &sealer,
                &annotator,
                &cand_req,
                Some(&cand_doc),
            )
            .await;
            if half_written {
                let error = retried.unwrap_err();
                assert!(
                    error
                        .to_string()
                        .contains("incomplete artifact requires repair"),
                    "{} write {fail_at} ({key}): {error}",
                    D::NAME
                );
                assert_eq!(dump(&reopened).await, before, "a refusal writes nothing");
                refused.set(refused.get() + 1);
                sealed::verify(&mut reopened, &sealer, &annotator, &prev_req, &previous)
                    .await
                    .unwrap();
                return;
            }
            assert_eq!(
                retried.unwrap(),
                expected,
                "{} retry identity after write {fail_at}",
                D::NAME
            );
            assert_eq!(
                sealed::capture_request(&mut reopened, &sealer, &annotator, &cand_req, None)
                    .await
                    .unwrap(),
                expected,
                "{} receipt-only recovery after write {fail_at}",
                D::NAME
            );
            sealed::acknowledge(&mut reopened, &sealer, &annotator, &cand_req)
                .await
                .unwrap();
            sealed::verify(&mut reopened, &sealer, &annotator, &prev_req, &previous)
                .await
                .unwrap();
            let receipts = sealed::receipts(&mut reopened, &sealer).await.unwrap();
            assert_eq!(receipts.len(), 2, "{} write {fail_at}", D::NAME);
            assert_eq!(receipts.iter().filter(|r| r.saved).count(), 1);
            let annotations = eidetic::list_typed::<Annotation>(&mut reopened)
                .await
                .unwrap();
            assert_eq!(annotations.len(), 2, "{} write {fail_at}", D::NAME);
        });
    }
    // Both halves of the window were exercised on every backend.
    assert!(refused.get() >= 2, "{}", D::NAME);
}

fn integrity_refusals<D: Durability>() {
    let annotator = JsonAnnotator::new();
    for annotation in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let mut store = D::open(root.path());
        pollster::block_on(async {
            let saved =
                capture::capture_source_document_in(&mut store, &annotator, &document("original"))
                    .await
                    .unwrap();
            let target = manifest_id(if annotation {
                &saved.annotation_manifest
            } else {
                &saved.raw_manifest
            });
            store
                .put(
                    &format!("blob:{}", target.0.to_hex()),
                    b"wrong bytes under the correct address",
                )
                .await
                .unwrap();
            let error = capture::verify_source_document(&mut store, &annotator, &saved)
                .await
                .unwrap_err();
            assert!(error.contains("payload integrity"), "{}: {error}", D::NAME);
        });
        close::<D>(root.path(), store);
        let mut reopened = D::open(root.path());
        pollster::block_on(async {
            let saved = capture::capture_source_document_in(
                &mut MemoryBackend::new(),
                &annotator,
                &document("original"),
            )
            .await
            .unwrap();
            assert!(
                capture::verify_source_document(&mut reopened, &annotator, &saved)
                    .await
                    .is_err(),
                "reopening cannot conceal corrupt bytes"
            );
        });
    }
    for annotation in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let mut store = D::open(root.path());
        pollster::block_on(async {
            let saved =
                capture::capture_source_document_in(&mut store, &annotator, &document("original"))
                    .await
                    .unwrap();
            let replacement = capture::capture_source_document_in(
                &mut store,
                &annotator,
                &document("replacement"),
            )
            .await
            .unwrap();
            let (requested, substitute) = if annotation {
                (
                    manifest_id(&saved.annotation_manifest),
                    manifest_id(&replacement.annotation_manifest),
                )
            } else {
                (
                    manifest_id(&saved.raw_manifest),
                    manifest_id(&replacement.raw_manifest),
                )
            };
            let bytes = store.get(&substitute.store_key()).await.unwrap().unwrap();
            store.put(&requested.store_key(), &bytes).await.unwrap();
            let error = capture::verify_source_document(&mut store, &annotator, &saved)
                .await
                .unwrap_err();
            assert!(
                error.contains("manifest identity mismatch"),
                "{}: {error}",
                D::NAME
            );
        });
    }
}

fn sealed_namespace_and_custody<D: Durability>() {
    let (annotator, sealer) = (JsonAnnotator::new(), FixtureSealer::open());
    let root = tempfile::tempdir().unwrap();
    let mut store = D::open(root.path());
    let (doc, req) = (document("private"), request(7, &document("private")));
    pollster::block_on(async {
        // A locked custody refuses before any write.
        sealer.set_open(false);
        assert!(
            sealed::capture_request(&mut store, &sealer, &annotator, &req, Some(&doc))
                .await
                .is_err()
        );
        assert!(store.list("").await.unwrap().is_empty(), "{}", D::NAME);
        sealer.set_open(true);

        // Oversized input is refused before extraction or any write.
        let big = {
            let body = "x".repeat(512 * 1024 + 1);
            FetchedDocument {
                bytes: body.as_bytes().to_vec(),
                body,
                ..document("big")
            }
        };
        let err = sealed::capture_request(
            &mut store,
            &sealer,
            &annotator,
            &request(8, &big),
            Some(&big),
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("custody payload limit"), "{err}");
        assert!(store.list("").await.unwrap().is_empty());

        // An unsealed artifact at a sealed address is never adopted or replaced.
        let legacy = eidetic::models::OpaqueBlob(doc.bytes.clone());
        let id = eidetic::save_typed(
            &mut store,
            &legacy,
            Vec::new(),
            eidetic::PrivacyClass::LocalOnly,
            eidetic::ProvenanceRecord {
                origin: eidetic::ProvenanceOrigin::Generated,
                upstream: Vec::new(),
                tooling: None,
                generated_at: eidetic::Timestamp(0),
            },
            eidetic::TrustEnvelope {
                level: eidetic::TrustLevel::SelfAsserted,
                signatures: Vec::new(),
                moderation_state: eidetic::ModerationState::Unreviewed,
            },
            eidetic::Timestamp(0),
        )
        .await
        .unwrap();
        let key = format!("blob:{}", id.0.to_hex());
        let before = store.get(&key).await.unwrap();
        assert!(sealed::save(&mut store, &sealer, &legacy, 1).await.is_err());
        assert_eq!(store.get(&key).await.unwrap(), before);
    });
}

macro_rules! on_every_backend {
    ($($test:ident),* $(,)?) => {$(
        mod $test {
            use super::*;
            #[test] fn memory() { super::$test::<Memory>(); }
            #[test] fn fjall() { super::$test::<Fjall>(); }
            #[test] fn redb() { super::$test::<Redb>(); }
        }
    )*};
}

on_every_backend!(
    deposit_failure_at_each_write,
    sealed_failure_at_each_write,
    integrity_refusals,
    sealed_namespace_and_custody,
);

#[test]
fn durable_sealed_store_holds_no_cleartext_on_disk() {
    let (annotator, sealer) = (JsonAnnotator::new(), FixtureSealer::open());
    let root = tempfile::tempdir().unwrap();
    let mut store = Fjall::open(root.path());
    let body = "<html><main><p>PRIVATE-CAPTURE-CANARY-λ-837</p></main></html>".to_string();
    let doc = FetchedDocument {
        bytes: body.as_bytes().to_vec(),
        body,
        effective_url: Some("https://private.example/secret".into()),
        content_type: Some("text/html".into()),
        acquired_at_ms: 1234,
    };
    let req = request(9, &doc);
    pollster::block_on(async {
        sealed::capture_request(&mut store, &sealer, &annotator, &req, Some(&doc))
            .await
            .unwrap();
        sealed::acknowledge(&mut store, &sealer, &annotator, &req)
            .await
            .unwrap();
        for key in store.list("").await.unwrap() {
            let bytes = store.get(&key).await.unwrap().unwrap();
            assert!(!String::from_utf8_lossy(&bytes).contains("PRIVATE-CAPTURE-CANARY"));
            assert!(!String::from_utf8_lossy(&bytes).contains("private.example"));
        }
    });
    drop(store);
    let mut paths = vec![root.path().to_path_buf()];
    while let Some(path) = paths.pop() {
        if path.is_dir() {
            paths.extend(std::fs::read_dir(path).unwrap().map(|e| e.unwrap().path()));
        } else {
            let bytes = std::fs::read(path).unwrap();
            assert!(!bytes.windows(21).any(|w| w == b"PRIVATE-CAPTURE-CANAR"));
            assert!(!bytes.windows(14).any(|w| w == b"private.example"));
        }
    }
}
