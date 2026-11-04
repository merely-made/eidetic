// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;
use crate::browsing::{PageRef, TraceTransition};

fn visit(url: &str, title: Option<&str>, at_ms: u64) -> TraceEvent {
    TraceEvent {
        from: None,
        to: PageRef {
            url: url.to_string(),
            title: title.map(str::to_string),
        },
        transition: TraceTransition::LinkClick,
        at_ms,
        dwell_ms: None,
        candidates: Vec::new(),
    }
}

fn corpus(events: Vec<TraceEvent>) -> Vec<BrowsingTrace> {
    vec![BrowsingTrace::from_events("p", events)]
}

/// A page body of realistic length — near-duplicate thresholds are only
/// meaningful against one. A 64-bit simhash over a paragraph is noisy;
/// over a page it is stable (see the constants' own note).
const ARTICLE: &str = "The page table is a projection over the trace log rather than a          second authority. Every traversal the browser records lands in the log exactly          once, and the table is folded from it whenever a reader needs one. Because the          fold is pure and clockless, the same corpus and the same reference time always          produce the same table, which is what makes it safe to throw away and mint again.          Identity is the interesting part. A URL is an address, not a page: tracking          parameters, mirrors, protocol upgrades and trailing slashes all give the same          content several names, and a reader who keys by address sees one page five times.          Keying by content instead means hashing the extracted main text, which gives exact          identity for free, and hashing its word shingles, which gives a near signature that          survives a small edit. The two together answer both questions a recall surface          asks: have I seen exactly this, and have I seen something very like it. The          threshold between them is a tuning decision and belongs in configuration, because          the right answer depends on how long the pages are and how tolerant the surface          wants to be about boilerplate.";

/// An unrelated page of the same length — the negative control.
const OTHER_ARTICLE: &str = "Analytic ephemerides are judged against numerical          integration, not against each other. A series expansion evaluates in microseconds          and carries no state, which is why a small program can carry one; the integration          it approximates needs a kernel of many megabytes and a reader for it. The          comparison that matters is angular: where does the analytic position of a body fall          relative to the integrated one, over what span of centuries, and at what error. For          the inner planets the agreement holds to well under a milliarcsecond across the          modern era, which is far finer than any use a calendar or a sky chart makes of it.          The outer bodies drift further and faster, and the historical fits diverge outside          the interval they were fitted over. None of this is visible from a single instant          of agreement, which is the trap: two ephemerides that match today may disagree by          an arcminute a thousand years out, so the parity claim has to name its span before          it means anything at all.";

/// Tracking parameters and a fragment are not identity: one page.
#[test]
fn tracking_params_and_fragment_collapse_to_one_record() {
    let traces = corpus(vec![
        visit(
            "https://Example.COM:443/notes/?utm_source=news&utm_medium=email&id=7#section-2",
            Some("Notes"),
            10,
        ),
        visit("https://example.com/notes/?id=7&fbclid=abc123", None, 20),
    ]);
    let table = page_table(&traces, |_| None);
    assert_eq!(table.len(), 1, "one page, two addresses");
    let record = table.records.values().next().unwrap();
    assert_eq!(record.urls.len(), 1, "both canonicalize to one key");
    assert_eq!(
        record.urls.iter().next().unwrap(),
        "https://example.com/notes/?id=7"
    );
    assert_eq!(record.visits, 2);
    assert_eq!(record.title.as_deref(), Some("Notes"));
    assert_eq!(
        record.last_url, "https://example.com/notes/?id=7&fbclid=abc123",
        "a hit opens the address actually visited"
    );
}

/// Same text, two unrelated addresses: one record, both URLs.
#[test]
fn identical_text_under_different_urls_collapses() {
    let traces = corpus(vec![
        visit("https://origin.example/post", None, 10),
        visit("https://mirror.example/2026/post.html", None, 20),
    ]);
    let table = page_table(&traces, |_| Some(ARTICLE.to_string()));
    assert_eq!(table.len(), 1);
    let record = table.records.values().next().unwrap();
    assert_eq!(record.fingerprint.source, FingerprintSource::Text);
    assert_eq!(record.urls.len(), 2, "both addresses reached one page");
    assert_eq!(record.visits, 2);
    assert_eq!(record.first_seen_ms, 10);
    assert_eq!(record.last_seen_ms, 20);
}

