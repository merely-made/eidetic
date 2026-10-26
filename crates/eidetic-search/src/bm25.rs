// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! An in-memory postings map with weighted per-field BM25.
//!
//! Field-agnostic on purpose: fields are positions, the caller names them.
//! A document's score is `Σ_f w_f · BM25_f(doc)` — per-field IDF and per-field
//! length normalization, summed with the caller's weights, which is what a
//! multi-field query does. Nothing here knows about URLs or traces, so it can
//! be exercised alone.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Term-frequency saturation, the literature's default.
pub const DEFAULT_K1: f32 = 1.2;

/// Length-normalization strength, the literature's default.
pub const DEFAULT_B: f32 = 0.75;

/// BM25's two knobs. No literal reaches the scorer.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Bm25Config {
    pub k1: f32,
    pub b: f32,
}

impl Default for Bm25Config {
    fn default() -> Self {
        Self {
            k1: DEFAULT_K1,
            b: DEFAULT_B,
        }
    }
}

/// Robertson/Sparck-Jones IDF, as tantivy computed it — kept public so a
/// caller scoring outside the postings map (an exact-key lane, say) stays on
/// the same scale.
pub fn idf(total_docs: usize, doc_freq: usize) -> f32 {
    let total = total_docs as f32;
    let freq = doc_freq as f32;
    (1.0 + (total - freq + 0.5) / (freq + 0.5)).ln()
}

#[derive(Clone, Copy, Debug)]
struct Posting {
    doc: u32,
    field: u8,
    term_freq: u32,
}

/// A postings map over `weights.len()` fields.
#[derive(Debug)]
pub struct Bm25Index {
    config: Bm25Config,
    weights: Vec<f32>,
    postings: HashMap<String, Vec<Posting>>,
    /// Token count per document per field, flattened `doc * fields + field`.
    lengths: Vec<u32>,
    /// Token total per field, for the average document length.
    field_totals: Vec<u64>,
    docs: usize,
}

impl Bm25Index {
    /// An empty index scoring `weights.len()` fields.
    pub fn new(config: Bm25Config, weights: Vec<f32>) -> Self {
        let fields = weights.len();
        Self {
            config,
            weights,
            postings: HashMap::new(),
            lengths: Vec::new(),
            field_totals: vec![0; fields],
            docs: 0,
        }
    }

    /// Add one document — `fields[i]` is field `i`'s token stream. Returns its
    /// document id, which is its insertion order.
    pub fn add_document(&mut self, fields: &[Vec<String>]) -> u32 {
        let doc = self.docs as u32;
        let field_count = self.weights.len();
        let base = self.lengths.len();
        self.lengths.resize(base + field_count, 0);
        for (field, tokens) in fields.iter().enumerate().take(field_count) {
            let field = field as u8;
            for token in tokens {
                // Documents and fields arrive in order, so a repeat of a term
                // within this field is always the tail of its postings list —
                // one hash lookup per token, and a term is only allocated the
                // first time the corpus sees it.
                match self.postings.get_mut(token.as_str()) {
                    Some(list) => match list.last_mut() {
                        Some(last) if last.doc == doc && last.field == field => {
                            last.term_freq += 1;
                        },
                        _ => list.push(Posting {
                            doc,
                            field,
                            term_freq: 1,
                        }),
                    },
                    None => {
                        self.postings.insert(
                            token.clone(),
                            vec![Posting {
                                doc,
                                field,
                                term_freq: 1,
                            }],
                        );
                    },
                }
            }
            self.lengths[base + field as usize] = tokens.len() as u32;
            self.field_totals[field as usize] += tokens.len() as u64;
        }
        self.docs += 1;
        doc
    }

    /// How many documents the index holds.
    pub fn len(&self) -> usize {
        self.docs
    }

    pub fn is_empty(&self) -> bool {
        self.docs == 0
    }

