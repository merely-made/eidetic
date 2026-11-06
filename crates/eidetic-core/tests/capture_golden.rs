// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The promoted capture core reads, and re-writes byte-for-byte, what
//! Turnstone's native capture wrote. See `fixtures/capture_golden/README.md`.

mod support;

use eidetic::MemoryBackend;
use eidetic::capture::{
    self, CaptureRequest, DepositReceipt, SourceIdentity, StoredSourceDocument, sealed,
};
use support::*;

fn request_and_stored() -> (CaptureRequest, StoredSourceDocument) {
    let meta = fixture_json("sealed_meta.json");
    (
        serde_json::from_value(meta["request"].clone()).unwrap(),
        serde_json::from_value(meta["stored"].clone()).unwrap(),
    )
}

#[test]
fn request_and_identity_shapes_round_trip_the_stored_json() {
    let (request, stored) = request_and_stored();
    let meta = fixture_json("sealed_meta.json");
    // Stored shapes re-serialize to the same JSON values (key order is
    // irrelevant to serde_json::Value equality; the bytes are checked below
    // through the receipt envelope).
    assert_eq!(serde_json::to_value(&request).unwrap(), meta["request"]);
    assert_eq!(serde_json::to_value(&stored).unwrap(), meta["stored"]);
    assert_eq!(request.source, Some(SourceIdentity::of(&golden_document())));
    assert!(request.keep);
}

#[test]
fn sealed_deposit_reads_and_recovers_from_stored_bytes() {
    pollster::block_on(async {
        let (request, stored) = request_and_stored();
        let mut store = MemoryBackend::new();
        load_into(&store, &fixture_kv("sealed_acknowledged.kv.json")).await;
        let (sealer, annotator) = (FixtureSealer::open(), JsonAnnotator::new());

        let receipts = sealed::receipts(&mut store, &sealer).await.unwrap();
        assert_eq!(
            receipts,
            vec![DepositReceipt {
                saved: true,
                request: request.clone(),
                stored: Some(stored.clone()),
            }]
        );
        // Recovery with no live document re-verifies the stored pair.
        assert_eq!(
            sealed::capture_request(&mut store, &sealer, &annotator, &request, None)
                .await
                .unwrap(),
            stored
        );
        sealed::verify(&mut store, &sealer, &annotator, &request, &stored)
            .await
            .unwrap();
        let before = dump(&store).await;
        sealed::acknowledge(&mut store, &sealer, &annotator, &request)
            .await
            .unwrap();
        assert_eq!(
            dump(&store).await,
            before,
            "acknowledging a saved receipt writes nothing"
        );

        // A changed request is refused, and a locked custody refuses the lot.
        let mut changed = request.clone();
        changed.url = "https://other.example/".into();
        assert!(
            sealed::capture_request(&mut store, &sealer, &annotator, &changed, None)
                .await
                .is_err()
        );
        sealer.set_open(false);
        assert!(sealed::receipts(&mut store, &sealer).await.is_err());
        assert!(
            sealed::capture_request(&mut store, &sealer, &annotator, &request, None)
                .await
                .is_err()
        );
    });
}

#[test]
fn sealed_capture_writes_the_bytes_turnstone_wrote() {
    pollster::block_on(async {
        let (request, stored) = request_and_stored();
        let mut store = MemoryBackend::new();
        let (sealer, annotator) = (FixtureSealer::open(), JsonAnnotator::new());
        let got = sealed::capture_request(
            &mut store,
            &sealer,
            &annotator,
            &request,
            Some(&golden_document()),
        )
        .await
        .unwrap();
        assert_eq!(got, stored);
        assert_eq!(dump(&store).await, fixture_kv("sealed_deposited.kv.json"));
        sealed::acknowledge(&mut store, &sealer, &annotator, &request)
            .await
            .unwrap();
        assert_eq!(
            dump(&store).await,
            fixture_kv("sealed_acknowledged.kv.json")
        );
    });
}

