// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Hybrid fusion — the engine-agnostic seam between the recall lanes.
//!
//! BM25 + vector fusion is the standard retrieval-quality move, and the
//! geist's factual side rides the same seam (the derivation plan's E4).
//! This module deliberately knows nothing about any engine: the caller brings
//! **rankings** (URLs, best first) — one from [`TrailIndex::search`], one from
//! whatever vector index it uses (`intel/embed` today), one from the
//! behavioural fold (`eidetic::browsing::frecency`, wiring plan W6b) — and
//! gets a fused ranking back by reciprocal-rank fusion:
//!
//! `score(u) = Σ_i w_i / (k + rank_i(u) + 1)`
//!
//! RRF is rank-based, so the engines' incomparable score scales never meet;
//! `k` damps the head (60 is the literature's default); the weights are a
//! *setting* (the configurability rule), not a constant.
//!
//! [`fuse`] is the two-ranking call, [`fuse_many`] the N-ranking one; the
//! first is a wrapper over the second, so they cannot drift apart.
//!
//! [`TrailIndex::search`]: crate::index::TrailIndex::search

use std::collections::BTreeMap;

/// One input ranking: URLs best first, with the weight its lane carries.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ranking<'a> {
    pub urls: &'a [String],
    pub weight: f64,
}

impl<'a> Ranking<'a> {
    pub fn new(urls: &'a [String], weight: f64) -> Self {
        Self { urls, weight }
    }
}

/// One fused result: where it ranked in each input, and the fused score.
#[derive(Clone, Debug, PartialEq)]
pub struct FusedHit {
    pub url: String,
    pub score: f64,
    /// 0-based rank in the lexical input (ranking 0), when present there.
    pub lexical_rank: Option<usize>,
    /// 0-based rank in the vector input (ranking 1), when present there.
    pub vector_rank: Option<usize>,
    /// 0-based rank in every input, index-aligned with the rankings passed —
    /// how a lane beyond the first two shows its contribution.
    pub ranks: Vec<Option<usize>>,
}

/// Reciprocal-rank-fuse two rankings (URLs, best first). `k` damps head
/// dominance (60 unless you have a reason); `weights` are
/// `(lexical, vector)`. Ties break lexicographically by URL so the output
/// is deterministic.
pub fn fuse(lexical: &[String], vector: &[String], k: f64, weights: (f64, f64)) -> Vec<FusedHit> {
    fuse_many(
        &[
            Ranking::new(lexical, weights.0),
            Ranking::new(vector, weights.1),
        ],
        k,
    )
}

/// Reciprocal-rank-fuse any number of weighted rankings. Rankings 0 and 1 also
/// fill `lexical_rank` and `vector_rank`, so the two-lane callers keep reading
/// the fields they always read.
pub fn fuse_many(rankings: &[Ranking<'_>], k: f64) -> Vec<FusedHit> {
    let mut by_url: BTreeMap<&str, FusedHit> = BTreeMap::new();
    for (lane, ranking) in rankings.iter().enumerate() {
        for (rank, url) in ranking.urls.iter().enumerate() {
            let entry = by_url.entry(url).or_insert_with(|| FusedHit {
                url: url.clone(),
                score: 0.0,
                lexical_rank: None,
                vector_rank: None,
                ranks: vec![None; rankings.len()],
            });
            entry.ranks[lane] = Some(rank);
            match lane {
                0 => entry.lexical_rank = Some(rank),
                1 => entry.vector_rank = Some(rank),
                _ => {},
            }
            entry.score += ranking.weight / (k + rank as f64 + 1.0);
        }
    }
    let mut fused: Vec<FusedHit> = by_url.into_values().collect();
    fused.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.url.cmp(&b.url))
    });
    fused
}

#[cfg(test)]
mod tests {
    use super::*;

