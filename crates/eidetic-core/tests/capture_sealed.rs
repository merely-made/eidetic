// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Behaviour moved from Turnstone's `sealed_capture`, `sealed_records` and
//! source-capture suites that does not need fault injection: acquisition
//! refusals, exact-byte reopen, sealed structured round trips, and storage
//! limits that must refuse before custody is asked.

mod support;

use eidetic::capture::{self, CaptureRequest, FetchedDocument, SourceIdentity, sealed};
use eidetic::records::{Cardinality, PropertyDefinitionV1, ValueType};
use eidetic::{Backend, MemoryBackend};
use std::sync::atomic::Ordering;
use support::*;

fn doc(url: Option<&str>, at: u64) -> FetchedDocument {
    FetchedDocument {
        bytes: b"<p>kestrel</p>".to_vec(),
        body: "<p>kestrel</p>".into(),
        effective_url: url.map(Into::into),
        content_type: Some("text/html".into()),
        acquired_at_ms: at,
    }
}

#[test]
fn source_capture_refuses_incomplete_acquisition_context() {
    pollster::block_on(async {
        let mut store = MemoryBackend::new();
        let annotator = JsonAnnotator::new();
        let missing_url = doc(None, 9);
        let error = capture::capture_source_document_in(&mut store, &annotator, &missing_url)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("final response URL"));
        let missing_time = doc(Some("https://example.test/kestrel"), 0);
        let error = capture::capture_source_document_in(&mut store, &annotator, &missing_time)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("acquisition time"));
        let mut not_html = doc(Some("https://example.test/kestrel"), 9);
        not_html.content_type = Some("application/json".into());
        let error = capture::capture_source_document_in(&mut store, &annotator, &not_html)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("text/html"));
        assert!(
            store.list("").await.unwrap().is_empty(),
            "refusals write nothing"
        );

        // The sealed path asks for the same observations.
        let sealer = FixtureSealer::open();
        for bad in [&missing_url, &missing_time, &not_html] {
            let request = CaptureRequest {
                id: uuid::Uuid::from_u128(1),
                session: uuid::Uuid::from_u128(2),
                graph: uuid::Uuid::from_u128(3),
                node: uuid::Uuid::from_u128(4),
                url: "https://example.test/kestrel".into(),
                source: Some(SourceIdentity::of(bad)),
                keep: false,
            };
            let error =
                sealed::capture_request(&mut store, &sealer, &annotator, &request, Some(bad))
                    .await
                    .unwrap_err();
            assert!(
                error.to_string().starts_with("sealed capture needs"),
                "{error}"
            );
        }
    });
}

#[test]
fn explicit_source_capture_keeps_exact_bytes_and_reopens_annotation() {
    pollster::block_on(async {
        let exact = b"<html><body><main><p>source-only kestrel</p></main></body></html>";
        let document = FetchedDocument {
            bytes: exact.to_vec(),
            content_type: Some("text/html; charset=utf-8".into()),
            body: String::from_utf8(exact.to_vec()).unwrap(),
            effective_url: Some("https://final.example/kestrel".into()),
            acquired_at_ms: 1_700_000_000_000,
        };
        let annotator = JsonAnnotator::new();
        let mut store = MemoryBackend::new();
        let stored = capture::capture_source_document_in(&mut store, &annotator, &document)
            .await
            .unwrap();
        assert_ne!(stored.raw_manifest, stored.annotation_manifest);

        let manifests = eidetic::list_typed::<Annotation>(&mut store).await.unwrap();
        assert_eq!(manifests.len(), 1);
        let record = <JsonAnnotator as capture::SourceAnnotator>::load(
            &annotator,
            &mut store,
            manifests[0].id,
        )
        .await
        .unwrap()
        .unwrap();
        let evidence =
            <JsonAnnotator as capture::SourceAnnotator>::evidence(&annotator, &record).unwrap();
        assert_eq!(evidence.final_source, "https://final.example/kestrel");
        assert_eq!(
            evidence.raw_manifest.unwrap().to_string(),
            stored.raw_manifest
        );
        assert_eq!(evidence.capture_hash, eidetic::Hash::of(exact));
        let raw_manifest =
            eidetic::manifest::load_manifest(&mut store, evidence.raw_manifest.unwrap())
                .await
                .unwrap()
                .unwrap();
        let raw =
            eidetic::manifest::resolve_blob(&mut store, &mut eidetic::NoFetcher, &raw_manifest)
                .await
                .unwrap();
        assert_eq!(
            raw, exact,
            "the raw response bytes are not the annotation's text"
        );
    });
}

