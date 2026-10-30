// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. SPDX-License-Identifier: MPL-2.0

use eidetic::models::recall::{
    PassageSelector, RecallSourceId, RecallSourceKind, RetainedSourceText,
};
use eidetic_search::query::*;

fn source(name: &str, kind: ProducerKind, text: &str) -> ResearchSource {
    ResearchSource {
        retained: RetainedSourceText {
            source: RecallSourceId {
                kind: RecallSourceKind::Fleece,
                owner: Some("owner".into()),
                source: name.into(),
                node: None,
            },
            version: "v1".into(),
            title: Some(format!("title {name}")),
            text: text.into(),
        },
        producer: kind,
        address: Some(format!("https://example.test/{name}")),
        date: Some(10),
        tags: vec![],
    }
}
fn snapshot(sources: Vec<ResearchSource>) -> ResearchSnapshot {
    ResearchSnapshot {
        generation: "session-1".into(),
        scope: sources.iter().map(|s| s.retained.source.clone()).collect(),
        sources,
        coverage: Coverage::Complete,
    }
}
fn run(s: &ResearchSnapshot, mode: QueryMode) -> SearchResult {
    search(
        s,
        &SearchQuery {
            mode,
            ..Default::default()
        },
        &SearchSettings::default(),
    )
    .unwrap()
}
fn names(r: &SearchResult) -> Vec<&str> {
    r.hits.iter().map(|h| h.source.source.as_str()).collect()
}

#[test]
fn model_free_mixed_kinds_and_filters_use_explicit_metadata() {
    let tag = TagId {
        owner: "owner".into(),
        concept: "topic".into(),
    };
    let mut a = source("a", ProducerKind::Capture, "shared text");
    let mut b = source("b", ProducerKind::Selection, "shared text");
    let mut c = source("c", ProducerKind::Note, "shared text");
    a.tags.push(tag.clone());
    b.date = None;
    c.tags.push(tag.clone());
    c.date = Some(20);
    let s = snapshot(vec![a.clone(), b, c.clone()]);
    assert_eq!(run(&s, QueryMode::Terms("shared".into())).total, 3);
    let q = SearchQuery {
        mode: QueryMode::Browse,
        sources: Some(vec![a.retained.source.clone(), c.retained.source.clone()]),
        kinds: vec![ProducerKind::Note],
        date_from: Some(15),
        date_to: Some(25),
        tags: vec![tag],
        ..Default::default()
    };
    assert_eq!(
        names(&search(&s, &q, &SearchSettings::default()).unwrap()),
        ["c"]
    );
    let q = SearchQuery {
        mode: QueryMode::Browse,
        date_from: Some(1),
        ..Default::default()
    };
    assert_eq!(search(&s, &q, &SearchSettings::default()).unwrap().total, 2);
}

#[test]
fn model_free_phrase_checks_whole_source_and_never_joins_sources() {
    let s = snapshot(vec![
        source(
            "a",
            ProducerKind::Capture,
            &format!("{}alpha beta", "x".repeat(1023)),
        ),
        source("b", ProducerKind::Note, "alpha"),
        source("c", ProducerKind::Selection, "beta"),
    ]);
    let result = run(&s, QueryMode::Phrase("alpha beta".into()));
    assert_eq!(names(&result), ["a"]);
    assert_eq!(
        result.hits[0].matches,
        vec![PassageSelector {
            start: 1023,
            end: 1033
        }]
    );
    assert!(
        run(&s, QueryMode::Phrase("alpha alpha".into()))
            .hits
            .is_empty()
    );
}

#[test]
fn model_free_prefix_is_explicit_and_alias_expansion_is_visible() {
    let s = snapshot(vec![
        source("a", ProducerKind::Note, "garden"),
        source("b", ProducerKind::Capture, "orchard"),
    ]);
    assert!(run(&s, QueryMode::Terms("gard".into())).hits.is_empty());
    assert_eq!(names(&run(&s, QueryMode::Prefix("gard".into()))), ["a"]);
    let q = SearchQuery {
        mode: QueryMode::Terms("garden".into()),
        aliases: vec![Alias {
            term: "garden".into(),
            expansions: vec!["orchard".into()],
        }],
        ..Default::default()
    };
    let result = search(&s, &q, &SearchSettings::default()).unwrap();
    assert_eq!(result.total, 2);
    assert_eq!(result.resolved_terms, vec!["garden", "orchard"]);
}

