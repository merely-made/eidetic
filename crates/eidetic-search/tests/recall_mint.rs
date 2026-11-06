// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The recall index minted over a trace corpus, moved from Turnstone's
//! `trail_memory` tests onto the promoted `recall_mint` module.

use eidetic::{BrowsingMemory, BrowsingTrace, PageRef, PageTexts, TraceEvent, TraceTransition};
use eidetic_search::recall_mint::{
    MintObserver, MintPhase, MintedRecall, RecallConfig, RecallSource, canonical_sources, index_dir,
};
use std::collections::HashMap;
use std::path::Path;

const SEGMENT_SIZE: usize = BrowsingMemory::DEFAULT_SEGMENT_SIZE;

fn synthetic_page(index: usize) -> (String, String) {
    (
        format!("https://page{index}.example/trail/{index}"),
        format!("Synthetic Page {index}"),
    )
}

/// `events` traversals cycling over `pages` distinct URLs, chained and chunked
/// into segments exactly as the actor stores them.
fn synthetic_traces(events: usize, pages: usize) -> Vec<BrowsingTrace> {
    let mut built = Vec::new();
    let mut segment = Vec::new();
    let mut previous: Option<PageRef> = None;
    for index in 0..events {
        let (url, title) = synthetic_page(index % pages);
        let to = PageRef {
            url,
            title: Some(title),
        };
        segment.push(TraceEvent {
            from: previous.replace(to.clone()),
            to,
            transition: TraceTransition::LinkClick,
            at_ms: index as u64 + 1,
            dwell_ms: None,
            candidates: Vec::new(),
        });
        if segment.len() == SEGMENT_SIZE {
            built.push(BrowsingTrace::from_events(
                "p",
                std::mem::take(&mut segment),
            ));
        }
    }
    if !segment.is_empty() {
        built.push(BrowsingTrace::from_events("p", segment));
    }
    built
}

fn mint(traces: &[BrowsingTrace], now_ms: u64) -> MintedRecall {
    MintedRecall::mint(
        Path::new("unused-memory-dir"),
        traces,
        &PageTexts::default(),
        now_ms,
        &mut (),
    )
    .unwrap()
}

#[test]
fn mint_counts_the_corpus_exactly_and_reports_each_phase() {
    const EVENTS: usize = 1_000;
    const PAGES: usize = 300;
    struct Phases(Vec<MintPhase>);
    impl MintObserver for Phases {
        fn phase_done(&mut self, phase: MintPhase) {
            self.0.push(phase);
        }
    }
    let traces = synthetic_traces(EVENTS, PAGES);
    let mut phases = Phases(Vec::new());
    let index = MintedRecall::mint(
        Path::new("unused-memory-dir"),
        &traces,
        &PageTexts::default(),
        10_000,
        &mut phases,
    )
    .unwrap();
    let counts = index.counts();
    assert_eq!(counts.traces, EVENTS.div_ceil(SEGMENT_SIZE));
    assert_eq!(counts.events, EVENTS);
    assert_eq!(counts.pages, PAGES, "distinct destination URLs");
    let expected: usize = (0..EVENTS)
        .map(|index| {
            let (url, title) = synthetic_page(index % PAGES);
            url.len() + title.len()
        })
        .sum();
    assert_eq!(counts.corpus_bytes, expected);
    assert_eq!(
        phases.0,
        [
            MintPhase::PageTable,
            MintPhase::Lexical,
            MintPhase::Frecency,
            MintPhase::Candidates
        ]
    );
}

#[test]
fn two_urls_for_one_page_recall_once_with_one_frecency() {
    let now = 1_700_000_000_000;
    let addresses = [
        "https://gazette.test/morning?utm_source=newsletter&utm_medium=email",
        "https://gazette.test/morning#lede",
        "https://gazette.test/morning",
    ];
    let events: Vec<TraceEvent> = addresses
        .iter()
        .enumerate()
        .map(|(nth, url)| TraceEvent {
            from: None,
            to: PageRef {
                url: (*url).to_string(),
                title: Some("The Morning Paper".to_string()),
            },
            transition: TraceTransition::UrlTyped,
            at_ms: now - nth as u64 * 1_000,
            dwell_ms: None,
            candidates: Vec::new(),
        })
        .collect();
    let traces = vec![BrowsingTrace::from_events("p", events)];
    let index = mint(&traces, now);
    assert_eq!(index.counts().events, 3);
    assert_eq!(index.counts().pages, 1, "one page behind three addresses");
    assert_eq!(index.counts().collapsed_urls, 2);

    let hits = index
        .search("gazette morning", 5, RecallConfig::default())
        .unwrap();
    assert_eq!(hits.len(), 1, "one page, one row");
    assert_eq!(
        hits[0].url, addresses[0],
        "the row opens the latest address"
    );

    // One combined score, not three split ones — and it is the sum.
    let combined = index.frecency_score(addresses[0]).unwrap();
    let single = mint(
        &[BrowsingTrace::from_events(
            "p",
            traces[0].events[..1].to_vec(),
        )],
        now,
    )
    .frecency_score(addresses[0])
    .unwrap();
    assert!(
        (combined - 3.0 * single).abs() < single * 0.01,
        "three typed visits to one page sum: {combined} vs 3 x {single}"
    );
}

