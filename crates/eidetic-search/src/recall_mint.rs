// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The page-keyed recall index minted over a trace corpus, promoted from
//! Turnstone's `trail_memory` (memory-gates plan S6, D17).
//!
//! One disposable projection over the authoritative traces: a lexical BM25
//! lane over title, URL and stored body, and a behavioural frecency lane,
//! both keyed by page rather than by visit, fused by reciprocal rank. It is
//! re-minted when the corpus or the title overlay moves and never repaired.
//!
//! Unlike [`crate::recall::RecallIndex`] (passage retrieval over an
//! owner-authorized retained corpus), this answers the omnibar: which visited
//! page did the person mean. The host passes the clock (`now_ms`), so a mint
//! is a pure function of its inputs and reaches wasm.

use crate::{CandidateIndex, FusedHit, IndexConfig, Ranking, SearchError, TrailIndex, fuse_many};
use eidetic::{
    BrowsingTrace, FrecencyConfig, PageFingerprint, PageRef, PageTable, PageTexts, TraceEvent,
    frecency_by_page, page_table,
};
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

/// Standard reciprocal-rank damping. The ranking-relevant ratio is exposed
/// instead: frecency, relative to BM25.
pub const RRF_K: f64 = 60.0;

/// Pull a wider head from each input before reducing to the row limit.
pub const FUSION_CANDIDATE_MULTIPLIER: usize = 4;

/// Frecency's fusion weight relative to BM25, on by default. Above 1.0 so a
/// typed prefix the person has been to before outranks a page BM25 merely
/// likes the title of — the behavioural lane's whole point (wiring plan W6b).
pub const DEFAULT_FRECENCY_WEIGHT: f32 = 2.0;

/// Live settings consumed by the derived recall index.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RecallConfig {
    frecency_weight: f32,
}

impl Default for RecallConfig {
    fn default() -> Self {
        Self {
            frecency_weight: DEFAULT_FRECENCY_WEIGHT,
        }
    }
}

impl RecallConfig {
    /// Weight of the behavioural ranking relative to BM25, clamped to
    /// `0.0..=4.0` (non-finite reads as zero); zero is the lane's off switch.
    #[must_use]
    pub fn with_frecency_weight(mut self, weight: f32) -> Self {
        self.frecency_weight = Self::clamp_weight(weight);
        self
    }

    fn clamp_weight(weight: f32) -> f32 {
        if weight.is_finite() {
            weight.clamp(0.0, 4.0)
        } else {
            0.0
        }
    }

    /// Whether the behavioural lane contributes.
    #[must_use]
    pub fn frecency_enabled(self) -> bool {
        self.frecency_weight > 0.0
    }

    /// The configured weight.
    #[must_use]
    pub fn frecency_weight(self) -> f32 {
        self.frecency_weight
    }
}

/// One application-owned title available to the derived recall index.
///
/// Graph and recycle-bin state remain authoritative for these labels. They
/// fill titleless cloned pages while an index is minted; the stored traces are
/// never mutated.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct RecallSource {
    /// The page address.
    pub url: String,
    /// The label to show for it.
    pub title: String,
}

impl RecallSource {
    /// Ignore fallback labels that add nothing beyond the canonical URL.
    pub fn new(url: impl Into<String>, title: impl Into<String>) -> Option<Self> {
        let url = url.into();
        let title = title.into().trim().to_string();
        (!title.is_empty() && title != url).then_some(Self { url, title })
    }
}

/// Canonicalize a title projection. A URL may appear in more than one graph
/// member; sorting makes the selected title deterministic rather than
/// dependent on graph iteration order.
#[must_use]
pub fn canonical_sources(mut sources: Vec<RecallSource>) -> Vec<RecallSource> {
    sources.sort_unstable();
    sources.dedup_by(|left, right| left.url == right.url);
    sources
}

/// A hit in app-neutral terms: the address a hit opens, its best title, and
/// when the traversal it indexes happened (unix milliseconds).
#[derive(Clone, Debug, PartialEq)]
pub struct RecallHit {
    /// The address a hit opens (the page's most recent URL).
    pub url: String,
    /// The best known title.
    pub title: Option<String>,
    /// When the indexed traversal happened.
    pub at_ms: u64,
}

