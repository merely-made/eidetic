// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The trail index — an in-tree postings map over `BrowsingTrace` events.
//!
//! One document per traversal event, three scored fields (URL tokens, title,
//! page text) weighted by [`FieldWeights`], plus the stored columns (domain,
//! owner, at_ms, transition) the reports read directly — no re-index, no text
//! scan.
//!
//! The index is derived state. [`TrailIndex::rebuild`] re-mints it from the
//! trace corpus, and [`TrailIndex::open`] refuses a projection whose spec
//! sidecar doesn't match this build ([`SearchError::FormatMismatch`]) so the
//! caller re-mints instead. Persistence is a cache policy, not the engine's
//! job: [`IndexConfig::persist`] turns the disk write off for the re-mint hot
//! path, and the file itself holds the projected documents — postings are
//! rebuilt on `open`, so `open` and `rebuild` cannot rank differently.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use eidetic::browsing::BrowsingTrace;
use serde::{Deserialize, Serialize};

use crate::bm25::{Bm25Config, Bm25Index, idf};
use crate::spec::SearchIndexSpec;
use crate::tokenize::Tokenizer;
use crate::{Result, SearchError};

/// The persisted projection's file name inside an index directory.
pub const PROJECTION_FILE: &str = "trail-projection.json";

/// The scored fields, in postings-map order.
const FIELD_URL: usize = 0;
const FIELD_TITLE: usize = 1;
const FIELD_TEXT: usize = 2;
const FIELD_COUNT: usize = 3;

/// Per-field weights: a title match outranks a body match, which outranks a
/// URL-component match. Settings, not constants in the scorer.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct FieldWeights {
    pub url: f32,
    pub title: f32,
    pub text: f32,
}

impl Default for FieldWeights {
    fn default() -> Self {
        Self {
            url: 1.0,
            title: 3.0,
            text: 1.5,
        }
    }
}

impl FieldWeights {
    pub(crate) fn to_vec(self) -> Vec<f32> {
        let mut weights = vec![0.0; FIELD_COUNT];
        weights[FIELD_URL] = self.url;
        weights[FIELD_TITLE] = self.title;
        weights[FIELD_TEXT] = self.text;
        weights
    }
}

/// How a [`TrailIndex`] is minted.
#[derive(Clone, Copy, Debug, Default)]
pub struct IndexConfig {
    pub bm25: Bm25Config,
    pub weights: FieldWeights,
    pub tokenizer: Tokenizer,
    /// Write the projection to disk so [`TrailIndex::open`] can read it back.
    /// The consumer that re-mints on every recall wants this off.
    pub persist: bool,
}

impl IndexConfig {
    /// The minting defaults: literature BM25, title over text over URL, and a
    /// projection written to disk (what `rebuild` has always done).
    pub fn persistent() -> Self {
        Self {
            persist: true,
            ..Self::default()
        }
    }
}

/// One indexed traversal, and the columns the reports read.
#[derive(Clone, Debug, Serialize, Deserialize)]
struct TrailDoc {
    url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    /// Derived from the URL on every build, so it never reaches the file.
    #[serde(skip)]
    domain: String,
    owner: String,
    at_ms: u64,
    transition: String,
    /// Page main text, retained only to persist the projection.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    text: Option<String>,
}

/// The on-disk projection: the documents plus the scoring settings that made
/// them. Postings are rebuilt from this on `open`.
#[derive(Serialize, Deserialize)]
struct Projection {
    bm25: Bm25Config,
    weights: FieldWeights,
    docs: Vec<TrailDoc>,
}

/// The host of a URL, lowercased — good enough for report facets without a
/// URL-crate dependency (canonical URLs come in already normalized by the
/// import/capture paths).
fn domain_of(url: &str) -> String {
    let rest = url.split_once("://").map(|(_, r)| r).unwrap_or(url);
    rest.split(['/', '?', '#'])
        .next()
        .unwrap_or("")
        .to_ascii_lowercase()
}

/// A single absolute URL bypasses term scoring and takes the exact URL path,
/// so a query that *is* an identity is answered as one.
fn is_absolute_url_query(query: &str) -> bool {
    let Some((scheme, rest)) = query.split_once("://") else {
        return false;
    };
    !scheme.is_empty()
        && !rest.is_empty()
        && !query.chars().any(char::is_whitespace)
        && scheme
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'-' | b'.'))
}

