// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;
use eidetic::browsing::{PageRef, TraceEvent, TraceTransition};

fn event(to: &str, title: &str, at_ms: u64) -> TraceEvent {
    TraceEvent {
        from: None,
        to: PageRef {
            url: to.to_string(),
            title: Some(title.to_string()),
        },
        transition: TraceTransition::LinkClick,
        at_ms,
        dwell_ms: None,
        candidates: Vec::new(),
    }
}

fn corpus() -> Vec<BrowsingTrace> {
    vec![BrowsingTrace::from_events(
        "mark",
        vec![
            event(
                "https://docs.example/vello",
                "vello scene encoding API",
                1_000,
            ),
            event(
                "https://docs.example/tantivy",
                "tantivy index format notes",
                90_000_000,
            ),
            event(
                "https://news.example/wgpu",
                "wgpu 29 release announcement",
                90_500_000,
            ),
        ],
    )]
}

#[test]
fn rebuild_then_search_ranks_the_matching_page() {
    let dir = tempfile::tempdir().unwrap();
    let index = TrailIndex::rebuild(dir.path().join("idx"), &corpus()).unwrap();
    assert_eq!(index.doc_count().unwrap(), 3);

    let hits = index.search("vello scene", 5).unwrap();
    assert!(!hits.is_empty());
    assert_eq!(hits[0].url, "https://docs.example/vello");
    assert_eq!(hits[0].at_ms, 1_000);
    assert!(hits[0].title.as_deref().unwrap_or("").contains("vello"));
}

#[test]
fn body_text_is_recallable_via_rebuild_with_text() {
    let dir = tempfile::tempdir().unwrap();
    let traces = vec![BrowsingTrace::from_events(
        "m",
        vec![event("https://a.test/page", "Plain Title", 1_000)],
    )];
    let texts: std::collections::HashMap<String, String> = [(
        "https://a.test/page".to_string(),
        "quantum entanglement field notes".to_string(),
    )]
    .into_iter()
    .collect();
    let index =
        TrailIndex::rebuild_with_text(dir.path().join("idx"), &traces, |u| texts.get(u).cloned())
            .unwrap();
    // A term that appears only in the body — not the title or URL — still
    // recalls the page (the C5 payoff).
    let hits = index.search("entanglement", 5).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].url, "https://a.test/page");
}

#[test]
fn titleless_url_components_are_recallable() {
    let dir = tempfile::tempdir().unwrap();
    let trace = BrowsingTrace::from_events(
        "m",
        vec![TraceEvent {
            from: None,
            to: PageRef {
                url: "https://docs.example/rust/async/book/getting-started".to_string(),
                title: None,
            },
            transition: TraceTransition::Imported,
            at_ms: 1_000,
            dwell_ms: None,
            candidates: Vec::new(),
        }],
    );
    let index = TrailIndex::rebuild(dir.path().join("idx"), [&trace]).unwrap();

    let hits = index.search("rust async book", 5).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(
        hits[0].url,
        "https://docs.example/rust/async/book/getting-started"
    );
    assert_eq!(hits[0].title, None);

    let exact = index
        .search("https://docs.example/rust/async/book/getting-started", 5)
        .unwrap();
    assert_eq!(exact[0].url, hits[0].url);
}

#[test]
fn open_refuses_a_drifted_spec_and_rebuild_recovers() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("idx");
    TrailIndex::rebuild(&path, &corpus()).unwrap();

    // Sabotage the sidecar to an older format.
    let drifted = SearchIndexSpec {
        engine_version: "tantivy 0.1.0".to_string(),
        ..SearchIndexSpec::current()
    };
    drifted.write_sidecar(&path).unwrap();

    match TrailIndex::open(&path) {
        Err(SearchError::FormatMismatch { found, current }) => {
            assert!(found.contains("0.1.0"));
            assert!(!current.is_empty());
        }
        other => panic!("expected FormatMismatch, got {:?}", other.err()),
    }

    // The re-mint path: rebuild from the corpus and search again.
    let index = TrailIndex::rebuild(&path, &corpus()).unwrap();
    let hits = index.search("wgpu", 5).unwrap();
    assert_eq!(hits[0].url, "https://news.example/wgpu");

    // And a normal reopen now succeeds.
    let reopened = TrailIndex::open(&path).unwrap();
    assert_eq!(reopened.doc_count().unwrap(), 3);
    assert_eq!(
        reopened.search("wgpu", 5).unwrap()[0].url,
        "https://news.example/wgpu"
    );
}