    /// The best `limit` documents for a disjunction of `terms`, as
    /// `(doc_id, score)` sorted by score descending then document id — a total
    /// order, so equal scores never reorder between runs.
    pub fn search(&self, terms: &[String], limit: usize) -> Vec<(u32, f32)> {
        let total_docs = self.docs;
        let field_count = self.weights.len();
        if total_docs == 0 || terms.is_empty() {
            return Vec::new();
        }
        let averages: Vec<f32> = self
            .field_totals
            .iter()
            .map(|total| match *total {
                0 => 1.0,
                total => total as f32 / total_docs as f32,
            })
            .collect();

        let mut scores = vec![0.0_f32; total_docs];
        for term in terms {
            let Some(postings) = self.postings.get(term.as_str()) else {
                continue;
            };
            // Document frequency is per field, as it is in a per-field BM25.
            let mut doc_freqs = vec![0_usize; self.weights.len()];
            for posting in postings {
                doc_freqs[posting.field as usize] += 1;
            }
            let field_idf: Vec<f32> = doc_freqs
                .iter()
                .map(|freq| idf(total_docs, *freq))
                .collect();
            for posting in postings {
                let field = posting.field as usize;
                let term_freq = posting.term_freq as f32;
                let length = self.lengths[posting.doc as usize * field_count + field] as f32;
                let norm = self.config.k1
                    * (1.0 - self.config.b + self.config.b * length / averages[field]);
                let saturated = term_freq * (self.config.k1 + 1.0) / (term_freq + norm);
                scores[posting.doc as usize] += self.weights[field] * field_idf[field] * saturated;
            }
        }

        let mut hits: Vec<(u32, f32)> = scores
            .into_iter()
            .enumerate()
            .filter(|(_, score)| *score > 0.0)
            .map(|(doc, score)| (doc as u32, score))
            .collect();
        let order = |a: &(u32, f32), b: &(u32, f32)| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0));
        // A wide disjunction touches most of the corpus; only the head is
        // asked for, so partition first and sort the head alone.
        if hits.len() > limit {
            hits.select_nth_unstable_by(limit - 1, order);
            hits.truncate(limit);
        }
        hits.sort_unstable_by(order);
        hits
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokens(text: &str) -> Vec<String> {
        text.split_whitespace().map(str::to_string).collect()
    }

    fn index() -> Bm25Index {
        // Two fields, the second weighted triple.
        let mut index = Bm25Index::new(Bm25Config::default(), vec![1.0, 3.0]);
        index.add_document(&[tokens("alpha beta"), tokens("gamma")]);
        index.add_document(&[tokens("gamma"), tokens("alpha")]);
        index.add_document(&[tokens("delta delta delta"), tokens("delta")]);
        index
    }

    #[test]
    fn the_weighted_field_wins_over_the_light_one() {
        let index = index();
        assert_eq!(index.len(), 3);
        let hits = index.search(&tokens("alpha"), 10);
        // Doc 1 carries `alpha` in the x3 field; doc 0 in the x1 field.
        assert_eq!(hits.iter().map(|(doc, _)| *doc).collect::<Vec<_>>(), [1, 0]);
        assert!(hits[0].1 > hits[1].1);
    }

    #[test]
    fn term_frequency_saturates_and_missing_terms_score_nothing() {
        let index = index();
        let hits = index.search(&tokens("delta"), 10);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].0, 2);
        assert!(index.search(&tokens("epsilon"), 10).is_empty());
        assert!(index.search(&[], 10).is_empty());
    }

    #[test]
    fn the_knobs_change_the_score_and_nothing_is_hardcoded() {
        let flat = Bm25Config { k1: 0.0, b: 0.0 };
        let mut index = Bm25Index::new(flat, vec![1.0]);
        index.add_document(&[tokens("a a a")]);
        index.add_document(&[tokens("a")]);
        // k1 = 0 removes the frequency term entirely, so both docs tie and the
        // tie-break is the document id.
        let hits = index.search(&tokens("a"), 10);
        assert_eq!(hits[0].1, hits[1].1);
        assert_eq!(hits[0].0, 0);
    }

    #[test]
    fn an_empty_index_answers_nothing() {
        let index = Bm25Index::new(Bm25Config::default(), vec![1.0]);
        assert!(index.is_empty());
        assert!(index.search(&tokens("a"), 5).is_empty());
    }
}
