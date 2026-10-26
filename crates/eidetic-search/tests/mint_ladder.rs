// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Mint and query cost for the in-tree engine, on a synthetic ladder.
//!
//! One document per page, which is what the consumer now feeds. Run
//! optimized — the debug numbers are not the ones to quote:
//!
//! `python3 /Users/markik/Code/tools/build_storage.py build --lane eidetic-search --cwd . -- test -p eidetic-search --release --test mint_ladder -- --ignored --nocapture`

use std::time::Instant;

use eidetic::browsing::{BrowsingTrace, PageRef, TraceEvent, TraceTransition};
use eidetic_search::{IndexConfig, TrailIndex};

/// Events per trace, matching the rehearsal bin's segment size.
const SEGMENT: usize = 512;

/// A vocabulary wide enough that a query is selective but every term has
/// company in the postings list.
const WORDS: &[&str] = &[
    "vello",
    "scene",
    "encoding",
    "wgpu",
    "render",
    "pipeline",
    "tantivy",
    "index",
    "format",
    "notes",
    "rust",
    "async",
    "book",
    "storage",
    "quota",
    "browser",
    "tab",
    "restore",
    "local",
    "first",
    "software",
    "device",
    "policy",
    "graph",
    "query",
    "nodes",
    "label",
    "reader",
    "mode",
    "typography",
];

/// Synthetic pages: a distinct URL each, a title drawn deterministically from
/// the vocabulary, no bodies (the consumer's shape until W6c lands text).
fn corpus(pages: usize) -> Vec<BrowsingTrace> {
    let events: Vec<TraceEvent> = (0..pages)
        .map(|page| {
            let a = WORDS[page % WORDS.len()];
            let b = WORDS[(page * 7 + 3) % WORDS.len()];
            let c = WORDS[(page * 13 + 11) % WORDS.len()];
            TraceEvent {
                from: None,
                to: PageRef {
                    url: format!("https://host{}.example/{a}/{b}/page-{page}", page % 64),
                    title: Some(format!("{a} {b} {c} page {page}")),
                },
                transition: TraceTransition::Imported,
                at_ms: page as u64 * 1_000,
                dwell_ms: None,
                candidates: Vec::new(),
            }
        })
        .collect();
    events
        .chunks(SEGMENT)
        .map(|chunk| BrowsingTrace::from_events("ladder", chunk.to_vec()))
        .collect()
}

const QUERIES: &[&str] = &[
    "vello scene encoding",
    "rust async book",
    "storage quota browser",
    "reader mode typography",
    "wgpu render pipeline",
];

#[test]
#[ignore = "measurement harness; run explicitly in release mode"]
fn mint_and_query_ladder() {
    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    println!(
        "eidetic-search mint ladder: profile={profile}, os={}, arch={}",
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    println!("pages,mint_ms_transient,mint_ms_persisted,query_us,hits");

    let directory = tempfile::tempdir().unwrap();
    for pages in [1_000_usize, 10_000, 100_000] {
        let traces = corpus(pages);
        let refs: Vec<&BrowsingTrace> = traces.iter().collect();
        let path = directory.path().join(format!("idx-{pages}"));

        let started = Instant::now();
        let index = TrailIndex::rebuild_with_config(
            &path,
            refs.iter().copied(),
            |_| None,
            IndexConfig::default(),
        )
        .unwrap();
        let transient_ms = started.elapsed().as_secs_f64() * 1_000.0;
        assert_eq!(index.doc_count().unwrap(), pages as u64);

        let started = Instant::now();
        TrailIndex::rebuild(&path, refs.iter().copied()).unwrap();
        let persisted_ms = started.elapsed().as_secs_f64() * 1_000.0;

        // Warm, then sweep the query set.
        let mut hits = 0;
        for query in QUERIES {
            hits += index.search(query, 20).unwrap().len();
        }
        let started = Instant::now();
        for query in QUERIES {
            hits += index.search(query, 20).unwrap().len();
        }
        let query_us = started.elapsed().as_secs_f64() * 1_000_000.0 / QUERIES.len() as f64;

        println!("{pages},{transient_ms:.1},{persisted_ms:.1},{query_us:.0},{hits}");
    }
}
