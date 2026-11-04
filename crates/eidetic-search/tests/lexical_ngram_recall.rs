// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Fixed-corpus receipt for ESP token n-gram lexical recall and the existing
//! Eidetic BM25/vector fusion seam.
//!
//! The target records and held-out queries model five application surfaces:
//! browsing actions, page titles, URLs, named entities, and commands. Each
//! phrase-sensitive target competes with a shorter record containing the same
//! unigrams in a different order. Five controls cover ordinary unigram or
//! order-insensitive recall. Nothing is trained; every configuration sees the
//! same documents and queries through the existing `SemanticSearch` and dense
//! `VectorIndex` derived-index boundary. A second receipt projects those same
//! records into a real `TrailIndex`, then compares BM25, each vector ranking,
//! and reciprocal-rank fusion. The cross-crate dependency is test-only: the
//! caller continues to bring both rankings to Eidetic's engine-agnostic seam.

#[cfg(feature = "bert")]
use std::fmt::Write as _;
use std::hint::black_box;
#[cfg(feature = "bert")]
use std::io::Read;
use std::mem::size_of;
#[cfg(feature = "bert")]
use std::path::{Path, PathBuf};
use std::time::Instant;

use eidetic::browsing::{BrowsingTrace, PageRef, TraceEvent, TraceTransition};
use eidetic_search::{TrailIndex, fuse};
use esp::embed::{EmbeddingProvider, LexicalEmbeddingProvider, SemanticSearch};
#[cfg(feature = "bert")]
use sha2::{Digest, Sha256};

const DIMENSIONS: usize = 4096;