    fn urls(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn a_page_ranked_by_both_engines_beats_single_engine_pages() {
        let lexical = urls(&["https://both.example", "https://lex-only.example"]);
        let vector = urls(&["https://vec-only.example", "https://both.example"]);
        let fused = fuse(&lexical, &vector, 60.0, (1.0, 1.0));

        assert_eq!(fused[0].url, "https://both.example");
        assert_eq!(fused[0].lexical_rank, Some(0));
        assert_eq!(fused[0].vector_rank, Some(1));
        // Both single-engine hits surface too — fusion unions, never drops.
        assert_eq!(fused.len(), 3);
        let urls_out: Vec<&str> = fused.iter().map(|h| h.url.as_str()).collect();
        assert!(urls_out.contains(&"https://lex-only.example"));
        assert!(urls_out.contains(&"https://vec-only.example"));
    }

    #[test]
    fn weights_are_a_setting_that_actually_steers() {
        let lexical = urls(&["https://lex.example"]);
        let vector = urls(&["https://vec.example"]);

        let lexical_heavy = fuse(&lexical, &vector, 60.0, (2.0, 1.0));
        assert_eq!(lexical_heavy[0].url, "https://lex.example");

        let vector_heavy = fuse(&lexical, &vector, 60.0, (1.0, 2.0));
        assert_eq!(vector_heavy[0].url, "https://vec.example");
    }

    #[test]
    fn empty_inputs_fuse_to_the_other_side() {
        let vector = urls(&["https://only.example"]);
        let fused = fuse(&[], &vector, 60.0, (1.0, 1.0));
        assert_eq!(fused.len(), 1);
        assert_eq!(fused[0].url, "https://only.example");
        assert_eq!(fused[0].lexical_rank, None);
        assert_eq!(fused[0].vector_rank, Some(0));
    }

    /// The two-ranking path is the N-ranking path: same scores, same order,
    /// same named ranks — the generalization must not move W5's answers.
    #[test]
    fn two_rankings_agree_however_they_are_spelled() {
        let lexical = urls(&["https://both.example", "https://lex.example"]);
        let vector = urls(&["https://vec.example", "https://both.example"]);
        let paired = fuse(&lexical, &vector, 60.0, (1.0, 1.5));
        let many = fuse_many(
            &[Ranking::new(&lexical, 1.0), Ranking::new(&vector, 1.5)],
            60.0,
        );
        assert_eq!(paired, many);
        for hit in &paired {
            assert_eq!(hit.ranks, vec![hit.lexical_rank, hit.vector_rank]);
        }
    }

    /// Three lanes: the behavioural one lifts a page the other two never saw
    /// above a page only one of them ranks, and its rank stays readable.
    #[test]
    fn a_third_ranking_contributes_and_is_visible() {
        let lexical = urls(&["https://lex.example"]);
        let vector = urls(&["https://vec.example"]);
        let behavioural = urls(&["https://frecent.example", "https://lex.example"]);
        let fused = fuse_many(
            &[
                Ranking::new(&lexical, 1.0),
                Ranking::new(&vector, 1.0),
                Ranking::new(&behavioural, 2.0),
            ],
            60.0,
        );
        assert_eq!(fused.len(), 3, "fusion still unions, never drops");
        let order: Vec<&str> = fused.iter().map(|hit| hit.url.as_str()).collect();
        assert_eq!(
            order,
            vec![
                "https://lex.example",     // lexical + behavioural
                "https://frecent.example", // behavioural alone, but weighted
                "https://vec.example",     // vector alone
            ]
        );
        assert_eq!(fused[0].ranks, vec![Some(0), None, Some(1)]);
        assert_eq!(fused[0].lexical_rank, Some(0));
        assert_eq!(fused[0].vector_rank, None);
        assert_eq!(fused[1].ranks, vec![None, None, Some(0)]);

        // Zero weight is the lane's off switch: the page only it ranked falls
        // to last, and the two remaining lanes tie back to URL order.
        let without = fuse_many(
            &[
                Ranking::new(&lexical, 1.0),
                Ranking::new(&vector, 1.0),
                Ranking::new(&behavioural, 0.0),
            ],
            60.0,
        );
        let order: Vec<&str> = without.iter().map(|hit| hit.url.as_str()).collect();
        assert_eq!(
            order,
            vec![
                "https://lex.example",
                "https://vec.example",
                "https://frecent.example",
            ]
        );
    }
}
