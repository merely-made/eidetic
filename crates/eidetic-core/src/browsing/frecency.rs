// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Frecency — the behavioural ranking, folded straight from the trace corpus.
//!
//! A pure fold over [`BrowsingTrace`]s: every traversal contributes its
//! transition's weight, decayed by an exponential half-life on `at_ms` and
//! lifted when `dwell_ms` says the visit was an interesting one. No index, no
//! state, no clock — the caller passes `now_ms`, so the same corpus and the
//! same reference time always produce the same table.
//!
//! Everything the fold uses comes from [`FrecencyConfig`]. The defaults are
//! Firefox's `places.frecency.*VisitBonus` percentages over 100 (typed 2000,
//! link 100, bookmark 75, redirect and reload 0) under the 30-day half-life
//! its ranking doc describes.
//!
//! The key is the destination URL today and a content fingerprint later
//! (wiring plan W6d), so the fold is written over a key function
//! ([`frecency_by`]) with [`frecency`] as the URL-keyed wrapper.

use std::collections::BTreeMap;

use super::{BrowsingTrace, TraceEvent, TraceTransition};

/// Milliseconds in a day, the unit both defaults below are expressed in.
const DAY_MS: u64 = 24 * 60 * 60 * 1_000;

/// What one visit is worth before decay, per transition kind.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TransitionWeights {
    pub link_click: f64,
    pub url_typed: f64,
    /// History moves: the page was already scored when it was first chosen.
    pub back: f64,
    pub forward: f64,
    pub reload: f64,
    pub redirect: f64,
    pub tab_spawn: f64,
    /// Session restore — the browser's choice, not the user's.
    pub restore: f64,
    pub imported: f64,
    /// A traversal whose cause the recorder could not name. Scored as a link
    /// click rather than Firefox's zero default: turnstone's engine callback
    /// cannot separate a link activation from a redirect, so zero here would
    /// silence a live corpus.
    pub unknown: f64,
}

impl TransitionWeights {
    /// The weight for one kind. Exhaustive by construction, so a new
    /// [`TraceTransition`] cannot silently inherit someone else's weight.
    pub fn of(&self, transition: TraceTransition) -> f64 {
        match transition {
            TraceTransition::LinkClick => self.link_click,
            TraceTransition::UrlTyped => self.url_typed,
            TraceTransition::Back => self.back,
            TraceTransition::Forward => self.forward,
            TraceTransition::Reload => self.reload,
            TraceTransition::Redirect => self.redirect,
            TraceTransition::TabSpawn => self.tab_spawn,
            TraceTransition::Restore => self.restore,
            TraceTransition::Imported => self.imported,
            TraceTransition::Unknown => self.unknown,
        }
    }
}

impl Default for TransitionWeights {
    fn default() -> Self {
        Self {
            link_click: 1.0,
            url_typed: 20.0,
            back: 0.25,
            forward: 0.25,
            reload: 0.0,
            redirect: 0.0,
            tab_spawn: 1.0,
            restore: 0.0,
            imported: 0.75,
            unknown: 1.0,
        }
    }
}

/// The whole fold's policy: weights, decay, and the interaction bonus.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrecencyConfig {
    pub weights: TransitionWeights,
    /// Age at which a visit is worth half. Zero disables decay entirely.
    pub half_life_ms: u64,
    /// Added to a visit's multiplier once its dwell reaches the threshold
    /// ("interesting interaction" in Mozilla's ranking doc).
    pub dwell_bonus: f64,
    pub dwell_threshold_ms: u64,
}

impl Default for FrecencyConfig {
    fn default() -> Self {
        Self {
            weights: TransitionWeights::default(),
            half_life_ms: 30 * DAY_MS,
            dwell_bonus: 0.5,
            dwell_threshold_ms: 30_000,
        }
    }
}

impl FrecencyConfig {
    /// How much of a visit's weight survives to `now_ms`. Visits stamped in
    /// the future decay by nothing rather than gaining.
    pub fn decay(&self, now_ms: u64, at_ms: u64) -> f64 {
        if self.half_life_ms == 0 {
            return 1.0;
        }
        let age = now_ms.saturating_sub(at_ms) as f64;
        0.5f64.powf(age / self.half_life_ms as f64)
    }

    /// The interaction multiplier for one visit's dwell.
    pub fn dwell_multiplier(&self, dwell_ms: Option<u64>) -> f64 {
        match dwell_ms {
            Some(dwell) if dwell >= self.dwell_threshold_ms => 1.0 + self.dwell_bonus,
            _ => 1.0,
        }
    }

    /// One visit's contribution, before it is summed into its page's score.
    pub fn visit_score(&self, event: &TraceEvent, now_ms: u64) -> f64 {
        self.weights.of(event.transition)
            * self.decay(now_ms, event.at_ms)
            * self.dwell_multiplier(event.dwell_ms)
    }
}

/// Frecency per destination URL over the corpus.
pub fn frecency(
    traces: &[BrowsingTrace],
    now_ms: u64,
    config: &FrecencyConfig,
) -> BTreeMap<String, f64> {
    frecency_by(traces, now_ms, config, |event| event.to.url.clone())
}

