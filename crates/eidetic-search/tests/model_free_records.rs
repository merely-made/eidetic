use eidetic::models::recall::{RecallSourceId, RecallSourceKind};
use eidetic::records::*;
use eidetic_search::records::evaluate;

fn source(id: &str) -> RecallSourceId {
    RecallSourceId {
        kind: RecallSourceKind::Capture,
        owner: Some("owner".into()),
        source: id.into(),
        node: None,
    }
}
fn definition() -> PropertyDefinitionV1 {
    PropertyDefinitionV1 {
        id: "ferry.departure".into(),
        revision: "1".into(),
        value_type: ValueType::Integer,
        unit: None,
        cardinality: Cardinality::Single,
        statuses: vec!["approved".into(), "guessed".into()],
    }
}
fn record(id: &str, entity: &str, value: i64) -> PropertyRecordV1 {
    PropertyRecordV1 {
        id: id.into(),
        entity: entity.into(),
        property: "ferry.departure".into(),
        schema_revision: "1".into(),
        definition_manifest: definition().manifest_id().unwrap(),
        context: "route".into(),
        value: PropertyValue::Integer(value),
        valid_time: Some(TimeInterval { start: 1, end: 2 }),
        status: Some("approved".into()),
        author: "owner".into(),
        method: "manual".into(),
        evidence: Evidence::Field {
            source: source("a"),
            version: "v1".into(),
            record_id: "field".into(),
            field: "departure".into(),
        },
        recorded_at: None,
    }
}
fn bind_definition(s: &mut RecordSnapshot) {
    let id = s.definitions[0].manifest_id().unwrap();
    for record in &mut s.records {
        record.definition_manifest = id;
    }
}
fn snapshot() -> RecordSnapshot {
    RecordSnapshot {
        scope_generation: "g1".into(),
        complete: true,
        exclusions: vec![],
        sources: vec![SourceRecord {
            source: source("a"),
            version: "v1".into(),
            producer: "capture".into(),
            captured_at: None,
            tags: vec![],
        }],
        definitions: vec![definition()],
        records: vec![
            record("a", "entity1", 930),
            record("b", "entity1", 930),
            record("c", "entity1", 1015),
        ],
        lifecycle: vec![
            LifecycleObservation {
                lifecycle_ref: "journal".into(),
                record_id: "a".into(),
                state: LifecycleState::Active,
            },
            LifecycleObservation {
                lifecycle_ref: "journal".into(),
                record_id: "b".into(),
                state: LifecycleState::Active,
            },
            LifecycleObservation {
                lifecycle_ref: "journal".into(),
                record_id: "c".into(),
                state: LifecycleState::Active,
            },
        ],
    }
}
#[test]
fn model_free_groups_distinct_assertions_and_deduplicates_projection() {
    let mut s = snapshot();
    s.records.push(s.records[0].clone());
    let query = RecordQuery {
        operation: RecordOperation::GroupValues,
        filters: RecordFilters::default(),
    };
    let r = evaluate(&s, &query, &RecordSettings::default()).unwrap();
    assert_eq!(r.rows.len(), 2);
    assert_eq!(
        r.rows
            .iter()
            .find(|row| row.key == RowValue::Value(PropertyValue::Integer(930)))
            .unwrap()
            .record_ids,
        vec!["a", "b"]
    );
    assert_eq!(
        r.rows
            .iter()
            .find(|row| row.key == RowValue::Value(PropertyValue::Integer(1015)))
            .unwrap()
            .record_ids,
        vec!["c"]
    );
    assert_eq!(evaluate(&r.snapshot, &r.query, &r.settings).unwrap(), r);
}
#[test]
fn model_free_incomplete_inputs_refuse_count_and_absence() {
    let mut s = snapshot();
    s.complete = false;
    let query = RecordQuery {
        operation: RecordOperation::CountBySource,
        filters: RecordFilters::default(),
    };
    assert_eq!(
        evaluate(&s, &query, &RecordSettings::default()).unwrap_err(),
        ResearchError::Incomplete
    );
    let query = RecordQuery {
        operation: RecordOperation::PropertyLookup,
        filters: RecordFilters {
            entity: Some("missing".into()),
            ..Default::default()
        },
    };
    assert_eq!(
        evaluate(&s, &query, &RecordSettings::default()).unwrap_err(),
        ResearchError::Incomplete
    );
}
#[test]
fn model_free_checked_sum_refuses_overflow_and_conflicting_duplicate() {
    let mut s = snapshot();
    s.records = vec![record("a", "entity1", i64::MAX), record("b", "entity1", 1)];
    s.lifecycle.pop();
    let query = RecordQuery {
        operation: RecordOperation::Sum,
        filters: RecordFilters::default(),
    };
    assert_eq!(
        evaluate(&s, &query, &RecordSettings::default()).unwrap_err(),
        ResearchError::Overflow
    );
    s.records.push(record("a", "entity1", 3));
    assert!(matches!(
        evaluate(&s, &query, &RecordSettings::default()),
        Err(ResearchError::Invalid(_))
    ));
}
#[test]
fn model_free_counts_timelines_and_tags_keep_exact_sources() {
    let mut s = snapshot();
    s.sources[0].tags.push(TagIdentity {
        owner: "owner".into(),
        concept: "harbor".into(),
    });
    let mut copied = s.sources[0].clone();
    copied.source = source("copy");
    copied.captured_at = Some(10);
    s.sources.push(copied);
    let query = RecordQuery {
        operation: RecordOperation::CountByProducer,
        filters: RecordFilters::default(),
    };
    let r = evaluate(&s, &query, &RecordSettings::default()).unwrap();
    assert_eq!(r.rows[0].count, Some(2));
    let query = RecordQuery {
        operation: RecordOperation::TagMembership,
        filters: RecordFilters {
            tag: Some(TagIdentity {
                owner: "owner".into(),
                concept: "harbor".into(),
            }),
            ..Default::default()
        },
    };
    assert_eq!(
        evaluate(&s, &query, &RecordSettings::default())
            .unwrap()
            .rows
            .len(),
        2
    );
    let query = RecordQuery {
        operation: RecordOperation::SourceTimeline,
        filters: RecordFilters::default(),
    };
    let r = evaluate(&s, &query, &RecordSettings::default()).unwrap();
    assert_eq!(r.rows[0].key, RowValue::Time(None));
    assert_eq!(r.rows[1].key, RowValue::Time(Some(10)));
}
#[test]
fn model_free_comparison_keeps_former_guessed_unknown_and_distinct_entities() {
    let mut s = snapshot();
    let mut former = record("d", "entity1", 1100);
    former.valid_time = Some(TimeInterval { start: -10, end: 0 });
    let mut guessed = record("e", "entity1", 1100);
    guessed.status = Some("guessed".into());
    let mut unknown = record("f", "entity1", 1100);
    unknown.valid_time = None;
    let other = record("g", "entity2", 1100);
    for r in [former, guessed, unknown, other] {
        s.lifecycle.push(LifecycleObservation {
            lifecycle_ref: "journal".into(),
            record_id: r.id.clone(),
            state: LifecycleState::Active,
        });
        s.records.push(r);
    }
    let q = RecordQuery {
        operation: RecordOperation::Compare,
        filters: RecordFilters::default(),
    };
    let rows = evaluate(&s, &q, &RecordSettings::default()).unwrap().rows;
    let pair = |a: &str, b: &str| {
        rows.iter()
            .find(|r| r.record_ids == [a, b])
            .unwrap()
            .key
            .clone()
    };
    assert_eq!(pair("a", "b"), RowValue::Comparison(Comparison::Equal));
    assert_eq!(
        pair("a", "c"),
        RowValue::Comparison(Comparison::Disagreement)
    );
    assert_eq!(pair("a", "d"), RowValue::Comparison(Comparison::Distinct));
    assert_eq!(
        pair("a", "e"),
        RowValue::Comparison(Comparison::NonComparable)
    );
    assert_eq!(
        pair("a", "f"),
        RowValue::Comparison(Comparison::NonComparable)
    );
    assert_eq!(pair("a", "g"), RowValue::Comparison(Comparison::Distinct));
    s.definitions[0].cardinality = Cardinality::Multiple;
    bind_definition(&mut s);
    assert_eq!(
        evaluate(&s, &q, &RecordSettings::default())
            .unwrap()
            .rows
            .iter()
            .find(|r| r.record_ids == ["a", "c"])
            .unwrap()
            .key,
        RowValue::Comparison(Comparison::Distinct)
    );
}
#[test]
fn model_free_rejects_unit_mismatch_and_invalid_civil_time() {
    let mut s = snapshot();
    s.definitions[0].value_type = ValueType::Decimal;
    s.definitions[0].unit = Some("kg".into());
    for r in &mut s.records {
        r.value = PropertyValue::Decimal {
            coefficient: 100,
            scale: 1,
            unit: "kg".into(),
        };
    }
    s.records[1].value = PropertyValue::Decimal {
        coefficient: 100,
        scale: 1,
        unit: "lb".into(),
    };
    bind_definition(&mut s);
    let q = RecordQuery {
        operation: RecordOperation::Sum,
        filters: RecordFilters::default(),
    };
    assert!(matches!(
        evaluate(&s, &q, &RecordSettings::default()),
        Err(ResearchError::Invalid(_))
    ));
    s.definitions[0].value_type = ValueType::Timestamp;
    s.definitions[0].unit = None;
    for r in &mut s.records {
        r.value = PropertyValue::Timestamp(OffsetTimestamp {
            date: CivilDate {
                year: 2026,
                month: 2,
                day: 30,
            },
            hour: 9,
            minute: 30,
            second: 0,
            offset_minutes: 0,
        });
    }
    bind_definition(&mut s);
    assert!(matches!(
        evaluate(&s, &q, &RecordSettings::default()),
        Err(ResearchError::Invalid(_))
    ));
}
#[test]
fn model_free_filters_joins_and_work_limits_use_explicit_identity() {
    let mut s = snapshot();
    s.records[2].entity = "entity2".into();
    let q = RecordQuery {
        operation: RecordOperation::JoinEntity,
        filters: RecordFilters::default(),
    };
    let rows = evaluate(&s, &q, &RecordSettings::default()).unwrap().rows;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].record_ids, ["a", "b"]);
    let q = RecordQuery {
        operation: RecordOperation::PropertyLookup,
        filters: RecordFilters {
            entity: Some("entity2".into()),
            status: Some("approved".into()),
            ..Default::default()
        },
    };
    assert_eq!(
        evaluate(&s, &q, &RecordSettings::default()).unwrap().rows[0].record_ids,
        ["c"]
    );
    let q = RecordQuery {
        operation: RecordOperation::Compare,
        filters: RecordFilters::default(),
    };
    assert_eq!(
        evaluate(
            &s,
            &q,
            &RecordSettings {
                max_work: 2,
                ..Default::default()
            }
        )
        .unwrap_err(),
        ResearchError::LimitExceeded
    );
}
#[test]
fn model_free_source_queries_refuse_inapplicable_record_filters() {
    let q = RecordQuery {
        operation: RecordOperation::CountByProducer,
        filters: RecordFilters {
            entity: Some("missing".into()),
            ..Default::default()
        },
    };
    assert!(matches!(
        evaluate(&snapshot(), &q, &RecordSettings::default()),
        Err(ResearchError::Unsupported(_))
    ));
}
#[test]
fn model_free_grouping_preserves_identity_status_context_and_valid_time() {
    let mut s = snapshot();
    s.records[1].entity = "entity2".into();
    s.records[2].value = PropertyValue::Integer(930);
    s.records[2].status = Some("guessed".into());
    let mut former = record("d", "entity1", 930);
    former.valid_time = Some(TimeInterval { start: -10, end: 0 });
    s.lifecycle.push(LifecycleObservation {
        record_id: "d".into(),
        lifecycle_ref: "journal-d".into(),
        state: LifecycleState::Active,
    });
    s.records.push(former);
    let q = RecordQuery {
        operation: RecordOperation::GroupValues,
        filters: RecordFilters::default(),
    };
    let rows = evaluate(&s, &q, &RecordSettings::default()).unwrap().rows;
    assert_eq!(rows.len(), 4);
    assert!(rows.iter().all(|r| r.record_ids.len() == 1));
}
#[test]
fn model_free_source_count_counts_distinct_filtered_assertions() {
    let q = RecordQuery {
        operation: RecordOperation::CountBySource,
        filters: RecordFilters::default(),
    };
    let result = evaluate(&snapshot(), &q, &RecordSettings::default()).unwrap();
    assert_eq!(result.rows[0].count, Some(3));
    assert_eq!(result.rows[0].record_ids, ["a", "b", "c"]);
    let mut input = snapshot();
    input.records.push(input.records[0].clone());
    let q = RecordQuery {
        operation: RecordOperation::CountBySource,
        filters: RecordFilters {
            entity: Some("entity1".into()),
            ..Default::default()
        },
    };
    assert_eq!(
        evaluate(&input, &q, &RecordSettings::default())
            .unwrap()
            .rows[0]
            .count,
        Some(3)
    );
}
#[test]
fn model_free_offset_timestamps_group_equal_instants_and_reject_unknown_zone() {
    let mut s = snapshot();
    s.definitions[0].value_type = ValueType::Timestamp;
    let at = |hour, minute, offset_minutes| {
        PropertyValue::Timestamp(OffsetTimestamp {
            date: CivilDate {
                year: 2026,
                month: 10,
                day: 30,
            },
            hour,
            minute,
            second: 0,
            offset_minutes,
        })
    };
    s.records[0].value = at(9, 30, 0);
    s.records[1].value = at(10, 30, 60);
    s.records[2].value = at(10, 15, 0);
    bind_definition(&mut s);
    let q = RecordQuery {
        operation: RecordOperation::GroupValues,
        filters: RecordFilters::default(),
    };
    let rows = evaluate(&s, &q, &RecordSettings::default()).unwrap().rows;
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().any(|r| r.record_ids == ["a", "b"]));
    assert!(
        serde_json::from_str::<OffsetTimestamp>(
            r#"{"date":{"year":2026,"month":10,"day":30},"hour":9,"minute":30,"second":0}"#
        )
        .is_err()
    );
}
#[test]
fn model_free_group_rows_retain_complete_grouping_key() {
    let q = RecordQuery {
        operation: RecordOperation::GroupValues,
        filters: RecordFilters::default(),
    };
    let r = evaluate(&snapshot(), &q, &RecordSettings::default()).unwrap();
    let group = r
        .rows
        .iter()
        .find(|row| row.record_ids == ["a", "b"])
        .unwrap()
        .group
        .as_ref()
        .unwrap();
    assert_eq!(group.entity, "entity1");
    assert_eq!(group.context, "route");
    assert_eq!(group.status.as_deref(), Some("approved"));
    assert_eq!(group.valid_time, Some(TimeInterval { start: 1, end: 2 }));
}
#[test]
fn model_free_definition_reference_must_match_exact_definition_bytes() {
    let mut s = snapshot();
    s.records[0].definition_manifest = eidetic::ManifestId::of_blob(b"wrong-definition");
    let q = RecordQuery {
        operation: RecordOperation::PropertyLookup,
        filters: RecordFilters::default(),
    };
    assert!(matches!(
        evaluate(&s, &q, &RecordSettings::default()),
        Err(ResearchError::Invalid(_))
    ));
}
#[test]
fn model_free_pair_results_refuse_multiplied_output_bytes() {
    let mut s = snapshot();
    for (i, r) in s.records.iter_mut().enumerate() {
        r.id = format!("{}-{i}", "x".repeat(2048));
        s.lifecycle[i].record_id = r.id.clone();
    }
    let q = RecordQuery {
        operation: RecordOperation::Compare,
        filters: RecordFilters::default(),
    };
    let settings = RecordSettings {
        max_output_bytes: 4096,
        ..Default::default()
    };
    assert_eq!(
        evaluate(&s, &q, &settings).unwrap_err(),
        ResearchError::LimitExceeded
    );
}
#[test]
fn model_free_definition_validation_work_is_bounded() {
    let mut s = snapshot();
    s.definitions[0].id = format!("test.{}", "x".repeat(1024));
    for r in &mut s.records {
        r.property = s.definitions[0].id.clone();
    }
    bind_definition(&mut s);
    let q = RecordQuery {
        operation: RecordOperation::PropertyLookup,
        filters: RecordFilters::default(),
    };
    let settings = RecordSettings {
        max_validation_bytes: 1000,
        ..Default::default()
    };
    assert_eq!(
        evaluate(&s, &q, &settings).unwrap_err(),
        ResearchError::LimitExceeded
    );
}