/// One recall hit.
#[derive(Clone, Debug, PartialEq)]
pub struct Hit {
    pub url: String,
    pub title: Option<String>,
    pub at_ms: u64,
    pub score: f32,
}

/// The lexical index over a trail, at a directory.
pub struct TrailIndex {
    config: IndexConfig,
    postings: Bm25Index,
    docs: Vec<TrailDoc>,
    /// Canonical URL to the documents that carry it, for the exact lane.
    by_url: HashMap<String, Vec<u32>>,
    path: PathBuf,
}

impl TrailIndex {
    /// Open an existing projection, refusing on spec mismatch (re-mint instead
    /// — the corpus is the source of truth) and `Missing` when there is none.
    ///
    /// The persisted settings come back with it; a stemming hook does not,
    /// since it is a function pointer. An index minted with one is re-minted,
    /// not reopened.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let on_disk = SearchIndexSpec::read_sidecar(&path)?;
        if !on_disk.matches_current() {
            return Err(SearchError::FormatMismatch {
                found: on_disk.describe(),
                current: SearchIndexSpec::current().describe(),
            });
        }
        let bytes = std::fs::read(path.join(PROJECTION_FILE))
            .map_err(|_| SearchError::Missing(path.display().to_string()))?;
        let projection: Projection = serde_json::from_slice(&bytes)
            .map_err(|e| SearchError::Engine(format!("projection parse: {e}")))?;
        let config = IndexConfig {
            bm25: projection.bm25,
            weights: projection.weights,
            tokenizer: Tokenizer::new(),
            persist: true,
        };
        Ok(Self::from_docs(config, projection.docs, path))
    }

    /// Re-mint the index at `path` from the trace corpus.
    pub fn rebuild<'a>(
        path: impl AsRef<Path>,
        traces: impl IntoIterator<Item = &'a BrowsingTrace>,
    ) -> Result<Self> {
        Self::rebuild_with_text(path, traces, |_| None)
    }

    /// Re-mint like [`rebuild`](Self::rebuild) but enrich each event's document
    /// with the page's main text, so BM25 recall reaches the body, not just the
    /// title/URL. `text_for(url)` supplies the extracted text for a visited URL,
    /// or `None` when no body is cached. (Capture plan C5.)
    pub fn rebuild_with_text<'a>(
        path: impl AsRef<Path>,
        traces: impl IntoIterator<Item = &'a BrowsingTrace>,
        text_for: impl Fn(&str) -> Option<String>,
    ) -> Result<Self> {
        Self::rebuild_with_config(path, traces, text_for, IndexConfig::persistent())
    }

    /// Re-mint under explicit settings — the BM25 knobs, the field weights, the
    /// tokenizer, and whether the projection reaches disk at all.
    pub fn rebuild_with_config<'a>(
        path: impl AsRef<Path>,
        traces: impl IntoIterator<Item = &'a BrowsingTrace>,
        text_for: impl Fn(&str) -> Option<String>,
        config: IndexConfig,
    ) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let mut docs = Vec::new();
        for trace in traces {
            for event in &trace.events {
                docs.push(TrailDoc {
                    url: event.to.url.clone(),
                    title: event.to.title.clone(),
                    domain: String::new(),
                    owner: trace.owner.clone(),
                    at_ms: event.at_ms,
                    transition: format!("{:?}", event.transition),
                    text: text_for(&event.to.url),
                });
            }
        }
        let index = Self::from_docs(config, docs, path);
        if config.persist {
            index.write()?;
        }
        Ok(index)
    }

    /// Build the postings map, the exact-URL lane and the domain column over a
    /// document set — the one path `rebuild` and `open` both take, so a
    /// reopened projection cannot rank differently from a fresh mint.
    fn from_docs(config: IndexConfig, mut docs: Vec<TrailDoc>, path: PathBuf) -> Self {
        let mut postings = Bm25Index::new(config.bm25, config.weights.to_vec());
        let mut by_url: HashMap<String, Vec<u32>> = HashMap::new();
        let mut fields: Vec<Vec<String>> = vec![Vec::new(); FIELD_COUNT];
        for (position, doc) in docs.iter_mut().enumerate() {
            for field in &mut fields {
                field.clear();
            }
            config
                .tokenizer
                .tokens_into(&doc.url, &mut fields[FIELD_URL]);
            if let Some(title) = &doc.title {
                config
                    .tokenizer
                    .tokens_into(title, &mut fields[FIELD_TITLE]);
            }
            if let Some(text) = &doc.text {
                config.tokenizer.tokens_into(text, &mut fields[FIELD_TEXT]);
            }
            postings.add_document(&fields);
            by_url
                .entry(doc.url.clone())
                .or_default()
                .push(position as u32);
            doc.domain = domain_of(&doc.url);
            if !config.persist {
                // Bodies are only held in order to write them out.
                doc.text = None;
            }
        }
        Self {
            config,
            postings,
            docs,
            by_url,
            path,
        }
    }

    /// Write the projection and its spec sidecar into the index directory,
    /// replacing whatever was there.
    fn write(&self) -> Result<()> {
        if self.path.exists() {
            std::fs::remove_dir_all(&self.path)?;
        }
        std::fs::create_dir_all(&self.path)?;
        let projection = Projection {
            bm25: self.config.bm25,
            weights: self.config.weights,
            docs: self.docs.clone(),
        };
        let bytes = serde_json::to_vec(&projection)
            .map_err(|e| SearchError::Engine(format!("projection serialize: {e}")))?;
        std::fs::write(self.path.join(PROJECTION_FILE), bytes)?;
        SearchIndexSpec::current().write_sidecar(&self.path)
    }

    /// Where the index lives.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// How many traversal documents the index holds.
    pub fn doc_count(&self) -> Result<u64> {
        Ok(self.docs.len() as u64)
    }

    /// BM25 recall over tokenized titles, page text, and URL components. A
    /// single absolute-URL query takes the exact canonical-URL lane instead, so
    /// an identity query is answered as an identity. Hits come back ranked,
    /// newest-irrelevant — relevance is the ranking; time is a column the
    /// caller can re-sort by.
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<Hit>> {
        let query = query.trim();
        let limit = limit.max(1);
        let ranked: Vec<(u32, f32)> = if is_absolute_url_query(query) {
            match self.by_url.get(query) {
                Some(docs) => {
                    let score = idf(self.docs.len(), docs.len());
                    docs.iter().take(limit).map(|doc| (*doc, score)).collect()
                }
                None => Vec::new(),
            }
        } else {
            self.postings
                .search(&self.config.tokenizer.tokens(query), limit)
        };
        Ok(ranked
            .into_iter()
            .map(|(doc, score)| {
                let doc = &self.docs[doc as usize];
                Hit {
                    url: doc.url.clone(),
                    title: doc.title.clone(),
                    at_ms: doc.at_ms,
                    score,
                }
            })
            .collect())
    }

    /// Report: the most-visited domains, by traversal count, over the stored
    /// columns (no re-index, no text scan).
    pub fn top_domains(&self, n: usize) -> Result<Vec<(String, u64)>> {
        let mut counts: BTreeMap<&str, u64> = BTreeMap::new();
        for doc in &self.docs {
            *counts.entry(doc.domain.as_str()).or_insert(0) += 1;
        }
        let mut out: Vec<(String, u64)> = counts
            .into_iter()
            .map(|(domain, count)| (domain.to_string(), count))
            .collect();
        out.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        out.truncate(n);
        Ok(out)
    }

    /// Report: traversals over time, bucketed at `interval_ms` (a day is
    /// 86_400_000). Returns `(bucket_start_ms, count)` for non-empty
    /// buckets, ascending.
    pub fn visits_histogram(&self, interval_ms: u64) -> Result<Vec<(u64, u64)>> {
        let interval = interval_ms.max(1);
        let mut buckets: BTreeMap<u64, u64> = BTreeMap::new();
        for doc in &self.docs {
            *buckets.entry(doc.at_ms / interval * interval).or_insert(0) += 1;
        }
        Ok(buckets.into_iter().collect())
    }
}

#[cfg(test)]
mod tests;