/// A few words changed is the same page at the default threshold and a
/// different one under exact identity.
#[test]
fn near_duplicate_text_collapses_at_the_default_threshold() {
    let edited = ARTICLE
        .replace("threshold between them", "threshold among them")
        .replace("safe to throw away", "cheap to throw away")
        .replace("the interesting part", "the delicate part");
    assert_ne!(edited, ARTICLE);
    let traces = corpus(vec![
        visit("https://a.example/post", None, 10),
        visit("https://b.example/post", None, 20),
    ]);
    let text_for = |url: &str| {
        Some(if url.starts_with("https://a.") {
            ARTICLE.to_string()
        } else {
            edited.clone()
        })
    };

    let default = page_table(&traces, text_for);
    assert_eq!(default.len(), 1, "an edit is not a new page");
    assert_eq!(default.records.values().next().unwrap().visits, 2);
    assert_eq!(
        default.by_address.len(),
        2,
        "both addresses point at the surviving record"
    );
    for page in default.by_address.values() {
        assert!(
            default.records.contains_key(page),
            "the memo follows the merge"
        );
    }

    let strict = page_table_with(&traces, text_for, &PageTableConfig::EXACT);
    assert_eq!(strict.len(), 2, "exact identity keeps the edit apart");
}

/// Unrelated bodies stay unrelated: the near hash must not be a merge-all.
#[test]
fn unrelated_text_stays_apart() {
    let traces = corpus(vec![
        visit("https://a.example/", None, 10),
        visit("https://b.example/", None, 20),
    ]);
    let table = page_table(&traces, |url| {
        Some(if url.starts_with("https://a.") {
            ARTICLE.to_string()
        } else {
            OTHER_ARTICLE.to_string()
        })
    });
    assert_eq!(table.len(), 2);
    let fingerprints: Vec<_> = table.records.keys().collect();
    assert!(
        fingerprints[0].hamming(fingerprints[1]) > 4 * PageTableConfig::default().near_hamming,
        "unrelated pages sit far outside the threshold, not just outside it"
    );
}

/// The fallback is a visible fact about the record, not a silent one.
#[test]
fn url_fallback_source_is_visible() {
    let traces = corpus(vec![
        visit("https://known.example/", None, 10),
        visit("https://unknown.example/", None, 20),
    ]);
    let table = page_table(&traces, |url| {
        url.contains("//known.").then(|| ARTICLE.to_string())
    });
    let sources: BTreeSet<FingerprintSource> = table
        .records
        .values()
        .map(|record| record.fingerprint.source)
        .collect();
    assert_eq!(
        sources,
        BTreeSet::from([FingerprintSource::Text, FingerprintSource::CanonicalUrl]),
        "a text-keyed page and a URL-keyed one are distinguishable"
    );
    for record in table.records.values() {
        assert_eq!(
            record.text.is_some(),
            record.fingerprint.source == FingerprintSource::Text,
            "the text slot and the source agree"
        );
    }
}

/// Frecency over the page table sums the visits the URL key split.
#[test]
fn frecency_by_fingerprint_sums_the_collapsed_urls() {
    let traces = corpus(vec![
        visit("https://split.example/x?utm_source=a", None, 100),
        visit("https://split.example/x?utm_source=b", None, 200),
        visit("https://split.example/x#top", None, 300),
        visit("https://other.example/", None, 400),
    ]);
    let config = FrecencyConfig {
        half_life_ms: 0,
        ..FrecencyConfig::default()
    };
    let by_url = crate::browsing::frecency::frecency(&traces, 1_000, &config);
    assert_eq!(by_url.len(), 4, "the URL key splits one page four ways");

    let table = page_table(&traces, |_| None);
    assert_eq!(table.len(), 2);
    let scores = frecency_by_page(&traces, 1_000, &config, &table);
    assert_eq!(scores.len(), 2);
    let split = table
        .records
        .values()
        .find(|record| record.urls.iter().any(|url| url.contains("split")))
        .unwrap();
    assert_eq!(split.visits, 3);
    assert!(
        (scores[&split.fingerprint] - 3.0 * config.weights.link_click).abs() < 1e-9,
        "three visits to one page, one score"
    );
    // And the lookup the caller uses to get there: keyed by the address
    // as visited, so the fold canonicalizes nothing.
    assert_eq!(
        table.by_address.get("https://split.example/x#top").copied(),
        Some(split.fingerprint)
    );
}