#[test]
fn recall_overlays_current_titles_and_remints_on_change() {
    let url = "https://example.test/page/42";
    let mut memory = BrowsingMemory::new(SEGMENT_SIZE);
    memory.adopt(BrowsingTrace::from_events(
        "p",
        vec![TraceEvent {
            from: None,
            to: PageRef {
                url: url.into(),
                title: None,
            },
            transition: TraceTransition::UrlTyped,
            at_ms: 1,
            dwell_ms: None,
            candidates: Vec::new(),
        }],
    ));
    for (query, title) in [
        ("field notes", "Field Notes"),
        ("harmony map", "Harmony Map"),
    ] {
        let sources = canonical_sources(vec![RecallSource::new(url, title).unwrap()]);
        let titles: HashMap<&str, &str> = sources
            .iter()
            .map(|source| (source.url.as_str(), source.title.as_str()))
            .collect();
        let traces = memory.traces_with_titles(&titles);
        let index = mint(&traces, 1_000);
        let hits = index.search(query, 5, RecallConfig::default()).unwrap();
        assert_eq!(hits.len(), 1, "{query:?} should match the current title");
        assert_eq!(hits[0].url, url);
        assert_eq!(hits[0].title.as_deref(), Some(title));
    }
    // The stored corpus was never rewritten.
    assert_eq!(memory.traces().next().unwrap().events[0].to.title, None);
}

#[test]
fn typed_prefix_recall_follows_frecency_over_title_overlap() {
    const DAY_MS: u64 = 24 * 60 * 60 * 1_000;
    let now = 1_800_000_000_000;
    let target = "https://gazette.test/";
    let decoy = "https://elsewhere.test/gazette/gazette-gazette";
    let event = |url: &str, transition, at_ms| TraceEvent {
        from: None,
        to: PageRef {
            url: url.into(),
            title: None,
        },
        transition,
        at_ms,
        dwell_ms: None,
        candidates: Vec::new(),
    };
    let mut events = vec![event(decoy, TraceTransition::LinkClick, now - 200 * DAY_MS)];
    for days_ago in 0..6 {
        events.push(event(
            target,
            TraceTransition::UrlTyped,
            now - days_ago * DAY_MS,
        ));
    }
    let mut memory = BrowsingMemory::new(SEGMENT_SIZE);
    memory.adopt(BrowsingTrace::from_events("p", events));
    let titles = HashMap::from([
        (target, "The Morning Paper"),
        (decoy, "Gazette Gazette Gazette"),
    ]);
    let index = mint(&memory.traces_with_titles(&titles), now);

    let lexical = index
        .search(
            "gazette",
            5,
            RecallConfig::default().with_frecency_weight(0.0),
        )
        .unwrap();
    assert_eq!(
        lexical[0].url, decoy,
        "BM25 alone follows the repeated title term"
    );
    let behavioural = index.search("gazette", 5, RecallConfig::default()).unwrap();
    assert_eq!(
        behavioural[0].url, target,
        "frecency lifts the page the prefix was typed into six times"
    );
}

#[test]
fn small_helpers_keep_their_contracts() {
    assert_eq!(RecallSource::new("https://a/", "  "), None);
    assert_eq!(RecallSource::new("https://a/", "https://a/"), None);
    let kept = RecallSource::new("https://a/", " A ").unwrap();
    assert_eq!(kept.title, "A");
    let sources = canonical_sources(vec![
        RecallSource::new("https://a/", "Zed").unwrap(),
        RecallSource::new("https://a/", "Alpha").unwrap(),
    ]);
    assert_eq!(sources.len(), 1);
    assert_eq!(
        sources[0].title, "Alpha",
        "sorted, so the pick is deterministic"
    );
    assert_eq!(
        index_dir(Path::new("/s/memory")),
        Path::new("/s/memory.index")
    );
    let clamped = RecallConfig::default();
    assert_eq!(
        clamped.with_frecency_weight(f32::NAN).frecency_weight(),
        0.0
    );
    assert_eq!(clamped.with_frecency_weight(99.0).frecency_weight(), 4.0);
    assert!(!clamped.with_frecency_weight(0.0).frecency_enabled());
}
