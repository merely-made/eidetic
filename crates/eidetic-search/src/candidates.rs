// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Which records could a typed query mean — a token-prefix lookup.
//!
//! A ranking lane that is not the lexical one (the behavioural lane in
//! `eidetic::browsing::frecency`, say) still has to narrow the corpus to the
//! records a query could mean before it ranks them. Scanning every record per
//! keystroke does not scale: at 42k pages a lowercase-and-substring scan cost
//! three orders of magnitude more than the BM25 query beside it.
//!
//! [`CandidateIndex`] answers the same question from the same token stream the
//! scorer uses ([`Tokenizer`]), so a candidate set and a BM25 hit agree about
//! what a word is. Hosts need no special case: the tokenizer already splits
//! `example.test` into `example` and `test`, so `ex` reaches both.
//!
//! **Semantics: token prefix, not substring.** Every query token must prefix
//! *some* indexed token of a record (AND across query tokens, prefix on each).
//! A mid-word substring therefore does not match — `zette` no longer finds
//! `gazette`. That is the trade for the scale: prefix is what a typed address
//! is, and it is answerable from a sorted token array.
//!
//! Field-agnostic and key-agnostic, so it can be exercised alone.

use std::collections::HashMap;
use std::ops::Range;

use crate::tokenize::Tokenizer;

/// A sorted token array with a postings list per token, over caller keys.
///
/// The layout is four flat arrays, not a trie or a map of `Vec`s: at 42k pages
/// a per-token `String` plus `Vec` header costs more than the tokens do. Token
/// `i` is `token_blob[token_starts[i]..token_starts[i + 1]]` and its record ids
/// are `postings[posting_starts[i]..posting_starts[i + 1]]`, both sorted, so a
/// prefix range is two binary searches and a union is a slice walk.
/// [`memory_bytes`](Self::memory_bytes) reports the whole of it.
#[derive(Debug, Default)]
pub struct CandidateIndex<K> {
    keys: Vec<K>,
    token_blob: String,
    /// `tokens + 1` entries: byte offsets into `token_blob`.
    token_starts: Vec<u32>,
    /// Record ids, ascending and deduplicated within each token's run.
    postings: Vec<u32>,
    /// `tokens + 1` entries: offsets into `postings`.
    posting_starts: Vec<u32>,
    tokenizer: Tokenizer,
}

impl<K> CandidateIndex<K> {
    /// Index `entries` with the default tokenizer. Each entry is a key and the
    /// texts that should recall it — for a page, its address and its title.
    pub fn build<'a, T: AsRef<[&'a str]>>(entries: impl IntoIterator<Item = (K, T)>) -> Self {
        Self::build_with(entries, Tokenizer::new())
    }

    /// The same, with the tokenizer the caller's scorer uses.
    pub fn build_with<'a, T: AsRef<[&'a str]>>(
        entries: impl IntoIterator<Item = (K, T)>,
        tokenizer: Tokenizer,
    ) -> Self {
        let mut keys = Vec::new();
        let mut by_token: HashMap<String, Vec<u32>> = HashMap::new();
        let mut buf: Vec<String> = Vec::new();
        for (key, texts) in entries {
            let id = keys.len() as u32;
            keys.push(key);
            buf.clear();
            for text in texts.as_ref() {
                tokenizer.tokens_into(text, &mut buf);
            }
            // One posting per token per record: dedup here so the postings are
            // ascending and unique by construction, never by a later pass.
            buf.sort_unstable();
            buf.dedup();
            for token in buf.drain(..) {
                by_token.entry(token).or_default().push(id);
            }
        }

        let mut terms: Vec<(String, Vec<u32>)> = by_token.into_iter().collect();
        terms.sort_unstable_by(|left, right| left.0.cmp(&right.0));
        let mut index = Self {
            keys,
            token_blob: String::new(),
            token_starts: Vec::with_capacity(terms.len() + 1),
            postings: Vec::new(),
            posting_starts: Vec::with_capacity(terms.len() + 1),
            tokenizer,
        };
        for (token, ids) in &terms {
            index.token_starts.push(index.token_blob.len() as u32);
            index.token_blob.push_str(token);
            index.posting_starts.push(index.postings.len() as u32);
            index.postings.extend_from_slice(ids);
        }
        index.token_starts.push(index.token_blob.len() as u32);
        index.posting_starts.push(index.postings.len() as u32);
        index
    }

    /// The keys every query token prefixes a token of, in insertion order and
    /// without repeats. An empty query, or one whose tokens are all unmatched,
    /// yields nothing — an empty lane, not the whole corpus.
    pub fn candidates(&self, query: &str) -> impl Iterator<Item = &K> {
        self.candidate_ids(query)
            .into_iter()
            .map(|id| &self.keys[id as usize])
    }

    /// The key behind a record id from [`candidate_ids`](Self::candidate_ids).
    pub fn key(&self, id: u32) -> &K {
        &self.keys[id as usize]
    }

    /// [`candidates`](Self::candidates) as record ids, ascending: the ids are
    /// the insertion order of `build`, so a caller that keeps a per-record
    /// score in a `Vec` built in the same order scores a wide candidate set
    /// by index rather than by key lookup.
    pub fn candidate_ids(&self, query: &str) -> Vec<u32> {
        let mut found: Option<Vec<u32>> = None;
        for token in self.tokenizer.tokens(query) {
            let matched = self.matching(&token);
            found = Some(match found {
                None => matched,
                Some(previous) => intersect(&previous, &matched),
            });
            if found.as_ref().is_some_and(Vec::is_empty) {
                break;
            }
        }
        found.unwrap_or_default()
    }

    /// How many records the index covers.
    pub fn len(&self) -> usize {
        self.keys.len()
    }

    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    /// Distinct tokens across every indexed text.
    pub fn token_count(&self) -> usize {
        self.token_starts.len().saturating_sub(1)
    }

    /// Heap bytes the index itself holds. The keys are the caller's own values
    /// and are not counted — a `CandidateIndex<K>` cannot size an arbitrary `K`.
    pub fn memory_bytes(&self) -> usize {
        self.token_blob.len()
            + size_of::<u32>()
                * (self.token_starts.len() + self.postings.len() + self.posting_starts.len())
    }

    fn token(&self, index: usize) -> &str {
        let start = self.token_starts[index] as usize;
        let end = self.token_starts[index + 1] as usize;
        &self.token_blob[start..end]
    }

    fn ids(&self, index: usize) -> &[u32] {
        let start = self.posting_starts[index] as usize;
        let end = self.posting_starts[index + 1] as usize;
        &self.postings[start..end]
    }

    /// The contiguous run of sorted tokens carrying `prefix`.
    fn prefix_range(&self, prefix: &str) -> Range<usize> {
        let count = self.token_count();
        let start = partition_point(0..count, |i| self.token(i) < prefix);
        let end = partition_point(start..count, |i| self.token(i).starts_with(prefix));
        start..end
    }

    /// Every record under `prefix`, ascending and deduplicated.
    fn matching(&self, prefix: &str) -> Vec<u32> {
        let range = self.prefix_range(prefix);
        match range.len() {
            0 => Vec::new(),
            // The common case for a settled word: no merge, no sort.
            1 => self.ids(range.start).to_vec(),
            _ => {
                let mut out = Vec::new();
                for index in range {
                    out.extend_from_slice(self.ids(index));
                }
                out.sort_unstable();
                out.dedup();
                out
            },
        }
    }
}

