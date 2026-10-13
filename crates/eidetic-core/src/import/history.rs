// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Imported history visits, lowered onto eidetic's browsing corpus.
//!
//! The review brief's §3.2 finding was that nothing mapped
//! [`HistoryTransitionKind`] onto [`TraceTransition`], so every imported visit
//! could only be `Imported` — one weight for a whole browsing life. This module
//! is that mapping plus the fold that turns a visit list into stored traces.
//!
//! The mapping is chosen against `eidetic::browsing::frecency`'s weight table,
//! not by name similarity: the point of importing a transition is that the
//! behavioural lane can tell a typed address from a subframe load.

use crate::browsing::{BrowsingTrace, PageRef, TraceEvent, TraceTransition};

use super::{HistoryTransitionKind, ImportedHistoryVisitItem};

/// Events per stored trace segment. Matches turnstone's own `SEGMENT_SIZE`, so
/// an imported corpus segments the way a captured one does and the two are
/// comparable receipt to receipt.
pub const HISTORY_SEGMENT_SIZE: usize = 32;

/// The transition a visit kind means to the behavioural ranking.
///
/// Weights (frecency defaults) drive the choices the names do not: a Firefox
/// bookmark visit scores 75/100, which is exactly `Imported`'s 0.75; a subframe
/// load and a download are the browser's fetches rather than the user's
/// choice, so they take `Restore`'s zero; and `Generated` is an address-bar
/// suggestion the user picked, which is the case `UrlTyped`'s 20 exists for.
/// `Other` is an open tag, so only the download spelling this crate's own
/// exporter emits is recognised — anything else stays `Unknown` (1.0).
impl From<&HistoryTransitionKind> for TraceTransition {
    fn from(kind: &HistoryTransitionKind) -> Self {
        match kind {
            HistoryTransitionKind::Link => Self::LinkClick,
            HistoryTransitionKind::Typed | HistoryTransitionKind::Generated => Self::UrlTyped,
            HistoryTransitionKind::AutoBookmark => Self::Imported,
            HistoryTransitionKind::AutoSubframe => Self::Restore,
            HistoryTransitionKind::Reload => Self::Reload,
            HistoryTransitionKind::Redirect => Self::Redirect,
            HistoryTransitionKind::Other(tag) if tag.eq_ignore_ascii_case("download") => {
                Self::Restore
            },
            HistoryTransitionKind::Other(_) => Self::Unknown,
        }
    }
}

/// One visit as a traversal. A visit with no transition is `Imported` — the
/// pre-mapping default, and the honest answer when the source did not say.
pub fn history_event(item: &ImportedHistoryVisitItem) -> TraceEvent {
    let title = item
        .page
        .normalized_title
        .clone()
        .or_else(|| item.page.raw_title.clone());
    TraceEvent {
        from: item.referring_url.as_ref().map(|url| PageRef {
            url: url.clone(),
            title: None,
        }),
        to: PageRef {
            url: item.page.canonical_url.clone(),
            title,
        },
        transition: item
            .transition
            .as_ref()
            .map_or(TraceTransition::Imported, TraceTransition::from),
        at_ms: item.visited_at_unix_micros.map_or_else(
            || (item.visited_at_unix_secs.max(0) as u64).saturating_mul(1_000),
            |micros| micros.max(0) as u64 / 1_000,
        ),
        dwell_ms: item.view_time_ms,
        candidates: Vec::new(),
    }
}

/// Imported visits as one owner's trace corpus, at the default segment size.
pub fn history_to_traces(items: &[ImportedHistoryVisitItem], owner: &str) -> Vec<BrowsingTrace> {
    history_to_traces_with(items, owner, HISTORY_SEGMENT_SIZE)
}

