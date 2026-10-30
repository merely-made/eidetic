// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. SPDX-License-Identifier: MPL-2.0
use eidetic::Hash;
use eidetic::models::recall::{RecallSourceId, RecallSourceKind, RetainedSourceText};
use eidetic_search::compare::*;

fn source(id: &str, version: &str, text: &str) -> RetainedSourceText {
    RetainedSourceText {
        source: RecallSourceId {
            kind: RecallSourceKind::Capture,
            owner: Some("owner".into()),
            source: id.into(),
            node: None,
        },
        version: version.into(),
        title: None,
        text: text.into(),
    }
}
fn reconstruct(
    text: &str,
    spans: impl Iterator<Item = eidetic::models::recall::PassageSelector>,
) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut pos = 0;
    let mut result = String::new();
    for span in spans {
        assert_eq!(span.start, pos);
        assert!(span.end >= span.start);
        result.extend(&chars[span.start..span.end]);
        pos = span.end;
    }
    assert_eq!(pos, chars.len());
    result
}
fn check(old: &str, new: &str) -> Comparison {
    let a = source("a", "old", old);
    let b = source("a", "current", new);
    let result = compare(
        &a,
        &b,
        ComparisonRelationship::OwnerDeclaredVersions,
        &ComparisonSettings::default(),
    )
    .unwrap();
    assert_eq!(result.old.source, a.source);
    assert_eq!(result.new.source, b.source);
    assert_eq!(result.old.version, "old");
    assert_eq!(result.new.version, "current");
    assert_eq!(result.old.text_hash, Hash::of(old.as_bytes()));
    assert_eq!(result.new.text_hash, Hash::of(new.as_bytes()));
    assert_eq!(
        reconstruct(old, result.spans.iter().filter_map(|s| s.old.clone())),
        old
    );
    assert_eq!(
        reconstruct(new, result.spans.iter().filter_map(|s| s.new.clone())),
        new
    );
    result
}
#[test]
fn model_free_equal_and_line_edits_reconstruct() {
    let equal = check("α\r\nlast", "α\r\nlast");
    assert!(equal.spans.iter().all(|s| s.kind == SpanKind::Equal));
    let edited = check("a\nrepeat\nrepeat\n終", "a\ninsert\nrepeat\n終\n");
    assert!(edited.spans.iter().any(|s| s.kind == SpanKind::Delete));
    assert!(edited.spans.iter().any(|s| s.kind == SpanKind::Insert));
    assert_eq!(
        edited,
        check("a\nrepeat\nrepeat\n終", "a\ninsert\nrepeat\n終\n")
    );
    check("", "😀\n");
    check("😀\n", "");
}
#[test]
fn model_free_sources_require_explicit_choice() {
    let a = source("a", "1", "same");
    let b = source("b", "1", "same");
    assert_eq!(
        compare(
            &a,
            &b,
            ComparisonRelationship::OwnerDeclaredVersions,
            &ComparisonSettings::default()
        ),
        Err(ComparisonError::DifferentSources)
    );
    assert!(
        compare(
            &a,
            &b,
            ComparisonRelationship::Explicit,
            &ComparisonSettings::default()
        )
        .is_ok()
    );
}
#[test]
fn model_free_limits_refuse() {
    let a = source("a", "1", "a\nb\n");
    let b = source("a", "2", "c\nd\n");
    let mut settings = ComparisonSettings::default();
    settings.max_input_bytes = 3;
    assert_eq!(
        compare(&a, &b, ComparisonRelationship::Explicit, &settings),
        Err(ComparisonError::InputLimit)
    );
    settings.max_input_bytes = 100;
    settings.max_diff_work = 3;
    assert_eq!(
        compare(&a, &b, ComparisonRelationship::Explicit, &settings),
        Err(ComparisonError::WorkLimit)
    );
    settings.max_diff_work = 100;
    settings.max_output_spans = 1;
    assert_eq!(
        compare(&a, &b, ComparisonRelationship::Explicit, &settings),
        Err(ComparisonError::OutputLimit)
    );
}

#[test]
fn model_free_identity_and_zero_limits_are_bounded() {
    let a = source("long identity", "1", "");
    let b = source("long identity", "2", "");
    let mut settings = ComparisonSettings::default();
    settings.max_identity_bytes = 2;
    assert_eq!(
        compare(&a, &b, ComparisonRelationship::Explicit, &settings),
        Err(ComparisonError::InputLimit)
    );
    settings.max_identity_bytes = usize::MAX;
    settings.max_input_bytes = 0;
    assert!(
        compare(&a, &b, ComparisonRelationship::Explicit, &settings)
            .unwrap()
            .spans
            .is_empty()
    );
    let c = source("long identity", "3", "x");
    assert_eq!(
        compare(&a, &c, ComparisonRelationship::Explicit, &settings),
        Err(ComparisonError::InputLimit)
    );
    settings.max_input_bytes = usize::MAX;
    settings.max_diff_work = 0;
    assert_eq!(
        compare(&a, &c, ComparisonRelationship::Explicit, &settings),
        Err(ComparisonError::WorkLimit)
    );
}

#[test]
fn model_free_long_repeated_lines_consume_comparison_work() {
    let line = format!("{}\n", "x".repeat(1000));
    let a = source("a", "1", &line.repeat(16));
    let b = source("a", "2", &format!("{}y\n", line.repeat(15)));
    let settings = ComparisonSettings {
        max_diff_work: 1000,
        ..Default::default()
    };
    assert_eq!(
        compare(&a, &b, ComparisonRelationship::Explicit, &settings),
        Err(ComparisonError::WorkLimit)
    );
}

#[test]
fn model_free_ties_prefer_deletion_with_exact_selectors() {
    use eidetic::models::recall::PassageSelector;
    let result = check("a\nb\n", "b\na\n");
    assert_eq!(
        result.spans,
        vec![
            ComparisonSpan {
                kind: SpanKind::Delete,
                old: Some(PassageSelector { start: 0, end: 2 }),
                new: None
            },
            ComparisonSpan {
                kind: SpanKind::Equal,
                old: Some(PassageSelector { start: 2, end: 4 }),
                new: Some(PassageSelector { start: 0, end: 2 })
            },
            ComparisonSpan {
                kind: SpanKind::Insert,
                old: None,
                new: Some(PassageSelector { start: 2, end: 4 })
            },
        ]
    );
}