#[test]
fn open_refuses_v3_fields_and_rebuild_recovers_url_recall() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("idx");
    let traces = corpus();
    TrailIndex::rebuild(&path, &traces).unwrap();

    SearchIndexSpec {
        fields_version: crate::spec::FIELDS_V3,
        ..SearchIndexSpec::current()
    }
    .write_sidecar(&path)
    .unwrap();

    match TrailIndex::open(&path) {
        Err(SearchError::FormatMismatch { found, current }) => {
            assert!(found.contains("fields v3"));
            assert!(current.contains("fields v4"));
        }
        other => panic!("expected field-version mismatch, got {:?}", other.err()),
    }

    let rebuilt = TrailIndex::rebuild(&path, &traces).unwrap();
    assert_eq!(
        rebuilt.search("docs example tantivy", 5).unwrap()[0].url,
        "https://docs.example/tantivy"
    );
}

/// A directory a tantivy build left behind refuses cleanly rather than
/// being misread: the sidecar's engine string is the tell.
#[test]
fn a_tantivy_era_sidecar_refuses_as_a_format_mismatch() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("idx");
    std::fs::create_dir_all(&path).unwrap();
    std::fs::write(
        path.join(crate::spec::SPEC_SIDECAR),
        br#"{"tantivy_version":"tantivy 0.26.0","fields_version":3,"tokenizer":"default"}"#,
    )
    .unwrap();

    match TrailIndex::open(&path) {
        Err(SearchError::FormatMismatch { found, .. }) => {
            assert!(found.contains("tantivy"), "{found}");
        }
        other => panic!("expected FormatMismatch, got {:?}", other.err()),
    }
}

#[test]
fn opening_nothing_is_missing_not_a_panic() {
    let dir = tempfile::tempdir().unwrap();
    assert!(matches!(
        TrailIndex::open(dir.path().join("nowhere")),
        Err(SearchError::Missing(_))
    ));
}

#[test]
fn a_transient_index_writes_nothing_and_still_recalls() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("idx");
    let index = TrailIndex::rebuild_with_config(
        &path,
        &corpus(),
        |_| None,
        IndexConfig::default(), // persist defaults to false
    )
    .unwrap();
    assert_eq!(
        index.search("wgpu", 5).unwrap()[0].url,
        "https://news.example/wgpu"
    );
    assert!(!path.exists());
    assert!(matches!(
        TrailIndex::open(&path),
        Err(SearchError::Missing(_))
    ));
}

#[test]
fn field_weights_are_settings_the_ranking_follows() {
    let dir = tempfile::tempdir().unwrap();
    let trace = BrowsingTrace::from_events(
        "m",
        vec![
            event("https://a.test/wombat", "unrelated heading", 1),
            event("https://b.test/page", "wombat", 2),
        ],
    );
    let title_first = TrailIndex::rebuild(dir.path().join("a"), [&trace]).unwrap();
    assert_eq!(
        title_first.search("wombat", 5).unwrap()[0].url,
        "https://b.test/page"
    );

    let url_first = TrailIndex::rebuild_with_config(
        dir.path().join("b"),
        [&trace],
        |_| None,
        IndexConfig {
            weights: FieldWeights {
                url: 10.0,
                ..FieldWeights::default()
            },
            ..IndexConfig::default()
        },
    )
    .unwrap();
    assert_eq!(
        url_first.search("wombat", 5).unwrap()[0].url,
        "https://a.test/wombat"
    );
}

#[test]
fn reports_run_over_the_stored_columns() {
    let dir = tempfile::tempdir().unwrap();
    let index = TrailIndex::rebuild(dir.path().join("idx"), &corpus()).unwrap();

    let domains = index.top_domains(5).unwrap();
    assert_eq!(
        domains.first().map(|(d, c)| (d.as_str(), *c)),
        Some(("docs.example", 2))
    );
    assert!(domains.iter().any(|(d, c)| d == "news.example" && *c == 1));

    // Day buckets: two events fall in day 1 (86.4M..172.8M), one in day 0.
    let histogram = index.visits_histogram(86_400_000).unwrap();
    assert_eq!(histogram.len(), 2);
    assert_eq!(histogram[0], (0, 1));
    assert_eq!(histogram[1], (86_400_000, 2));
}

#[test]
fn domain_extraction_is_boring_and_correct() {
    assert_eq!(domain_of("https://Docs.Example/path?q=1"), "docs.example");
    assert_eq!(domain_of("gemini://smol.host"), "smol.host");
    assert_eq!(domain_of("no-scheme/path"), "no-scheme");
    assert!(is_absolute_url_query("https://docs.example/path?q=1"));
    assert!(is_absolute_url_query("gemini://smol.host"));
    assert!(!is_absolute_url_query("https://docs.example one more term"));
    assert!(!is_absolute_url_query("where did I read this"));
}