/// What a mint saw. Counts only: per-phase timings are the host's, taken with
/// a [`MintObserver`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MintCounts {
    /// Stored trace segments read.
    pub traces: usize,
    /// Traversal events across them.
    pub events: usize,
    /// Distinct pages by content fingerprint — one index document each.
    pub pages: usize,
    /// Pages that carried a stored body into the index.
    pub pages_with_text: usize,
    /// Distinct destination addresses that folded into those pages.
    pub collapsed_urls: usize,
    /// Url plus title bytes summed over every event.
    pub corpus_bytes: usize,
    /// Stored body bytes the index carried.
    pub text_bytes: usize,
    /// Memory held by the candidate index.
    pub candidate_bytes: usize,
}

/// A mint phase, reported to a [`MintObserver`] as it completes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MintPhase {
    /// The page table and its documents.
    PageTable,
    /// The lexical rebuild.
    Lexical,
    /// The frecency fold.
    Frecency,
    /// The candidate index.
    Candidates,
}

/// Host hook for timing a mint without the core reading a clock.
pub trait MintObserver {
    /// `phase` just finished.
    fn phase_done(&mut self, phase: MintPhase);
}

impl MintObserver for () {
    fn phase_done(&mut self, _phase: MintPhase) {}
}

/// The lexical index directory beside a memory store, on the `<db>.index`
/// convention of ESP's `eidetic-recall` example. Derived state: re-minted from the corpus, never
/// repaired.
#[must_use]
pub fn index_dir(memory: &Path) -> PathBuf {
    let mut dir = memory.as_os_str().to_os_string();
    dir.push(".index");
    PathBuf::from(dir)
}

/// Events, indexable bytes and distinct destination addresses across a
/// corpus. Addresses are what the index used to hold one document each of;
/// the page table's record count subtracted from them is what collapsed.
fn corpus_size(traces: &[BrowsingTrace]) -> (usize, usize, usize) {
    let mut events = 0;
    let mut bytes = 0;
    let mut urls = HashSet::new();
    for trace in traces {
        events += trace.events.len();
        for event in &trace.events {
            bytes += event.to.url.len() + event.to.title.as_deref().map_or(0, str::len);
            urls.insert(event.to.url.as_str());
        }
    }
    (events, bytes, urls.len())
}

/// One document per page record, keyed by the address a hit opens.
fn recall_documents(table: &PageTable) -> BTreeMap<String, RecallHit> {
    table
        .records
        .values()
        .map(|record| {
            (
                record.last_url.clone(),
                RecallHit {
                    url: record.last_url.clone(),
                    title: record.title.clone(),
                    at_ms: record.last_seen_ms,
                },
            )
        })
        .collect()
}

/// The page table as a corpus the lexical index can mint from: the most
/// recent event that reached each page, destination rewritten to the record's
/// address and best title. One document per page rather than one per visit
/// (the 3x duplication W6a measured), while owner, transition and timestamp
/// stay the real ones rather than invented.
#[must_use]
pub fn page_corpus(traces: &[BrowsingTrace], table: &PageTable) -> Vec<BrowsingTrace> {
    let mut latest: BTreeMap<PageFingerprint, (String, TraceEvent)> = BTreeMap::new();
    for trace in traces {
        for event in &trace.events {
            // The table's own memo, so the address is canonicalized once for
            // the whole mint rather than once per fold.
            let Some(fingerprint) = table.by_address.get(&event.to.url).copied() else {
                continue;
            };
            match latest.get_mut(&fingerprint) {
                Some(entry) if event.at_ms >= entry.1.at_ms => {
                    *entry = (trace.owner.clone(), event.clone());
                }
                Some(_) => {}
                None => {
                    latest.insert(fingerprint, (trace.owner.clone(), event.clone()));
                }
            }
        }
    }
    let mut by_owner: BTreeMap<String, Vec<TraceEvent>> = BTreeMap::new();
    for (fingerprint, (owner, mut event)) in latest {
        let Some(record) = table.records.get(&fingerprint) else {
            continue;
        };
        event.to = PageRef {
            url: record.last_url.clone(),
            title: record.title.clone(),
        };
        by_owner.entry(owner).or_default().push(event);
    }
    by_owner
        .into_iter()
        .map(|(owner, mut events)| {
            events.sort_by_key(|event| event.at_ms);
            BrowsingTrace::from_events(owner, events)
        })
        .collect()
}

