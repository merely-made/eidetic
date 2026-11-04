// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;

fn seed(url: &str, title: Option<&str>) -> ImportedPageSeed {
    ImportedPageSeed {
        canonical_url: url.to_string(),
        normalized_title: title.map(str::to_string),
        raw_url: None,
        raw_title: None,
        favicon_url: None,
    }
}

#[test]
fn bookmarks_become_sorted_imported_origin_events() {
    let items = vec![
        eidetic::import::ImportedBookmarkItem {
            page: seed("https://b.example", Some("B")),
            bookmark_id: None,
            folder_path: Vec::new(),
            location: eidetic::import::BookmarkLocation::Toolbar,
            created_at_unix_secs: Some(200),
            modified_at_unix_secs: None,
            tags: Vec::new(),
        },
        eidetic::import::ImportedBookmarkItem {
            page: seed("https://a.example", Some("A")),
            bookmark_id: None,
            folder_path: Vec::new(),
            location: eidetic::import::BookmarkLocation::Menu,
            created_at_unix_secs: Some(100),
            modified_at_unix_secs: None,
            tags: Vec::new(),
        },
    ];
    let events = events_from_bookmarks(&items);
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].to.url, "https://a.example", "sorted by time");
    assert_eq!(events[0].at_ms, 100_000);
    assert!(
        events
            .iter()
            .all(|e| e.transition == TraceTransition::Imported && e.from.is_none())
    );
}

#[test]
fn history_visits_keep_real_transitions_and_referrers() {
    let jsonl = r#"{"page":{"canonical_url":"https://b.example","normalized_title":"B","raw_url":null,"raw_title":null,"favicon_url":null},"visit_id":"2","visited_at_unix_secs":200,"visit_count_hint":3,"transition":"Link","referring_url":"https://a.example","session_context":null}"#;
    let item: ImportedHistoryVisitItem = serde_json::from_str(jsonl).unwrap();
    let events = events_from_history(&[item]);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].transition, TraceTransition::LinkClick);
    assert_eq!(
        events[0].from.as_ref().map(|p| p.url.as_str()),
        Some("https://a.example")
    );
    assert_eq!(events[0].at_ms, 200_000);
}

#[test]
fn large_imports_chunk_into_segment_sized_traces() {
    let events: Vec<TraceEvent> = (0..(SEGMENT + 3))
        .map(|i| TraceEvent {
            from: None,
            to: PageRef {
                url: format!("https://p{i}.example"),
                title: None,
            },
            transition: TraceTransition::Imported,
            at_ms: i as u64,
            dwell_ms: None,
            candidates: Vec::new(),
        })
        .collect();
    let traces = traces_from_events("mark", events);
    assert_eq!(traces.len(), 2);
    assert_eq!(traces[0].events.len(), SEGMENT);
    assert_eq!(traces[1].events.len(), 3);
    assert_eq!(traces[0].owner, "mark");
}

#[test]
fn ingest_index_search_round_trips_through_real_stores() {
    pollster::block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let mut store = FjallStore::open(dir.path().join("recall")).unwrap();
        eidetic::bootstrap(&mut store).await.unwrap();
        bootstrap_browsing_schema(&mut store).await.unwrap();

        let events = vec![
            TraceEvent {
                from: None,
                to: PageRef {
                    url: "https://a.example".into(),
                    title: Some("alpha notes".into()),
                },
                transition: TraceTransition::UrlTyped,
                at_ms: 1_000,
                dwell_ms: None,
                candidates: Vec::new(),
            },
            TraceEvent {
                from: Some(PageRef {
                    url: "https://a.example".into(),
                    title: None,
                }),
                to: PageRef {
                    url: "https://b.example".into(),
                    title: Some("beta manual".into()),
                },
                transition: TraceTransition::LinkClick,
                at_ms: 2_000,
                dwell_ms: None,
                candidates: Vec::new(),
            },
        ];
        let traces = traces_from_events("mark", events);
        ingest(&mut store, &traces).await.unwrap();

        let memory = load(&mut store).await.unwrap();
        assert_eq!(memory.recent_corridor(10).len(), 2);
        assert_eq!(
            memory.co_occurrence("https://a.example", "https://b.example"),
            1
        );

        // Mint the index from the corpus and recall by title word.
        let index_dir = dir.path().join("recall.index");
        let index = mint_index(&memory, index_dir.to_str().unwrap()).unwrap();
        assert_eq!(index.doc_count().unwrap(), 2);
        let hits = index.search("beta", 5).unwrap();
        assert_eq!(hits[0].url, "https://b.example");
    });
}