/// The first index in `range` where `pred` stops holding. `pred` must be true
/// for a prefix of the range and false after it.
fn partition_point(range: Range<usize>, mut pred: impl FnMut(usize) -> bool) -> usize {
    let (mut low, mut high) = (range.start, range.end);
    while low < high {
        let mid = low + (high - low) / 2;
        if pred(mid) {
            low = mid + 1;
        } else {
            high = mid;
        }
    }
    low
}

/// Intersection of two ascending, deduplicated id lists.
fn intersect(left: &[u32], right: &[u32]) -> Vec<u32> {
    let mut out = Vec::with_capacity(left.len().min(right.len()));
    let (mut l, mut r) = (0, 0);
    while l < left.len() && r < right.len() {
        match left[l].cmp(&right[r]) {
            std::cmp::Ordering::Less => l += 1,
            std::cmp::Ordering::Greater => r += 1,
            std::cmp::Ordering::Equal => {
                out.push(left[l]);
                l += 1;
                r += 1;
            },
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn corpus() -> CandidateIndex<&'static str> {
        CandidateIndex::build([
            ("a", ["https://example.test/rust", "The Gazette Morning"]),
            (
                "b",
                ["https://elsewhere.test/gazette", "Vello Scene Encoding"],
            ),
            ("c", ["https://example.test/vello", "Rust Async Book"]),
        ])
    }

    fn found<'a>(index: &'a CandidateIndex<&'static str>, query: &str) -> Vec<&'a &'static str> {
        index.candidates(query).collect()
    }

    #[test]
    fn a_title_token_matches_by_prefix() {
        let index = corpus();
        assert_eq!(found(&index, "gaz"), [&"a", &"b"]);
        assert_eq!(found(&index, "morn"), [&"a"]);
        assert_eq!(found(&index, "encoding"), [&"b"]);
    }

    #[test]
    fn a_host_component_matches_by_prefix() {
        // The shared tokenizer already splits `example.test`, so the host
        // parts are ordinary tokens and need no separate decomposition.
        let index = corpus();
        assert_eq!(found(&index, "ex"), [&"a", &"c"]);
        assert_eq!(found(&index, "elsewhere"), [&"b"]);
        // Every record is on `.test`, and both hosts prefix-match `e`.
        assert_eq!(found(&index, "te"), [&"a", &"b", &"c"]);
    }

    #[test]
    fn every_query_token_must_match() {
        let index = corpus();
        assert_eq!(found(&index, "example vello"), [&"c"]);
        assert_eq!(found(&index, "gaz morn"), [&"a"]);
        // `elsewhere` and `rust` never share a record.
        assert!(found(&index, "elsewhere rust").is_empty());
        assert!(found(&index, "gazette nowhere").is_empty());
    }

    #[test]
    fn a_key_appears_once_however_many_tokens_match() {
        // `example` occurs in a's URL and `e` prefixes `example`, `test` and
        // `elsewhere`; the union still names each record once.
        let index = corpus();
        assert_eq!(found(&index, "e"), [&"a", &"b", &"c"]);
        let repeated = CandidateIndex::build([("solo", ["gazette gazette", "Gazette"])]);
        assert_eq!(repeated.candidates("gaz").count(), 1);
    }

    #[test]
    fn an_empty_query_yields_no_candidates() {
        let index = corpus();
        assert!(found(&index, "").is_empty());
        assert!(found(&index, "   ---  ").is_empty());
        assert!(
            CandidateIndex::<&str>::build::<[&str; 0]>([])
                .candidates("gazette")
                .next()
                .is_none()
        );
    }

    #[test]
    fn a_mid_word_substring_no_longer_matches() {
        // The behaviour change from the whole-query substring scan this index
        // replaces: prefixes reach a token, interior fragments do not.
        let index = corpus();
        assert_eq!(found(&index, "zette"), Vec::<&&str>::new());
        assert!(found(&index, "azette").is_empty());
        assert!(found(&index, "ello").is_empty());
        assert!(found(&index, "ncoding").is_empty());
        // The same words reached by their prefixes still match.
        assert_eq!(found(&index, "gaz"), [&"a", &"b"]);
        assert_eq!(found(&index, "vell"), [&"b", &"c"]);
    }

    #[test]
    fn the_layout_stays_consistent_with_what_it_indexed() {
        let index = corpus();
        assert_eq!(index.len(), 3);
        assert_eq!(index.token_starts.len(), index.token_count() + 1);
        assert_eq!(index.posting_starts.len(), index.token_count() + 1);
        for i in 1..index.token_count() {
            assert!(index.token(i - 1) < index.token(i), "tokens are sorted");
            assert!(index.ids(i).windows(2).all(|w| w[0] < w[1]), "ids ascend");
        }
        assert!(index.memory_bytes() > 0);
    }

    /// Build and query at the corpus size W6e measured (41,822 pages), which
    /// is where the scan this index replaces cost 20.8-22.7 ms per keystroke.
    #[test]
    #[ignore = "timing receipt; run with --release --nocapture"]
    fn candidate_index_at_corpus_scale() {
        use std::time::Instant;

        const PAGES: usize = 42_000;
        let hosts = ["example", "elsewhere", "gazette", "vello", "kestrel"];
        let words = [
            "rust", "async", "book", "scene", "encoding", "morning", "paper", "index", "search",
            "memory", "trail", "canvas", "shader", "render", "atlas",
        ];
        let rows: Vec<(String, String)> = (0..PAGES)
            .map(|i| {
                let host = hosts[i % hosts.len()];
                let url = format!(
                    "https://{host}{}.test/{}/{i}",
                    i % 97,
                    words[i % words.len()]
                );
                let title = format!(
                    "{} {} {i}",
                    words[(i / 3) % words.len()],
                    words[(i / 7) % words.len()]
                );
                (url, title)
            })
            .collect();

        let started = Instant::now();
        let index = CandidateIndex::build(
            rows.iter()
                .map(|(url, title)| (url.clone(), [url.as_str(), title.as_str()])),
        );
        let build = started.elapsed();

        // Two-character queries drawn from the corpus's own hosts and title
        // words, the way W6e's receipt takes them from the busiest pages: a
        // prefix nobody ever typed measures only the empty path. A host prefix
        // spans all 97 host variants, so this is the wide-union case too.
        let queries: Vec<String> = (0..100)
            .map(|i| {
                let word = if i % 2 == 0 {
                    hosts[(i / 2) % hosts.len()]
                } else {
                    words[(i / 2) % words.len()]
                };
                word.chars().take(2).collect()
            })
            .collect();
        let mut timings = Vec::new();
        let mut hits = 0_usize;
        for query in &queries {
            let started = Instant::now();
            hits += index.candidates(query).count();
            timings.push(started.elapsed().as_micros());
        }
        timings.sort_unstable();

        let key_bytes: usize = rows
            .iter()
            .map(|(url, _)| url.len() + size_of::<String>())
            .sum();
        println!(
            "candidate index pages={PAGES} tokens={} build_ms={:.1} index_bytes={} bytes_per_page={:.1} with_keys={:.1} median_query_us={} p90_us={} max_us={} total_candidates={hits}",
            index.token_count(),
            build.as_secs_f64() * 1e3,
            index.memory_bytes(),
            index.memory_bytes() as f64 / PAGES as f64,
            (index.memory_bytes() + key_bytes) as f64 / PAGES as f64,
            timings[timings.len() / 2],
            timings[timings.len() * 9 / 10],
            timings[timings.len() - 1],
        );
        assert_eq!(index.len(), PAGES);
    }
}
