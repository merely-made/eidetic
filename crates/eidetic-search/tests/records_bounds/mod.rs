// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. SPDX-License-Identifier: MPL-2.0
use super::*;

#[test]
fn model_free_compare_charges_repeated_long_values_to_work_limit() {
    let mut s = snapshot();
    s.definitions[0].value_type = ValueType::Text;
    s.records.clear();
    s.lifecycle.clear();
    for i in 0..8 {
        let id = format!("record-{i}");
        let mut r = record(&id, "entity1", 0);
        r.value = PropertyValue::Text("long value".repeat(200));
        s.records.push(r);
        s.lifecycle.push(LifecycleObservation {
            record_id: id.clone(),
            lifecycle_ref: id,
            state: LifecycleState::Active,
        });
    }
    bind_definition(&mut s);
    let query = RecordQuery {
        operation: RecordOperation::Compare,
        filters: RecordFilters::default(),
    };
    let bounded = RecordSettings {
        max_work: 10_000,
        ..Default::default()
    };
    assert_eq!(
        evaluate(&s, &query, &bounded).unwrap_err(),
        ResearchError::LimitExceeded
    );
    let adequate = RecordSettings {
        max_work: 1_000_000,
        ..Default::default()
    };
    let rows = evaluate(&s, &query, &adequate).unwrap().rows;
    assert_eq!(rows.len(), 28);
    assert!(
        rows.iter()
            .all(|row| row.key == RowValue::Comparison(Comparison::Equal))
    );
}
#[test]
fn model_free_aggregate_output_budget_covers_exact_rows_before_emission() {
    let mut s = snapshot();
    s.records.clear();
    s.lifecycle.clear();
    s.sources[0].source.source = "source".repeat(500);
    for i in 0..12 {
        let id = format!("{}-{i}", "identifier".repeat(100));
        let mut r = record(&id, "entity1", 930);
        if let Evidence::Field { source, .. } = &mut r.evidence {
            *source = s.sources[0].source.clone();
        }
        s.records.push(r);
        s.lifecycle.push(LifecycleObservation {
            record_id: id.clone(),
            lifecycle_ref: id,
            state: LifecycleState::Active,
        });
    }
    for operation in [
        RecordOperation::CountByProducer,
        RecordOperation::CountBySource,
        RecordOperation::GroupValues,
        RecordOperation::Sum,
    ] {
        let query = RecordQuery {
            operation,
            filters: RecordFilters::default(),
        };
        let unrestricted = evaluate(&s, &query, &RecordSettings::default()).unwrap();
        assert_eq!(unrestricted.rows.len(), 1);
        let size = serde_json::to_vec(&unrestricted.rows[0]).unwrap().len() + 1;
        let exact = RecordSettings {
            max_output_bytes: size,
            ..Default::default()
        };
        assert_eq!(
            evaluate(&s, &query, &exact).unwrap().rows,
            unrestricted.rows
        );
        let short = RecordSettings {
            max_output_bytes: size - 1,
            ..Default::default()
        };
        assert_eq!(
            evaluate(&s, &query, &short).unwrap_err(),
            ResearchError::LimitExceeded
        );
        let tiny = RecordSettings {
            max_output_bytes: 8,
            ..Default::default()
        };
        assert_eq!(
            evaluate(&s, &query, &tiny).unwrap_err(),
            ResearchError::LimitExceeded
        );
    }
}
#[test]
fn model_free_decimal_sum_borrowed_preflight_matches_owned_row() {
    let mut s = snapshot();
    s.definitions[0].value_type = ValueType::Decimal;
    s.definitions[0].unit = Some("kg".into());
    for r in &mut s.records {
        r.value = PropertyValue::Decimal {
            coefficient: 100,
            scale: 2,
            unit: "kg".into(),
        };
    }
    bind_definition(&mut s);
    let query = RecordQuery {
        operation: RecordOperation::Sum,
        filters: RecordFilters::default(),
    };
    let result = evaluate(&s, &query, &RecordSettings::default()).unwrap();
    assert_eq!(
        result.rows[0].key,
        RowValue::Value(PropertyValue::Decimal {
            coefficient: 300,
            scale: 2,
            unit: "kg".into()
        })
    );
    let bytes = serde_json::to_vec(&result.rows[0]).unwrap().len() + 1;
    let settings = RecordSettings {
        max_output_bytes: bytes,
        ..Default::default()
    };
    assert_eq!(evaluate(&s, &query, &settings).unwrap().rows, result.rows);
    let settings = RecordSettings {
        max_output_bytes: bytes - 1,
        ..Default::default()
    };
    assert_eq!(
        evaluate(&s, &query, &settings).unwrap_err(),
        ResearchError::LimitExceeded
    );
}
#[test]
fn model_free_group_output_budget_covers_long_value_and_identity() {
    let mut s = snapshot();
    s.definitions[0].value_type = ValueType::Text;
    for r in &mut s.records {
        r.value = PropertyValue::Text("long value".repeat(1000));
    }
    bind_definition(&mut s);
    let query = RecordQuery {
        operation: RecordOperation::GroupValues,
        filters: RecordFilters::default(),
    };
    let r = evaluate(&s, &query, &RecordSettings::default()).unwrap();
    let size = serde_json::to_vec(&r.rows[0]).unwrap().len() + 1;
    assert_eq!(
        evaluate(
            &s,
            &query,
            &RecordSettings {
                max_output_bytes: size,
                ..Default::default()
            }
        )
        .unwrap()
        .rows,
        r.rows
    );
    assert_eq!(
        evaluate(
            &s,
            &query,
            &RecordSettings {
                max_output_bytes: size - 1,
                ..Default::default()
            }
        )
        .unwrap_err(),
        ResearchError::LimitExceeded
    );
}