const DOCUMENTS: &[(&str, &str)] = &[
    ("browse-storage", "Browser Storage Quota settings"),
    (
        "browse-storage-decoy",
        "Quota browser report: storage totals by site",
    ),
    ("browse-tab", "Restore Closed Tab history"),
    ("browse-tab-decoy", "Tab restore: closed"),
    ("title-local-first", "Local First Software essay"),
    ("title-local-first-decoy", "Software first: local"),
    ("title-device-policy", "Shared Device Policy notes"),
    ("title-device-policy-decoy", "Device policy: shared"),
    (
        "url-rust-async",
        "https://example.test/rust/async/book/getting-started",
    ),
    ("url-rust-async-decoy", "https://rust.book/async"),
    (
        "url-graph-query",
        "https://api.example.test/graph/query/nodes/by-label",
    ),
    ("url-graph-query-decoy", "https://graph.nodes/query"),
    ("entity-nyt", "New York Times profile"),
    ("entity-nyt-decoy", "Times: New York"),
    ("entity-wapo", "Washington Post Company profile"),
    ("entity-wapo-decoy", "Company post: Washington"),
    ("command-downloads", "Open Downloads Folder command"),
    ("command-downloads-decoy", "Folder Downloads: Open"),
    ("command-tabs", "Close Other Tabs command"),
    ("command-tabs-decoy", "Tabs menu: other close actions"),
    ("control-wgpu", "WGPU render pipeline diagnostics"),
    (
        "control-offline-sync",
        "Offline browsing sync across owned devices",
    ),
    (
        "control-entity-aliases",
        "Entity aliases and provenance records",
    ),
    ("control-pin-page", "Pin Current Page to workspace"),
    ("control-reader", "Reader mode typography settings"),
    (
        "background-radio",
        "Reticulum packet radio link budget notes",
    ),
    ("background-music", "Fingerstyle guitar practice journal"),
    ("background-cooking", "Weeknight lentil soup recipe"),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Surface {
    Browsing,
    Title,
    Url,
    Entity,
    Command,
    Control,
}

#[derive(Clone, Copy)]
struct EvalCase {
    surface: Surface,
    query: &'static str,
    expected: &'static str,
}

const EVAL_CASES: &[EvalCase] = &[
    EvalCase {
        surface: Surface::Browsing,
        query: "browser storage quota",
        expected: "browse-storage",
    },
    EvalCase {
        surface: Surface::Browsing,
        query: "restore closed tab",
        expected: "browse-tab",
    },
    EvalCase {
        surface: Surface::Title,
        query: "local first software",
        expected: "title-local-first",
    },
    EvalCase {
        surface: Surface::Title,
        query: "shared device policy",
        expected: "title-device-policy",
    },
    EvalCase {
        surface: Surface::Url,
        query: "rust async book",
        expected: "url-rust-async",
    },
    EvalCase {
        surface: Surface::Url,
        query: "graph query nodes",
        expected: "url-graph-query",
    },
    EvalCase {
        surface: Surface::Entity,
        query: "new york times",
        expected: "entity-nyt",
    },
    EvalCase {
        surface: Surface::Entity,
        query: "washington post company",
        expected: "entity-wapo",
    },
    EvalCase {
        surface: Surface::Command,
        query: "open downloads folder",
        expected: "command-downloads",
    },
    EvalCase {
        surface: Surface::Command,
        query: "close other tabs",
        expected: "command-tabs",
    },
    EvalCase {
        surface: Surface::Control,
        query: "wgpu",
        expected: "control-wgpu",
    },
    EvalCase {
        surface: Surface::Control,
        query: "offline sync",
        expected: "control-offline-sync",
    },
    EvalCase {
        surface: Surface::Control,
        query: "provenance aliases",
        expected: "control-entity-aliases",
    },
    EvalCase {
        surface: Surface::Control,
        query: "pin page",
        expected: "control-pin-page",
    },
    EvalCase {
        surface: Surface::Control,
        query: "typography settings",
        expected: "control-reader",
    },
];

type Search = SemanticSearch<&'static str, LexicalEmbeddingProvider>;
#[cfg(feature = "bert")]
type DenseSearch = SemanticSearch<&'static str, Box<dyn EmbeddingProvider>>;
type RankedCases = Vec<(Surface, &'static str, usize)>;
type OptionalRankedCases = Vec<(Surface, &'static str, Option<usize>)>;

fn build_search(orders: &[usize]) -> Search {
    let provider = LexicalEmbeddingProvider::with_token_ngram_orders(DIMENSIONS, orders).unwrap();
    let mut search = SemanticSearch::new(provider);
    search.ingest_batch(DOCUMENTS).unwrap();
    search
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct RankingTally {
    at_1: usize,
    at_3: usize,
    phrase_at_1: usize,
    controls_at_1: usize,
}

fn evaluate(search: &Search) -> (RankingTally, RankedCases) {
    let mut tally = RankingTally::default();
    let mut ranks = Vec::with_capacity(EVAL_CASES.len());
    for case in EVAL_CASES {
        let ranking = search.search(case.query, DOCUMENTS.len()).unwrap();
        let rank = ranking
            .iter()
            .position(|(key, _)| key == &case.expected)
            .map(|zero_based| zero_based + 1)
            .expect("every expected record is indexed");
        tally.at_1 += usize::from(rank <= 1);
        tally.at_3 += usize::from(rank <= 3);
        if case.surface == Surface::Control {
            tally.controls_at_1 += usize::from(rank <= 1);
        } else {
            tally.phrase_at_1 += usize::from(rank <= 1);
        }
        ranks.push((case.surface, case.query, rank));
    }
    (tally, ranks)
}

fn record_url(id: &str) -> String {
    let (_, text) = DOCUMENTS
        .iter()
        .find(|(document_id, _)| *document_id == id)
        .expect("record id belongs to the fixed corpus");
    if id.starts_with("url-") {
        (*text).to_string()
    } else {
        format!("https://fixture.test/{id}")
    }
}

fn browsing_trace() -> BrowsingTrace {
    let events = DOCUMENTS
        .iter()
        .enumerate()
        .map(|(index, (id, text))| TraceEvent {
            from: None,
            to: PageRef {
                url: record_url(id),
                title: (!id.starts_with("url-")).then(|| (*text).to_string()),
            },
            transition: TraceTransition::Imported,
            at_ms: index as u64 + 1,
            dwell_ms: None,
            candidates: Vec::new(),
        })
        .collect();
    BrowsingTrace::from_events("held-out", events)
}

fn lexical_ranking(trail: &TrailIndex, query: &str) -> Vec<String> {
    trail
        .search(query, DOCUMENTS.len())
        .unwrap()
        .into_iter()
        .map(|hit| hit.url)
        .collect()
}

fn vector_ranking<P: EmbeddingProvider>(
    search: &SemanticSearch<&'static str, P>,
    query: &str,
) -> Vec<String> {
    search
        .search(query, DOCUMENTS.len())
        .unwrap()
        .into_iter()
        .map(|(id, _)| record_url(id))
        .collect()
}

fn evaluate_ranker(
    ranker: impl Fn(&EvalCase) -> Vec<String>,
) -> (RankingTally, OptionalRankedCases) {
    let mut tally = RankingTally::default();
    let mut ranks = Vec::with_capacity(EVAL_CASES.len());
    for case in EVAL_CASES {
        let ranking = ranker(case);
        let expected = record_url(case.expected);
        let rank = ranking
            .iter()
            .position(|candidate| candidate == &expected)
            .map(|zero_based| zero_based + 1);
        tally.at_1 += usize::from(rank.is_some_and(|rank| rank <= 1));
        tally.at_3 += usize::from(rank.is_some_and(|rank| rank <= 3));
        if case.surface == Surface::Control {
            tally.controls_at_1 += usize::from(rank.is_some_and(|rank| rank <= 1));
        } else {
            tally.phrase_at_1 += usize::from(rank.is_some_and(|rank| rank <= 1));
        }
        ranks.push((case.surface, case.query, rank));
    }
    (tally, ranks)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct FusionTally {
    deterministic: RankingTally,
    unique_at_1: usize,
    phrase_unique_at_1: usize,
    controls_unique_at_1: usize,
    expected_top_ties: usize,
}

fn evaluate_fusion<P: EmbeddingProvider>(
    trail: &TrailIndex,
    vector: &SemanticSearch<&'static str, P>,
    weights: (f64, f64),
) -> (FusionTally, RankedCases) {
    let mut tally = FusionTally::default();
    let mut ranks = Vec::with_capacity(EVAL_CASES.len());
    for case in EVAL_CASES {
        let fused = fuse(
            &lexical_ranking(trail, case.query),
            &vector_ranking(vector, case.query),
            60.0,
            weights,
        );
        let expected = record_url(case.expected);
        let rank = fused
            .iter()
            .position(|hit| hit.url == expected)
            .map(|zero_based| zero_based + 1)
            .expect("every expected record is returned by fusion");
        tally.deterministic.at_1 += usize::from(rank <= 1);
        tally.deterministic.at_3 += usize::from(rank <= 3);
        if case.surface == Surface::Control {
            tally.deterministic.controls_at_1 += usize::from(rank <= 1);
        } else {
            tally.deterministic.phrase_at_1 += usize::from(rank <= 1);
        }

        let top_score = fused.first().unwrap().score;
        let expected_score = fused.iter().find(|hit| hit.url == expected).unwrap().score;
        let expected_is_top = (expected_score - top_score).abs() < f64::EPSILON;
        let top_count = fused
            .iter()
            .take_while(|hit| (hit.score - top_score).abs() < f64::EPSILON)
            .count();
        let unique_at_1 = expected_is_top && top_count == 1;
        tally.unique_at_1 += usize::from(unique_at_1);
        tally.expected_top_ties += usize::from(expected_is_top && top_count > 1);
        if case.surface == Surface::Control {
            tally.controls_unique_at_1 += usize::from(unique_at_1);
        } else {
            tally.phrase_unique_at_1 += usize::from(unique_at_1);
        }
        ranks.push((case.surface, case.query, rank));
    }
    (tally, ranks)
}

#[test]
fn held_out_phrase_ranking_improves_without_control_regressions() {
    let unigram = build_search(&[1]);
    let bigram = build_search(&[1, 2]);
    let trigram = build_search(&[1, 2, 3]);

    let (unigram_tally, unigram_ranks) = evaluate(&unigram);
    let (bigram_tally, bigram_ranks) = evaluate(&bigram);
    let (trigram_tally, trigram_ranks) = evaluate(&trigram);

    assert_eq!(
        unigram_tally,
        RankingTally {
            at_1: 7,
            at_3: 15,
            phrase_at_1: 2,
            controls_at_1: 5,
        },
        "unigram baseline drifted: {unigram_ranks:?}"
    );
    for (label, tally, ranks) in [
        ("unigram+bigram", bigram_tally, bigram_ranks),
        ("unigram+bigram+trigram", trigram_tally, trigram_ranks),
    ] {
        assert_eq!(
            tally,
            RankingTally {
                at_1: 15,
                at_3: 15,
                phrase_at_1: 10,
                controls_at_1: 5,
            },
            "{label} receipt drifted: {ranks:?}"
        );
    }

    // Trigrams are exercised but earn no ranking win over bigrams on this
    // corpus. That distinction belongs in the receipt rather than a default.
    assert_eq!(bigram_tally, trigram_tally);
}

#[test]
fn held_out_bm25_and_fusion_baselines_are_explicit() {
    let directory = tempfile::tempdir().unwrap();
    let trace = browsing_trace();
    let trail = TrailIndex::rebuild(directory.path().join("trail"), [&trace]).unwrap();
    let unigram = build_search(&[1]);
    let bigram = build_search(&[1, 2]);

    let bm25_baseline = RankingTally {
        at_1: 7,
        at_3: 15,
        phrase_at_1: 2,
        controls_at_1: 5,
    };
    let vector_baseline = RankingTally {
        at_1: 7,
        at_3: 15,
        phrase_at_1: 2,
        controls_at_1: 5,
    };
    let phrase_sensitive = RankingTally {
        at_1: 15,
        at_3: 15,
        phrase_at_1: 10,
        controls_at_1: 5,
    };

    let (bm25, bm25_ranks) = evaluate_ranker(|case| lexical_ranking(&trail, case.query));
    let (unigram_vector, unigram_ranks) =
        evaluate_ranker(|case| vector_ranking(&unigram, case.query));
    let (bigram_vector, bigram_ranks) = evaluate_ranker(|case| vector_ranking(&bigram, case.query));
    assert_eq!(bm25, bm25_baseline, "BM25 baseline drifted: {bm25_ranks:?}");
    assert!(
        bm25_ranks.iter().all(|(_, _, rank)| rank.is_some()),
        "tokenized URL components must keep every expected record recallable: {bm25_ranks:?}"
    );
    assert_eq!(
        unigram_vector, vector_baseline,
        "unigram baseline drifted: {unigram_ranks:?}"
    );
    assert_eq!(
        bigram_vector, phrase_sensitive,
        "1+2 vector receipt drifted: {bigram_ranks:?}"
    );

    let (equal_unigram, equal_unigram_ranks) = evaluate_fusion(&trail, &unigram, (1.0, 1.0));
    assert_eq!(
        equal_unigram,
        FusionTally {
            deterministic: vector_baseline,
            unique_at_1: 7,
            phrase_unique_at_1: 2,
            controls_unique_at_1: 5,
            expected_top_ties: 0,
        },
        "equal-weight BM25 + unigram drifted: {equal_unigram_ranks:?}"
    );

    let (equal_bigram, equal_bigram_ranks) = evaluate_fusion(&trail, &bigram, (1.0, 1.0));
    assert_eq!(
        equal_bigram,
        FusionTally {
            // The deterministic URL tie-break happens to put every expected
            // target first. Eight phrase cases have the same RRF score as the
            // decoy, so only seven results are unique winners.
            deterministic: phrase_sensitive,
            unique_at_1: 7,
            phrase_unique_at_1: 2,
            controls_unique_at_1: 5,
            expected_top_ties: 8,
        },
        "equal-weight BM25 + 1+2 drifted: {equal_bigram_ranks:?}"
    );

    let (vector_heavy_bigram, vector_heavy_ranks) = evaluate_fusion(&trail, &bigram, (1.0, 2.0));
    assert_eq!(
        vector_heavy_bigram,
        FusionTally {
            deterministic: phrase_sensitive,
            unique_at_1: 15,
            phrase_unique_at_1: 10,
            controls_unique_at_1: 5,
            expected_top_ties: 0,
        },
        "vector-heavy BM25 + 1+2 probe drifted: {vector_heavy_ranks:?}"
    );
}

#[path = "lexical_ngram_recall/measurements.rs"]
mod measurements;
