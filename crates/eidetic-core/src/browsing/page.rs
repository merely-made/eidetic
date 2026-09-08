// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Page identity by content — the projection traces are read through.
//!
//! Traces stay an event log. A **page table** is a fold over them keyed by a
//! [`PageFingerprint`]: blake3 over the normalized main text for exact
//! identity, plus a 64-bit simhash over word shingles for near-duplicates.
//! The fingerprint is both the dedup key and the index key, so one visited
//! page is one record and N visit events (wiring plan W6d).
//!
//! Text comes from the host through the `text_for` supplier
//! ([`text`](super::text)'s store, W6c); an address with none falls back to the [`canonical_url`]. The
//! fallback is named in the type ([`PageFingerprint::source`]), never silent:
//! a URL-keyed identity is a weaker claim than a content-keyed one and the
//! caller can see which it holds.
//!
//! Everything the fold decides comes from [`PageTableConfig`]; nothing here
//! reads a clock or a store.

use std::collections::{BTreeMap, BTreeSet};

use super::frecency::{FrecencyConfig, frecency_by};
use super::{BrowsingTrace, TraceEvent};

/// Query keys dropped as campaign noise: these name the click, not the page.
const TRACKING_PARAMS: [&str; 9] = [
    "fbclid", "gclid", "dclid", "gbraid", "wbraid", "msclkid", "mc_cid", "mc_eid", "igshid",
];

/// Whole families dropped by prefix (Urchin's `utm_source`, `utm_medium`, …).
const TRACKING_PARAM_PREFIXES: [&str; 1] = ["utm_"];

/// The port a scheme already implies, and so need not carry.
fn default_port(scheme: &str) -> Option<&'static str> {
    match scheme {
        "http" | "ws" => Some("80"),
        "https" | "wss" => Some("443"),
        _ => None,
    }
}

/// Collapse a URL to the page it names: lowercase scheme and host, no
/// fragment, no default port, no trailing slash on an empty path, no tracking
/// parameters. Everything else is kept verbatim — percent-encoding, case in
/// the path and in surviving query values, and parameter order all carry
/// meaning on real sites.
///
/// A string with no `scheme://` (a `data:` or `about:` form, a bare path)
/// keeps its shape; only the fragment comes off. There is no URL crate here
/// on purpose: the WHATWG parser belongs to the engine, and this key must be
/// computable in the storage layer.
pub fn canonical_url(raw: &str) -> String {
    let raw = raw.trim();
    let without_fragment = raw.split_once('#').map_or(raw, |(head, _)| head);
    let Some((scheme, rest)) = without_fragment.split_once("://") else {
        return without_fragment.to_string();
    };
    let scheme = scheme.to_lowercase();
    let (authority, tail) = rest.split_at(rest.find(['/', '?']).unwrap_or(rest.len()));
    let (path, query) = tail.split_once('?').map_or((tail, ""), |(p, q)| (p, q));
    let authority = canonical_authority(authority, &scheme);
    let path = if path == "/" { "" } else { path };
    let query = canonical_query(query);
    let mut canonical = format!("{scheme}://{authority}{path}");
    if !query.is_empty() {
        canonical.push('?');
        canonical.push_str(&query);
    }
    canonical
}

fn canonical_authority(authority: &str, scheme: &str) -> String {
    let (userinfo, host_port) = match authority.rsplit_once('@') {
        Some((user, host)) => (Some(user), host),
        None => (None, authority),
    };
    // An IPv6 literal keeps its brackets; its port is whatever follows them.
    let (host, port) = match host_port.rfind(']') {
        Some(end) => {
            let (host, rest) = host_port.split_at(end + 1);
            (host, rest.strip_prefix(':'))
        },
        None => match host_port.rsplit_once(':') {
            Some((host, port)) => (host, Some(port)),
            None => (host_port, None),
        },
    };
    let mut canonical = String::new();
    if let Some(userinfo) = userinfo {
        canonical.push_str(userinfo);
        canonical.push('@');
    }
    canonical.push_str(&host.to_lowercase());
    if let Some(port) = port.filter(|p| !p.is_empty() && Some(*p) != default_port(scheme)) {
        canonical.push(':');
        canonical.push_str(port);
    }
    canonical
}

fn canonical_query(query: &str) -> String {
    query
        .split('&')
        .filter(|pair| !pair.is_empty())
        .filter(|pair| {
            let key = pair
                .split_once('=')
                .map_or(*pair, |(key, _)| key)
                .to_lowercase();
            !TRACKING_PARAMS.contains(&key.as_str())
                && !TRACKING_PARAM_PREFIXES
                    .iter()
                    .any(|prefix| key.starts_with(prefix))
        })
        .collect::<Vec<_>>()
        .join("&")
}