/// Frecency per `key(event)` — the seam W6d's page fingerprint arrives
/// through. Every visited key appears, including those scoring zero: absence
/// from the table would say "never seen", which is a different fact.
pub fn frecency_by<K, F>(
    traces: &[BrowsingTrace],
    now_ms: u64,
    config: &FrecencyConfig,
    key: F,
) -> BTreeMap<K, f64>
where
    K: Ord,
    F: Fn(&TraceEvent) -> K,
{
    let mut scores = BTreeMap::new();
    for event in traces.iter().flat_map(|trace| trace.events.iter()) {
        *scores.entry(key(event)).or_insert(0.0) += config.visit_score(event, now_ms);
    }
    scores
}

/// A score table as a ranking, best first. Ties break by key, so the order is
/// stable across runs and safe to hand to rank-based fusion.
pub fn ranked<K: Ord + Clone>(scores: &BTreeMap<K, f64>) -> Vec<K> {
    let mut by_score: Vec<(&K, f64)> = scores.iter().map(|(key, s)| (key, *s)).collect();
    by_score.sort_by(|left, right| right.1.total_cmp(&left.1).then_with(|| left.0.cmp(right.0)));
    by_score.into_iter().map(|(key, _)| key.clone()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::browsing::PageRef;

    const NOW: u64 = 100 * DAY_MS;

    fn visit(
        url: &str,
        transition: TraceTransition,
        at_ms: u64,
        dwell_ms: Option<u64>,
    ) -> TraceEvent {
        TraceEvent {
            from: None,
            to: PageRef {
                url: url.to_string(),
                title: None,
            },
            transition,
            at_ms,
            dwell_ms,
            candidates: Vec::new(),
        }
    }

    fn corpus(events: Vec<TraceEvent>) -> Vec<BrowsingTrace> {
        vec![BrowsingTrace::from_events("p", events)]
    }

    /// Same visit, older loses — and one half-life halves it exactly.
    #[test]
    fn recency_decays_on_the_half_life() {
        let config = FrecencyConfig::default();
        let traces = corpus(vec![
            visit(
                "https://fresh.example/",
                TraceTransition::LinkClick,
                NOW,
                None,
            ),
            visit(
                "https://stale.example/",
                TraceTransition::LinkClick,
                NOW - config.half_life_ms,
                None,
            ),
        ]);
        let scores = frecency(&traces, NOW, &config);
        let fresh = scores["https://fresh.example/"];
        let stale = scores["https://stale.example/"];
        assert!(fresh > stale, "the older visit of the same kind loses");
        assert!(
            (stale - fresh / 2.0).abs() < 1e-9,
            "one half-life halves it"
        );
        assert_eq!(
            ranked(&scores),
            vec![
                "https://fresh.example/".to_string(),
                "https://stale.example/".to_string()
            ]
        );
    }

    /// Typed above link click above back/forward above redirect/reload — the
    /// ordering the whole ranking rests on, at one instant so only the
    /// transition varies.
    #[test]
    fn transition_weights_order_the_kinds() {
        let kinds = [
            TraceTransition::UrlTyped,
            TraceTransition::LinkClick,
            TraceTransition::Imported,
            TraceTransition::Back,
            TraceTransition::Redirect,
        ];
        let traces = corpus(
            kinds
                .iter()
                .map(|kind| visit(&format!("https://{kind:?}.example/"), *kind, NOW, None))
                .collect(),
        );
        let scores = frecency(&traces, NOW, &FrecencyConfig::default());
        let order: Vec<String> = kinds
            .iter()
            .map(|kind| format!("https://{kind:?}.example/"))
            .collect();
        assert_eq!(ranked(&scores), order);
    }

    /// A redirect and a reload are near zero next to one link click.
    #[test]
    fn redirects_and_reloads_contribute_near_zero() {
        let config = FrecencyConfig::default();
        let traces = corpus(vec![
            visit(
                "https://noise.example/",
                TraceTransition::Redirect,
                NOW,
                None,
            ),
            visit("https://noise.example/", TraceTransition::Reload, NOW, None),
            visit(
                "https://real.example/",
                TraceTransition::LinkClick,
                NOW,
                None,
            ),
        ]);
        let scores = frecency(&traces, NOW, &config);
        let noise = scores["https://noise.example/"];
        assert!(
            noise < config.weights.link_click * 0.01,
            "two zero-weight visits stay under a hundredth of one link click, got {noise}"
        );
        assert_eq!(ranked(&scores)[0], "https://real.example/");
    }

    /// Dwell past the threshold promotes an otherwise identical visit.
    #[test]
    fn dwell_past_the_threshold_promotes() {
        let config = FrecencyConfig::default();
        let traces = corpus(vec![
            visit(
                "https://read.example/",
                TraceTransition::LinkClick,
                NOW,
                Some(config.dwell_threshold_ms),
            ),
            visit(
                "https://bounced.example/",
                TraceTransition::LinkClick,
                NOW,
                Some(config.dwell_threshold_ms - 1),
            ),
        ]);
        let scores = frecency(&traces, NOW, &config);
        assert_eq!(ranked(&scores)[0], "https://read.example/");
        assert!(
            (scores["https://read.example/"]
                - scores["https://bounced.example/"] * (1.0 + config.dwell_bonus))
                .abs()
                < 1e-9,
            "the bonus is exactly the configured one"
        );
        // Turning the bonus off makes them tie again — nothing else moved.
        let flat = FrecencyConfig {
            dwell_bonus: 0.0,
            ..config
        };
        let flat_scores = frecency(&traces, NOW, &flat);
        assert_eq!(
            flat_scores["https://read.example/"],
            flat_scores["https://bounced.example/"]
        );
    }
}