/// One disposable projection over the authoritative trace corpus.
pub struct MintedRecall {
    lexical: TrailIndex,
    /// The behavioural lane, folded keyed by page fingerprint (so visits split
    /// across a page's addresses sum into one score) then re-keyed to each
    /// record's address. Folded at mint against one reference `now`, so every
    /// query this index answers decays from the same instant.
    frecency: BTreeMap<String, f64>,
    /// Which records a typed query could mean, by token prefix over each
    /// record's addresses and title.
    candidates: CandidateIndex<String>,
    /// The same scores as `frecency`, by the candidate index's record id.
    frecency_by_id: Vec<f64>,
    documents: BTreeMap<String, RecallHit>,
    counts: MintCounts,
}

impl MintedRecall {
    /// Mint both lanes whole over `traces` with page bodies from `texts`.
    ///
    /// `memory_dir` names the memory store the index sits beside
    /// ([`index_dir`]); the projection stays in memory, since it is re-minted
    /// on every recall and never opened from disk. `now_ms` is the single
    /// reference instant the frecency fold decays from.
    ///
    /// # Errors
    /// The lexical rebuild's refusal.
    pub fn mint(
        memory_dir: &Path,
        traces: &[BrowsingTrace],
        texts: &PageTexts,
        now_ms: u64,
        observer: &mut dyn MintObserver,
    ) -> Result<Self, SearchError> {
        let (events, corpus_bytes, urls) = corpus_size(traces);
        let table = page_table(traces, texts.lookup());
        let documents = recall_documents(&table);
        let page_traces = page_corpus(traces, &table);
        observer.phase_done(MintPhase::PageTable);

        // The record's own text, not the snapshot's: the page table already
        // resolved which body belongs to the address a document is keyed by.
        let bodies: BTreeMap<&str, &str> = table
            .records
            .values()
            .filter_map(|record| Some((record.last_url.as_str(), record.text.as_deref()?)))
            .collect();
        let lexical = TrailIndex::rebuild_with_config(
            index_dir(memory_dir),
            &page_traces,
            |url| bodies.get(url).map(|text| (*text).to_string()),
            IndexConfig {
                persist: false,
                ..IndexConfig::default()
            },
        )?;
        observer.phase_done(MintPhase::Lexical);

        let frecency: BTreeMap<String, f64> =
            frecency_by_page(traces, now_ms, &FrecencyConfig::default(), &table)
                .into_iter()
                .filter_map(|(fingerprint, score)| {
                    table
                        .records
                        .get(&fingerprint)
                        .map(|record| (record.last_url.clone(), score))
                })
                .collect();
        observer.phase_done(MintPhase::Frecency);

        // The behavioural lane's narrowing step: every address that resolved
        // to the record, the address a hit opens, and the title.
        let mut frecency_by_id = Vec::with_capacity(table.records.len());
        let candidates = CandidateIndex::build(table.records.values().map(|record| {
            let mut texts: Vec<&str> = record.urls.iter().map(String::as_str).collect();
            texts.push(record.last_url.as_str());
            if let Some(title) = record.title.as_deref() {
                texts.push(title);
            }
            frecency_by_id.push(frecency.get(&record.last_url).copied().unwrap_or(0.0));
            (record.last_url.clone(), texts)
        }));
        observer.phase_done(MintPhase::Candidates);

        let counts = MintCounts {
            traces: traces.len(),
            events,
            pages: documents.len(),
            pages_with_text: bodies.len(),
            collapsed_urls: urls.saturating_sub(documents.len()),
            corpus_bytes,
            text_bytes: bodies.values().map(|text| text.len()).sum(),
            candidate_bytes: candidates.memory_bytes(),
        };
        Ok(Self {
            lexical,
            frecency,
            candidates,
            frecency_by_id,
            documents,
            counts,
        })
    }