/// Whitespace-collapsed page text — the exact hash's input.
///
/// No Unicode normalization: eidetic carries no unicode dependency, and
/// adding one for NFC is its own decision. Text arriving from one extractor
/// is already consistently composed, so the gap shows only across sources.
pub fn normalize_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn hash64(bytes: &[u8]) -> u64 {
    let digest = blake3::hash(bytes);
    u64::from_le_bytes(
        digest.as_bytes()[..8]
            .try_into()
            .expect("blake3 is 32 bytes"),
    )
}

/// One hash per `shingle`-word window, lowercased. Text shorter than the
/// window hashes whole, so a one-line page still gets a signature.
fn shingle_hashes(text: &str, shingle: usize) -> Vec<u64> {
    let words: Vec<String> = text.split_whitespace().map(str::to_lowercase).collect();
    if words.is_empty() {
        return Vec::new();
    }
    let shingle = shingle.max(1);
    if words.len() <= shingle {
        return vec![hash64(words.join(" ").as_bytes())];
    }
    words
        .windows(shingle)
        .map(|window| hash64(window.join(" ").as_bytes()))
        .collect()
}

/// Charikar simhash: each bit votes across the shingle hashes, and the sign
/// of the vote is the bit. Documents differing in a few shingles differ in a
/// few bits, which is the whole point — unlike blake3, which differs in half.
pub fn simhash(text: &str, shingle: usize) -> u64 {
    let mut votes = [0i64; 64];
    for hash in shingle_hashes(text, shingle) {
        for (bit, vote) in votes.iter_mut().enumerate() {
            *vote += if (hash >> bit) & 1 == 1 { 1 } else { -1 };
        }
    }
    votes
        .iter()
        .enumerate()
        .filter(|(_, vote)| **vote > 0)
        .fold(0u64, |bits, (bit, _)| bits | (1 << bit))
}

/// Which claim a fingerprint makes. A [`Text`](Self::Text) fingerprint says
/// "this content"; a [`CanonicalUrl`](Self::CanonicalUrl) one says only "this
/// address", and two addresses for one page stay two records.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FingerprintSource {
    /// Hashed from extracted main text.
    Text,
    /// Fallback: hashed from [`canonical_url`] because no text was available.
    CanonicalUrl,
}

/// A page's content identity: exact and near, with the source of both named.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PageFingerprint {
    pub source: FingerprintSource,
    /// blake3 of the normalized input — equality is exact identity.
    pub exact: [u8; 32],
    /// Simhash of the same input's word shingles; see [`PageFingerprint::near`].
    pub near: u64,
}

impl PageFingerprint {
    /// Fingerprint extracted page text. Normalization is done here so no
    /// caller can hash a differently-whitespaced copy of the same page.
    pub fn of_text(text: &str, shingle: usize) -> Self {
        let normalized = normalize_text(text);
        Self {
            source: FingerprintSource::Text,
            exact: *blake3::hash(normalized.as_bytes()).as_bytes(),
            near: simhash(&normalized, shingle),
        }
    }

    /// Fingerprint an address. Pass a [`canonical_url`] — this does not
    /// canonicalize, so the caller's key and the table's agree.
    pub fn of_url(canonical: &str) -> Self {
        Self {
            source: FingerprintSource::CanonicalUrl,
            exact: *blake3::hash(canonical.as_bytes()).as_bytes(),
            near: simhash(canonical, 1),
        }
    }

    /// Bits by which the two near-hashes disagree; 0 is identical content.
    pub fn hamming(&self, other: &Self) -> u32 {
        (self.near ^ other.near).count_ones()
    }

    /// Do these fingerprints name the same page within `max_hamming` bits?
    /// Sources must match: a text signature and an address signature are not
    /// comparable, and unrelated 64-bit simhashes sit near 32 bits apart.
    pub fn near(&self, other: &Self, max_hamming: u32) -> bool {
        self.source == other.source
            && (self.exact == other.exact || self.hamming(other) <= max_hamming)
    }

    pub fn hex(&self) -> String {
        blake3::Hash::from(self.exact).to_hex().to_string()
    }
}

/// One page as the table sees it: an identity, every address that reached it,
/// and the visit summary folded from the events.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PageRecord {
    pub fingerprint: PageFingerprint,
    /// Every [`canonical_url`] that resolved to this page.
    pub urls: BTreeSet<String>,
    /// The address of the most recent visit, verbatim — what a hit opens.
    pub last_url: String,
    /// The title of the most recent visit that carried one.
    pub title: Option<String>,
    pub first_seen_ms: u64,
    pub last_seen_ms: u64,
    pub visits: u64,
    /// Normalized main text, when the supplier had it. W6c fills this.
    pub text: Option<String>,
}