#[test]
fn model_free_metadata_only_hits_have_no_body_passage() {
    let s = snapshot(vec![source("a", ProducerKind::Capture, "unrelated body")]);
    for mode in [
        QueryMode::Terms("title".into()),
        QueryMode::Address("https://example.test/a".into()),
    ] {
        let result = run(&s, mode);
        assert_eq!(result.total, 1);
        assert!(result.hits[0].passage.is_none());
        assert!(result.hits[0].matches.is_empty());
    }
}

#[test]
fn model_free_pagination_is_stable_and_does_not_change_coverage() {
    let s = snapshot(vec![
        source("a", ProducerKind::Note, "same"),
        source("b", ProducerKind::Note, "same"),
        source("c", ProducerKind::Note, "same"),
    ]);
    let q = SearchQuery {
        mode: QueryMode::Terms("same".into()),
        ..Default::default()
    };
    let all = search(&s, &q, &SearchSettings::default()).unwrap();
    let mut settings = SearchSettings {
        limit: 1,
        ..Default::default()
    };
    let mut paged = Vec::new();
    for offset in 0..3 {
        settings.offset = offset;
        let page = search(&s, &q, &settings).unwrap();
        assert_eq!(page.total, 3);
        assert_eq!(page.coverage, Coverage::Complete);
        paged.extend(page.hits.into_iter().map(|h| h.source));
    }
    assert_eq!(
        paged,
        all.hits.into_iter().map(|h| h.source).collect::<Vec<_>>()
    );
}

#[test]
fn model_free_empty_and_incomplete_scope_remain_distinct() {
    let mut s = snapshot(vec![source("a", ProducerKind::Note, "text")]);
    s.scope.clear();
    assert_eq!(run(&s, QueryMode::Browse).total, 0);
    s.coverage = Coverage::Incomplete {
        exclusions: vec!["unavailable source".into()],
    };
    let result = run(&s, QueryMode::Terms("absent".into()));
    assert_eq!(result.total, 0);
    assert_eq!(result.coverage, s.coverage);
}

#[test]
fn model_free_unicode_ranges_and_hash_retain_exact_source() {
    let s = snapshot(vec![source(
        "a",
        ProducerKind::Selection,
        "a 🦉 e\u{301} fin",
    )]);
    let result = run(&s, QueryMode::Phrase("🦉 e\u{301}".into()));
    let hit = &result.hits[0];
    assert_eq!(hit.matches, vec![PassageSelector { start: 2, end: 6 }]);
    let p = hit.passage.as_ref().unwrap();
    assert_eq!(p.text, "a 🦉 e\u{301} fin");
    assert_eq!(
        p.text_hash,
        eidetic::Hash::of("a 🦉 e\u{301} fin".as_bytes())
    );
    assert_eq!(p.source, s.sources[0].retained.source);
    assert_eq!(p.version, "v1");
}

#[test]
fn model_free_invalid_bounds_and_duplicate_sources_are_rejected() {
    let a = source("a", ProducerKind::Note, "text");
    let s = snapshot(vec![a.clone()]);
    let q = SearchQuery::default();
    assert!(
        search(
            &s,
            &q,
            &SearchSettings {
                limit: 0,
                ..Default::default()
            }
        )
        .is_err()
    );
    assert!(
        search(
            &s,
            &q,
            &SearchSettings {
                max_corpus_chars: 3,
                ..Default::default()
            }
        )
        .is_err()
    );
    assert!(
        search(
            &snapshot(vec![a.clone(), a]),
            &q,
            &SearchSettings::default()
        )
        .is_err()
    );
}

#[test]
fn model_free_missing_scoped_source_downgrades_coverage_and_filter_cannot_widen_scope() {
    let a = source("a", ProducerKind::Note, "shared");
    let b = source("b", ProducerKind::Note, "shared");
    let mut s = snapshot(vec![a.clone(), b.clone()]);
    s.scope = vec![a.retained.source.clone()];
    let q = SearchQuery {
        sources: Some(vec![b.retained.source.clone()]),
        ..Default::default()
    };
    assert!(
        search(&s, &q, &SearchSettings::default())
            .unwrap()
            .hits
            .is_empty()
    );
    s.sources.clear();
    let result = run(&s, QueryMode::Browse);
    assert!(matches!(result.coverage, Coverage::Incomplete { .. }));
}