    /// The page's frecency score at the mint's reference instant, by the
    /// address a hit opens.
    #[must_use]
    pub fn frecency_score(&self, url: &str) -> Option<f64> {
        self.frecency.get(url).copied()
    }

    /// What the mint saw.
    #[must_use]
    pub fn counts(&self) -> MintCounts {
        self.counts
    }

    /// Pages the typed text could mean, best-frecency first. The case this
    /// lane answers is a typed URL prefix, and the page a prefix should recall
    /// is the one visited most and most recently. Pages with no behavioural
    /// evidence (only redirects and reloads) score zero and stay out — an empty
    /// lane, not a zero-rank one.
    #[must_use]
    pub fn frecency_urls(&self, query: &str, limit: usize) -> Vec<String> {
        if limit == 0 {
            return Vec::new();
        }
        let mut scored: Vec<(u32, f64)> = self
            .candidates
            .candidate_ids(query)
            .into_iter()
            .map(|id| (id, self.frecency_by_id[id as usize]))
            .filter(|(_, score)| score.is_finite() && *score > 0.0)
            .collect();
        // Best score first, id as the tiebreak; only the head is ordered,
        // since a two-character prefix can name thousands of pages.
        let order = |left: &(u32, f64), right: &(u32, f64)| {
            right
                .1
                .total_cmp(&left.1)
                .then_with(|| left.0.cmp(&right.0))
        };
        if scored.len() > limit {
            scored.select_nth_unstable_by(limit - 1, order);
            scored.truncate(limit);
        }
        scored.sort_unstable_by(order);
        scored
            .into_iter()
            .map(|(id, _)| self.candidates.key(id).clone())
            .collect()
    }

    /// The fused ranking of the lanes that are on.
    ///
    /// # Errors
    /// The lexical search's refusal.
    pub fn fused_hits(
        &self,
        query: &str,
        limit: usize,
        config: RecallConfig,
    ) -> Result<Vec<FusedHit>, SearchError> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        let candidate_limit = limit.saturating_mul(FUSION_CANDIDATE_MULTIPLIER).max(limit);
        let lexical_hits = self.lexical.search(query, candidate_limit)?;
        let mut seen = HashSet::new();
        let mut lexical_urls = Vec::new();
        for hit in &lexical_hits {
            if seen.insert(hit.url.clone()) {
                lexical_urls.push(hit.url.clone());
            }
        }
        let frecency_urls = if config.frecency_enabled() {
            self.frecency_urls(query, candidate_limit)
        } else {
            Vec::new()
        };
        // Lane order is the contract fusion's named ranks read: lexical, then
        // behavioural.
        Ok(fuse_many(
            &[
                Ranking::new(&lexical_urls, 1.0),
                Ranking::new(&frecency_urls, f64::from(config.frecency_weight)),
            ],
            RRF_K,
        )
        .into_iter()
        .take(limit)
        .collect())
    }

    /// Answer a typed query.
    ///
    /// # Errors
    /// The lexical search's refusal.
    pub fn search(
        &self,
        query: &str,
        limit: usize,
        config: RecallConfig,
    ) -> Result<Vec<RecallHit>, SearchError> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        // The behavioural lane off is the compatibility path: preserve
        // TrailIndex's ranking and metadata exactly.
        if !config.frecency_enabled() {
            return Ok(self
                .lexical
                .search(query, limit)?
                .into_iter()
                .map(|hit| RecallHit {
                    url: hit.url,
                    title: hit.title,
                    at_ms: hit.at_ms,
                })
                .collect());
        }
        Ok(self
            .fused_hits(query, limit, config)?
            .into_iter()
            .filter_map(|fused| self.documents.get(&fused.url).cloned())
            .collect())
    }
}