/// The projection's policy. The near threshold is here rather than inline in
/// the fold because it is a tuning decision, not an invariant: 3 bits of 64
/// collapses a page whose boilerplate or a few words moved, and leaves
/// unrelated pages (~32 bits apart) alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PageTableConfig {
    /// Words per shingle for the near hash.
    pub shingle_words: usize,
    /// Bits two near-hashes may differ by and still be one page. Zero turns
    /// near-duplicate collapsing off, leaving exact identity only.
    pub near_hamming: u32,
}

impl PageTableConfig {
    /// Exact identity only — near-duplicates stay separate records.
    pub const EXACT: Self = Self {
        shingle_words: 3,
        near_hamming: 0,
    };
}

impl Default for PageTableConfig {
    fn default() -> Self {
        Self {
            shingle_words: 3,
            near_hamming: 3,
        }
    }
}

/// The projection: one record per page, and the address memo that reached it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PageTable {
    /// One record per page, keyed by its fingerprint.
    pub records: BTreeMap<PageFingerprint, PageRecord>,
    /// Every raw address the corpus carried, mapped to the page it resolved
    /// to. Keyed by the address **as visited**, so a second fold over the same
    /// events (frecency, an index corpus) canonicalizes nothing again — the
    /// fold already paid for that, once per distinct address.
    pub by_address: BTreeMap<String, PageFingerprint>,
}

impl PageTable {
    /// The page an event's address resolved to. An address this table never
    /// saw scores under its own fingerprint rather than being dropped: a
    /// missing key would read as "never visited".
    pub fn page_of(&self, raw_url: &str) -> PageFingerprint {
        self.by_address
            .get(raw_url)
            .copied()
            .unwrap_or_else(|| PageFingerprint::of_url(&canonical_url(raw_url)))
    }

    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
}

/// The page table over a trace corpus, at the default policy.
///
/// `text_for(url)` supplies extracted main text for a visited URL — the same
/// shape `TrailIndex::rebuild_with_text` takes — and is called once per
/// distinct address. `browsing::text::PageTexts::lookup` is the supplier the
/// host wires in (W6c); `|_| None` yields a table of URL-sourced fingerprints.
pub fn page_table(
    traces: &[BrowsingTrace],
    text_for: impl Fn(&str) -> Option<String>,
) -> PageTable {
    page_table_with(traces, text_for, &PageTableConfig::default())
}

/// [`page_table`] under a caller's policy.
pub fn page_table_with(
    traces: &[BrowsingTrace],
    text_for: impl Fn(&str) -> Option<String>,
    config: &PageTableConfig,
) -> PageTable {
    // Canonicalizing and fingerprinting are per *address*, not per visit: a
    // corpus is mostly revisits, and hashing a page body again per visit is
    // the fold's whole cost.
    let mut seen: BTreeMap<String, (String, Option<String>, PageFingerprint)> = BTreeMap::new();
    let mut records: BTreeMap<PageFingerprint, PageRecord> = BTreeMap::new();
    // When each record's title was taken, so a merge keeps the newer one.
    let mut title_at: BTreeMap<PageFingerprint, u64> = BTreeMap::new();

    for event in traces.iter().flat_map(|trace| trace.events.iter()) {
        let raw = event.to.url.as_str();
        let (canonical, text, fingerprint) = seen.entry(raw.to_string()).or_insert_with(|| {
            let canonical = canonical_url(raw);
            let text = text_for(raw)
                .map(|text| normalize_text(&text))
                .filter(|text| !text.is_empty());
            let fingerprint = match &text {
                Some(text) => PageFingerprint::of_text(text, config.shingle_words),
                None => PageFingerprint::of_url(&canonical),
            };
            (canonical, text, fingerprint)
        });
        let fingerprint = *fingerprint;

        let record = records.entry(fingerprint).or_insert_with(|| PageRecord {
            fingerprint,
            urls: BTreeSet::new(),
            last_url: String::new(),
            title: None,
            first_seen_ms: event.at_ms,
            last_seen_ms: event.at_ms,
            visits: 0,
            text: text.clone(),
        });
        record.urls.insert(canonical.clone());
        record.visits += 1;
        record.first_seen_ms = record.first_seen_ms.min(event.at_ms);
        if event.at_ms >= record.last_seen_ms || record.last_url.is_empty() {
            record.last_seen_ms = record.last_seen_ms.max(event.at_ms);
            record.last_url = raw.to_string();
        }
        if let Some(title) = event
            .to
            .title
            .as_deref()
            .map(str::trim)
            .filter(|t| !t.is_empty())
        {
            let stamp = title_at.get(&fingerprint).copied();
            if record.title.is_none() || stamp.is_none_or(|at| event.at_ms >= at) {
                record.title = Some(title.to_string());
                title_at.insert(fingerprint, event.at_ms);
            }
        }
    }

    let (records, merged) = collapse_near(records, &title_at, config);
    let by_address = seen
        .into_iter()
        .map(|(raw, (_, _, fingerprint))| {
            let page = merged.get(&fingerprint).copied().unwrap_or(fingerprint);
            (raw, page)
        })
        .collect();
    PageTable {
        records,
        by_address,
    }
}