/// [`history_to_traces`] under a caller's segment size.
///
/// Preserve original microsecond ordering when supplied; legacy second stamps
/// keep source order for ties. Neither ordering creates navigation parentage.
pub fn history_to_traces_with(
    items: &[ImportedHistoryVisitItem],
    owner: &str,
    segment_size: usize,
) -> Vec<BrowsingTrace> {
    let mut events: Vec<(i64, TraceEvent)> = items
        .iter()
        .map(|item| {
            (
                item.visited_at_unix_micros
                    .unwrap_or_else(|| item.visited_at_unix_secs.saturating_mul(1_000_000)),
                history_event(item),
            )
        })
        .collect();
    events.sort_by_key(|(at, _)| *at);
    events
        .chunks(segment_size.max(1))
        .map(|chunk| {
            BrowsingTrace::from_events(
                owner,
                chunk.iter().map(|(_, event)| event.clone()).collect(),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::ImportedPageSeed;

    fn visit(
        url: &str,
        at: i64,
        transition: HistoryTransitionKind,
        referrer: Option<&str>,
        view_time_ms: Option<u64>,
    ) -> ImportedHistoryVisitItem {
        ImportedHistoryVisitItem {
            page: ImportedPageSeed {
                canonical_url: url.to_string(),
                normalized_title: Some("a title".to_string()),
                raw_url: None,
                raw_title: None,
                favicon_url: None,
            },
            visit_id: None,
            source_scope: None,
            parent: Default::default(),
            visited_at_unix_micros: None,
            visited_at_unix_secs: at,
            visit_count_hint: Some(3),
            transition: Some(transition),
            referring_url: referrer.map(str::to_string),
            session_context: None,
            view_time_ms,
        }
    }

    #[test]
    fn transitions_map_onto_the_frecency_weights_they_were_chosen_for() {
        use crate::browsing::frecency::TransitionWeights;
        let weights = TransitionWeights::default();
        let weight_of = |kind: HistoryTransitionKind| weights.of(TraceTransition::from(&kind));
        assert_eq!(weight_of(HistoryTransitionKind::Typed), 20.0);
        assert_eq!(weight_of(HistoryTransitionKind::Generated), 20.0);
        assert_eq!(weight_of(HistoryTransitionKind::Link), 1.0);
        assert_eq!(weight_of(HistoryTransitionKind::AutoBookmark), 0.75);
        assert_eq!(weight_of(HistoryTransitionKind::AutoSubframe), 0.0);
        assert_eq!(weight_of(HistoryTransitionKind::Reload), 0.0);
        assert_eq!(weight_of(HistoryTransitionKind::Redirect), 0.0);
        assert_eq!(
            weight_of(HistoryTransitionKind::Other("download".to_string())),
            0.0
        );
        // A tag this crate does not know is not silently zeroed.
        assert_eq!(
            weight_of(HistoryTransitionKind::Other("mystery".to_string())),
            1.0
        );
    }

    #[test]
    fn visits_become_ordered_events_with_referrers_and_dwell() {
        let items = vec![
            visit(
                "https://example.test/b",
                200,
                HistoryTransitionKind::Link,
                Some("https://example.test/a"),
                Some(45_000),
            ),
            visit(
                "https://example.test/a",
                100,
                HistoryTransitionKind::Typed,
                None,
                None,
            ),
        ];
        let traces = history_to_traces(&items, "persona");
        assert_eq!(traces.len(), 1);
        let trace = &traces[0];
        assert_eq!(trace.owner, "persona");
        assert_eq!(trace.started_at_ms, 100_000);
        assert_eq!(trace.ended_at_ms, 200_000);
        assert_eq!(trace.events[0].transition, TraceTransition::UrlTyped);
        assert!(trace.events[0].from.is_none());
        assert_eq!(trace.events[0].dwell_ms, None);
        assert_eq!(
            trace.events[1].from.as_ref().map(|page| page.url.as_str()),
            Some("https://example.test/a")
        );
        assert_eq!(trace.events[1].dwell_ms, Some(45_000));
        assert!(trace.events[1].candidates.is_empty());
    }

    #[test]
    fn segments_chunk_the_way_a_captured_corpus_does() {
        let items: Vec<_> = (0..70)
            .map(|index| {
                visit(
                    "https://example.test/c",
                    i64::from(index),
                    HistoryTransitionKind::Link,
                    None,
                    None,
                )
            })
            .collect();
        let traces = history_to_traces(&items, "");
        assert_eq!(traces.len(), 3);
        assert_eq!(traces[0].events.len(), HISTORY_SEGMENT_SIZE);
        assert_eq!(traces[2].events.len(), 70 - 2 * HISTORY_SEGMENT_SIZE);
        assert_eq!(traces[2].ended_at_ms, 69_000);
    }
}
