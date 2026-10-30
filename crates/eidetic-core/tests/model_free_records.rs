use eidetic::manifest::NoFetcher;
use eidetic::models::recall::{RecallSourceId, RecallSourceKind};
use eidetic::records::*;
use eidetic::schema::{
    Hash, ModerationState, PrivacyClass, ProvenanceOrigin, ProvenanceRecord, Timestamp,
    TrustEnvelope, TrustLevel,
};
use eidetic::seal::{PayloadSealer, SealEpochId, SealedBlobRef};
use eidetic::typed::{TypedPayload, load_typed_sealed, save_typed_sealed};
use muniment::MemoryBackend;

struct TestSealer;
impl PayloadSealer for TestSealer {
    fn seal(&self, _: &Hash, bytes: &[u8]) -> eidetic::Result<(Vec<u8>, SealedBlobRef)> {
        Ok((
            bytes.iter().map(|x| x ^ 0xaa).collect(),
            SealedBlobRef {
                epoch: SealEpochId([1; 16]),
                format: "test".into(),
            },
        ))
    }
    fn unseal(&self, _: &Hash, _: &SealedBlobRef, bytes: &[u8]) -> eidetic::Result<Vec<u8>> {
        Ok(bytes.iter().map(|x| x ^ 0xaa).collect())
    }
}
fn definition() -> PropertyDefinitionV1 {
    PropertyDefinitionV1 {
        id: "ferry.departure".into(),
        revision: "1".into(),
        value_type: ValueType::Integer,
        unit: None,
        cardinality: Cardinality::Single,
        statuses: vec!["approved".into()],
    }
}
fn record() -> PropertyRecordV1 {
    PropertyRecordV1 {
        id: "assertion".into(),
        entity: "boat".into(),
        property: "ferry.departure".into(),
        schema_revision: "1".into(),
        definition_manifest: definition().manifest_id().unwrap(),
        context: "route".into(),
        value: PropertyValue::Integer(930),
        valid_time: None,
        status: None,
        author: "owner".into(),
        method: "manual".into(),
        evidence: Evidence::Field {
            source: RecallSourceId {
                kind: RecallSourceKind::Capture,
                owner: Some("owner".into()),
                source: "source".into(),
                node: None,
            },
            version: "v1".into(),
            record_id: "field-record".into(),
            field: "departure".into(),
        },
        recorded_at: None,
    }
}
async fn roundtrip<T: TypedPayload + std::fmt::Debug + PartialEq>(
    store: &mut MemoryBackend,
    value: &T,
) {
    let provenance = ProvenanceRecord {
        origin: ProvenanceOrigin::Generated,
        upstream: vec![],
        tooling: None,
        generated_at: Timestamp(0),
    };
    let trust = TrustEnvelope {
        level: TrustLevel::SelfAsserted,
        signatures: vec![],
        moderation_state: ModerationState::Unreviewed,
    };
    let id = save_typed_sealed(
        store,
        Some(&TestSealer),
        value,
        vec![],
        PrivacyClass::LocalOnly,
        provenance,
        trust,
        Timestamp(0),
    )
    .await
    .unwrap();
    let loaded: T = load_typed_sealed(store, &mut NoFetcher, Some(&TestSealer), id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(&loaded, value);
}
#[test]
fn model_free_sealed_payloads_retain_inputs_settings_and_results() {
    pollster::block_on(async {
        let d = definition();
        let r = record();
        let snapshot = RecordSnapshot {
            scope_generation: "generation".into(),
            complete: true,
            exclusions: vec![],
            sources: vec![SourceRecord {
                source: r.evidence.source().clone(),
                version: "v1".into(),
                producer: "capture".into(),
                captured_at: None,
                tags: vec![],
            }],
            definitions: vec![d.clone()],
            records: vec![r.clone()],
            lifecycle: vec![LifecycleObservation {
                lifecycle_ref: "journal".into(),
                record_id: r.id.clone(),
                state: LifecycleState::Active,
            }],
        };
        let result = RecordResult {
            operation_version: "eidetic.structured-records/v1".into(),
            snapshot,
            query: RecordQuery {
                operation: RecordOperation::PropertyLookup,
                filters: RecordFilters::default(),
            },
            settings: RecordSettings::default(),
            rows: vec![RecordRow {
                key: RowValue::Value(r.value.clone()),
                group: None,
                count: None,
                record_ids: vec![r.id.clone()],
                sources: vec![r.evidence.source().clone()],
            }],
        };
        let mut store = MemoryBackend::default();
        roundtrip(&mut store, &d).await;
        roundtrip(&mut store, &r).await;
        roundtrip(&mut store, &result.query).await;
        roundtrip(&mut store, &result).await;
    });
}
#[test]
fn model_free_schema_bootstrap_provides_resolvable_real_definitions() {
    pollster::block_on(async {
        let mut store = MemoryBackend::default();
        assert!(
            eidetic::records::validate_required_schema(&mut store, &mut NoFetcher, &definition())
                .await
                .is_err()
        );
        eidetic::records::bootstrap_record_schemas(&mut store)
            .await
            .unwrap();
        eidetic::records::validate_required_schema(&mut store, &mut NoFetcher, &definition())
            .await
            .unwrap();
        eidetic::records::validate_required_schema(&mut store, &mut NoFetcher, &record())
            .await
            .unwrap();
        let query = RecordQuery {
            operation: RecordOperation::PropertyLookup,
            filters: RecordFilters::default(),
        };
        eidetic::records::validate_required_schema(&mut store, &mut NoFetcher, &query)
            .await
            .unwrap();
    });
}