/// Merge records whose near-hashes agree within the threshold, oldest first,
/// so the surviving key is the fingerprint of the page as first seen. Returns
/// the surviving records and, for each fingerprint that was merged away, the
/// one it merged into — the remap the address memo follows.
///
/// Quadratic in the number of records that pass the source gate. Exact
/// matches never reach here (they share a key), so this walks only genuine
/// near-duplicate candidates; a corpus large enough to feel it wants a
/// banded index, which is a change to this function alone.
#[allow(clippy::type_complexity)]
fn collapse_near(
    records: BTreeMap<PageFingerprint, PageRecord>,
    title_at: &BTreeMap<PageFingerprint, u64>,
    config: &PageTableConfig,
) -> (
    BTreeMap<PageFingerprint, PageRecord>,
    BTreeMap<PageFingerprint, PageFingerprint>,
) {
    let mut ordered: Vec<PageRecord> = records.into_values().collect();
    ordered.sort_by(|left, right| {
        left.first_seen_ms
            .cmp(&right.first_seen_ms)
            .then_with(|| left.fingerprint.cmp(&right.fingerprint))
    });

    let mut kept: Vec<PageRecord> = Vec::new();
    let mut kept_title_at: Vec<u64> = Vec::new();
    let mut merged: BTreeMap<PageFingerprint, PageFingerprint> = BTreeMap::new();
    for record in ordered {
        let stamp = title_at.get(&record.fingerprint).copied().unwrap_or(0);
        // Only content signatures collapse. Two canonical URLs a few
        // characters apart are different pages, not a near-duplicate.
        let mergeable =
            record.fingerprint.source == FingerprintSource::Text && config.near_hamming > 0;
        let target = mergeable
            .then(|| {
                kept.iter()
                    .position(|k| k.fingerprint.near(&record.fingerprint, config.near_hamming))
            })
            .flatten();
        match target {
            Some(index) => {
                merged.insert(record.fingerprint, kept[index].fingerprint);
                let target = &mut kept[index];
                target.visits += record.visits;
                target.first_seen_ms = target.first_seen_ms.min(record.first_seen_ms);
                let newer_title = record.title.is_some()
                    && (target.title.is_none() || stamp >= kept_title_at[index]);
                if record.last_seen_ms > target.last_seen_ms {
                    target.last_seen_ms = record.last_seen_ms;
                    target.last_url = record.last_url;
                }
                if target.text.is_none() {
                    target.text = record.text;
                }
                if newer_title {
                    target.title = record.title;
                    kept_title_at[index] = stamp;
                }
                target.urls.extend(record.urls);
            },
            None => {
                kept.push(record);
                kept_title_at.push(stamp);
            },
        }
    }
    let records = kept
        .into_iter()
        .map(|record| (record.fingerprint, record))
        .collect();
    (records, merged)
}

/// Frecency keyed by page rather than by address: every visit to any URL that
/// collapsed into a record sums into that record's score.
///
/// Takes the table the projection already built, so the fold reads
/// [`PageTable::page_of`] per event and canonicalizes nothing.
pub fn frecency_by_page(
    traces: &[BrowsingTrace],
    now_ms: u64,
    config: &FrecencyConfig,
    table: &PageTable,
) -> BTreeMap<PageFingerprint, f64> {
    frecency_by(traces, now_ms, config, |event: &TraceEvent| {
        table.page_of(&event.to.url)
    })
}

#[cfg(test)]
mod tests {
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
        // The rules, one at a time.
        assert_eq!(
            canonical_url("HTTP://Example.com:80/"),
            "http://example.com"
        );
        assert_eq!(
            canonical_url("https://example.com/a/"),
            "https://example.com/a/",
            "only an empty path loses its slash"
        );
        assert_eq!(
            canonical_url("https://example.com/a?Q=Keep&gclid=x"),
            "https://example.com/a?Q=Keep",
            "case and order of surviving parameters are kept"
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
}