#[test]
fn model_free_invalid_metadata_and_nonfinite_weights_are_rejected() {
    let mut a = source("a", ProducerKind::Note, "text");
    a.tags.push(TagId {
        owner: "".into(),
        concept: "topic".into(),
    });
    assert!(
        search(
            &snapshot(vec![a]),
            &SearchQuery::default(),
            &SearchSettings::default()
        )
        .is_err()
    );
    let s = snapshot(vec![source("a", ProducerKind::Note, "text")]);
    let mut settings = SearchSettings::default();
    settings.weights.text = f32::NAN;
    assert!(search(&s, &SearchQuery::default(), &settings).is_err());
}

#[test]
fn model_free_input_identity_records_metadata_used_by_queries() {
    let a = source("a", ProducerKind::Capture, "same");
    let mut changed = a.clone();
    changed.producer = ProducerKind::Note;
    changed.date = None;
    changed.tags.push(TagId {
        owner: "o".into(),
        concept: "c".into(),
    });
    assert_ne!(
        run(&snapshot(vec![a]), QueryMode::Browse).inputs,
        run(&snapshot(vec![changed]), QueryMode::Browse).inputs
    );
}

#[test]
fn model_free_repeated_matches_on_late_page_keep_exact_ranges() {
    let text = "🦉 word ".repeat(20_000);
    let s = snapshot(vec![
        source("a", ProducerKind::Note, &text),
        source("b", ProducerKind::Note, &text),
    ]);
    let settings = SearchSettings {
        offset: 1,
        limit: 1,
        ..Default::default()
    };
    for mode in [
        QueryMode::Terms("word".into()),
        QueryMode::Phrase("🦉 word".into()),
    ] {
        let result = search(
            &s,
            &SearchQuery {
                mode,
                ..Default::default()
            },
            &settings,
        )
        .unwrap();
        assert_eq!(result.total, 2);
        assert_eq!(result.hits[0].matches.len(), 20_000);
        assert_eq!(result.hits[0].matches.last().unwrap().end, 139_999);
    }
}

#[test]
fn model_free_oversized_metadata_query_and_aliases_are_refused() {
    let mut a = source("a", ProducerKind::Note, "x");
    a.retained.title = Some("long title".into());
    let settings = SearchSettings {
        max_metadata_chars: 10,
        ..Default::default()
    };
    assert!(search(&snapshot(vec![a]), &SearchQuery::default(), &settings).is_err());
    let s = snapshot(vec![source("a", ProducerKind::Note, "x")]);
    let settings = SearchSettings {
        max_query_chars: 5,
        ..Default::default()
    };
    assert!(
        search(
            &s,
            &SearchQuery {
                mode: QueryMode::Terms("long query".into()),
                ..Default::default()
            },
            &settings
        )
        .is_err()
    );
    assert!(
        search(
            &s,
            &SearchQuery {
                mode: QueryMode::Terms("x".into()),
                aliases: vec![Alias {
                    term: "x".into(),
                    expansions: vec!["lengthy".into()]
                }],
                ..Default::default()
            },
            &settings
        )
        .is_err()
    );
}

#[test]
fn model_free_collection_bounds_refuse_duplicate_kinds_and_empty_exclusions() {
    let mut s = snapshot(vec![]);
    let q = SearchQuery {
        kinds: vec![ProducerKind::Note, ProducerKind::Note],
        ..Default::default()
    };
    assert!(search(&s, &q, &SearchSettings::default()).is_err());
    s.coverage = Coverage::Incomplete {
        exclusions: vec![String::new(); 20],
    };
    let settings = SearchSettings {
        max_metadata_chars: 15,
        ..Default::default()
    };
    assert!(search(&s, &SearchQuery::default(), &settings).is_err());
}

#[test]
fn model_free_source_filter_count_is_bounded_before_deduplication() {
    let s = snapshot(vec![]);
    let ids = (0..3)
        .map(|i| {
            source(&format!("s{i}"), ProducerKind::Note, "")
                .retained
                .source
                .clone()
        })
        .collect();
    let q = SearchQuery {
        sources: Some(ids),
        ..Default::default()
    };
    let settings = SearchSettings {
        max_sources: 2,
        ..Default::default()
    };
    assert!(search(&s, &q, &settings).is_err());
}