#[test]
fn sealed_artifacts_carry_markers_and_no_cleartext() {
    pollster::block_on(async {
        let store = MemoryBackend::new();
        let kv = fixture_kv("sealed_acknowledged.kv.json");
        load_into(&store, &kv).await;
        let mut sealed_manifests = 0;
        for (key, value) in &kv {
            let text = String::from_utf8_lossy(value);
            assert!(!text.contains("Synthetic capture fixture"), "{key}");
            assert!(!text.contains("fixture.example"), "{key}");
            if key.starts_with("manifest:") {
                let manifest: eidetic::BlobManifest = serde_json::from_slice(value).unwrap();
                assert!(eidetic::seal_marker(&manifest).unwrap().is_some());
                sealed_manifests += 1;
            }
        }
        assert_eq!(sealed_manifests, 2);
        assert!(
            kv.iter()
                .any(|(k, _)| k.starts_with(sealed::RECEIPT_PREFIX))
        );
    });
}

#[test]
fn source_document_verifies_and_rewrites_byte_identically() {
    pollster::block_on(async {
        let meta = fixture_json("source_document_meta.json");
        let stored: StoredSourceDocument = serde_json::from_value(meta["stored"].clone()).unwrap();
        let annotator = JsonAnnotator::new();

        let mut stored_store = MemoryBackend::new();
        load_into(&stored_store, &fixture_kv("source_document.kv.json")).await;
        capture::verify_source_document_typed(&mut stored_store, &annotator, &stored)
            .await
            .unwrap();
        // Re-capturing over the stored bytes is idempotent.
        let before = dump(&stored_store).await;
        let again =
            capture::capture_source_document_in(&mut stored_store, &annotator, &golden_document())
                .await
                .unwrap();
        assert_eq!(again, stored);
        assert_eq!(dump(&stored_store).await, before);

        // A fresh capture writes what Turnstone wrote.
        let mut fresh = MemoryBackend::new();
        let wrote = capture::capture_source_document_in(&mut fresh, &annotator, &golden_document())
            .await
            .unwrap();
        assert_eq!(wrote, stored);
        assert_eq!(dump(&fresh).await, fixture_kv("source_document.kv.json"));
    });
}

#[test]
fn structured_sealed_records_read_and_rewrite_identically() {
    pollster::block_on(async {
        let meta = fixture_json("structured_meta.json");
        let definition: eidetic::records::PropertyDefinitionV1 =
            serde_json::from_value(meta["definition"].clone()).unwrap();
        let record: eidetic::records::PropertyRecordV1 =
            serde_json::from_value(meta["record"].clone()).unwrap();
        let record_id = manifest_id(meta["record_id"].as_str().unwrap());
        let sealer = FixtureSealer::open();

        let mut loaded = FaultStore::new(MemoryBackend::new(), None, false);
        load_into(&loaded, &fixture_kv("structured.kv.json")).await;
        let (got, def) = capture::load_record(&mut loaded, &sealer, record_id)
            .await
            .unwrap();
        assert_eq!((got, def), (record.clone(), definition.clone()));
        // A byte ceiling below the stored size refuses before reading.
        let tiny = capture::load_record_bounded(&mut loaded, &sealer, record_id, 16, usize::MAX)
            .await
            .unwrap_err();
        assert!(matches!(
            tiny.storage_error(),
            Some(muniment::StoreError::LimitExceeded { .. })
        ));

        let mut fresh = FaultStore::new(MemoryBackend::new(), None, false);
        let def_id = capture::deposit_definition(&mut fresh, &sealer, &definition)
            .await
            .unwrap();
        assert_eq!(def_id.to_string(), meta["definition_id"].as_str().unwrap());
        assert_eq!(
            capture::deposit_record(&mut fresh, &sealer, &record)
                .await
                .unwrap(),
            record_id
        );
        // Every stored key is rewritten byte-for-byte. Eidetic has since added
        // record schemas that `bootstrap_record_schemas` also seeds (the
        // suggestion lane), so the fresh store may hold extra schema artifacts;
        // it must hold nothing else extra.
        let fresh_dump = dump(&fresh.inner).await;
        let golden = fixture_kv("structured.kv.json");
        for (key, value) in &golden {
            let found = fresh_dump.iter().find(|(k, _)| k == key);
            assert_eq!(found.map(|(_, v)| v), Some(value), "{key}");
        }
        let extra: Vec<&String> = fresh_dump
            .iter()
            .map(|(k, _)| k)
            .filter(|k| !golden.iter().any(|(g, _)| &g == k))
            .collect();
        for key in &extra {
            let id = key
                .strip_prefix("blob:")
                .or_else(|| key.strip_prefix("manifest:"))
                .unwrap_or_else(|| panic!("unexpected extra key {key}"));
            let (blob, manifest) = (format!("blob:{id}"), format!("manifest:{id}"));
            assert!(
                extra.contains(&&blob) && extra.contains(&&manifest),
                "{key} is not a whole schema artifact"
            );
        }
    });
}