#[test]
fn sealed_property_definition_requires_valid_round_trip() {
    pollster::block_on(async {
        let sealer = FixtureSealer::open();
        let mut store = FaultStore::new(MemoryBackend::new(), None, false);
        let definition = PropertyDefinitionV1 {
            id: "fixture.departure".into(),
            revision: "1".into(),
            value_type: ValueType::Text,
            unit: None,
            cardinality: Cardinality::Single,
            statuses: vec!["approved".into()],
        };
        let id = capture::deposit_definition(&mut store, &sealer, &definition)
            .await
            .unwrap();
        assert_eq!(id, definition.manifest_id().unwrap());
        for (key, bytes) in dump(&store).await {
            let text = String::from_utf8_lossy(&bytes);
            // Schema artifacts are public; the definition itself is sealed.
            if key.starts_with("blob:") && !text.contains("\"schema_id\"") {
                assert!(!text.contains("fixture.departure"), "{key} holds cleartext");
            }
        }
        assert_eq!(
            capture::load_definition(&mut store, &sealer, id)
                .await
                .unwrap(),
            definition
        );
        let mut invalid = definition.clone();
        invalid.id.clear();
        assert!(
            capture::deposit_definition(&mut store, &sealer, &invalid)
                .await
                .is_err()
        );
        sealer.set_open(false);
        assert!(
            capture::load_definition(&mut store, &sealer, id)
                .await
                .is_err()
        );
    });
}

#[test]
fn storage_limits_refuse_before_unseal_and_keep_their_cause() {
    pollster::block_on(async {
        let document = golden_document();
        let request: CaptureRequest =
            serde_json::from_value(fixture_json("sealed_meta.json")["request"].clone()).unwrap();
        let annotator = JsonAnnotator::new();
        let sealer = CountingSealer::new(FixtureSealer::open());
        let mut store = BudgetSpent::new(MemoryBackend::new());
        sealed::capture_request(&mut store, &sealer, &annotator, &request, Some(&document))
            .await
            .unwrap();
        sealed::acknowledge(&mut store, &sealer, &annotator, &request)
            .await
            .unwrap();
        let before = sealer.unseals();

        store.refuse_get.store(true, Ordering::SeqCst);
        let stored = sealed::receipts(&mut store, &sealer).await;
        let error = stored.unwrap_err();
        assert!(
            error.to_string().contains("sealed capture receipt read"),
            "{error}"
        );
        let error = sealed::verify(
            &mut store,
            &sealer,
            &annotator,
            &request,
            &serde_json::from_value(fixture_json("sealed_meta.json")["stored"].clone()).unwrap(),
        )
        .await
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("sealed source manifest read failed"),
            "{error}"
        );
        assert!(matches!(
            error.storage_error(),
            Some(muniment::StoreError::LimitExceeded {
                kind: muniment::StoreLimitKind::ValueBytes,
                limit: 0
            })
        ));
        store.refuse_get.store(false, Ordering::SeqCst);

        store.refuse_list.store(true, Ordering::SeqCst);
        let error = sealed::receipts(&mut store, &sealer).await.unwrap_err();
        assert!(
            error
                .to_string()
                .contains("sealed capture inventory unavailable"),
            "{error}"
        );
        assert!(matches!(
            error.storage_error(),
            Some(muniment::StoreError::LimitExceeded {
                kind: muniment::StoreLimitKind::VisitedItems,
                limit: 0
            })
        ));
        assert_eq!(sealer.unseals(), before, "no refusal reached custody");
        store.refuse_list.store(false, Ordering::SeqCst);
        assert_eq!(
            sealed::receipts(&mut store, &sealer).await.unwrap().len(),
            1
        );
    });
}