#[test]
fn browsing_and_page_text_records_read_and_rewrite_identically() {
    pollster::block_on(async {
        let kv = fixture_kv("browsing.kv.json");
        let mut store = MemoryBackend::new();
        load_into(&store, &kv).await;
        let memory = eidetic::BrowsingMemory::load(&mut store, 32).await.unwrap();
        let events: usize = memory.traces().map(|t| t.events.len()).sum();
        assert_eq!(events, 2);
        let texts = eidetic::PageTextStore::new(&store)
            .load_all()
            .await
            .unwrap();
        assert_eq!(texts.len(), 2);
        assert_eq!(
            texts.get("https://fixture.example/a"),
            Some("Alpha page text alpha beta.")
        );
        // Fragments canonicalize away: the second record is keyed by its page.
        assert_eq!(
            texts.get("https://fixture.example/b"),
            Some("Beta page text.")
        );

        // Key layouts.
        let keys: Vec<&str> = kv.iter().map(|(k, _)| k.as_str()).collect();
        for prefix in [
            "blob/",
            "blob:",
            "manifest:",
            "page-text/",
            "resource-capture/",
        ] {
            assert!(keys.iter().any(|k| k.starts_with(prefix)), "{prefix}");
        }

        // Re-write with the same calls.
        let mut fresh = MemoryBackend::new();
        eidetic::bootstrap_browsing_schema(&mut fresh)
            .await
            .unwrap();
        let mut memory = eidetic::BrowsingMemory::new(32);
        let page = |u: &str, t: Option<&str>| eidetic::PageRef {
            url: u.into(),
            title: t.map(Into::into),
        };
        memory.record_traversal(
            "",
            eidetic::TraceEvent {
                from: None,
                to: page("https://fixture.example/a", Some("Alpha")),
                transition: eidetic::TraceTransition::UrlTyped,
                at_ms: 1_700_000_000_100,
                dwell_ms: None,
                candidates: vec![],
            },
        );
        memory.record_traversal(
            "",
            eidetic::TraceEvent {
                from: Some(page("https://fixture.example/a", Some("Alpha"))),
                to: page("https://fixture.example/b", None),
                transition: eidetic::TraceTransition::LinkClick,
                at_ms: 1_700_000_000_200,
                dwell_ms: Some(4000),
                candidates: vec![page("https://fixture.example/c", Some("Gamma"))],
            },
        );
        memory.flush(&mut fresh, 1_700_000_001_000).await.unwrap();
        let pt = eidetic::PageTextStore::new(&fresh);
        pt.put(
            "https://fixture.example/a",
            "Alpha page text alpha beta.",
            1_700_000_000_150,
        )
        .await
        .unwrap();
        pt.put(
            "https://fixture.example/b#frag",
            "Beta page text.",
            1_700_000_000_250,
        )
        .await
        .unwrap();
        assert_eq!(dump(&fresh).await, kv);
    });
}
