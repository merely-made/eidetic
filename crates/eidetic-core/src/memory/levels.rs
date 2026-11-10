// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The memory levels over any node record: short-term working memory and
//! promoted long-term memory, and the eviction policy that decides which
//! short-term records are stale.
//!
//! The model (Alembic plan, decision #2, confirmed with Mark): **a tag
//! promotes a node to long-term**, retained and never evicted. Everything else
//! is short-term working memory, subject to the eviction policy. Codicils are
//! the third level and are not nodes, so they are out of scope here.
//!
//! Split from pandect's `memory_levels` (engine split D45), which now reads a
//! graph's nodes into [`MemoryRecord`]s.

use std::collections::HashMap;

/// What the memory program needs to know about one node.
pub trait MemoryRecord {
    /// The record's stable id: the key the host's last-visit times use.
    fn record_id(&self) -> String;
    /// Whether the record was promoted to long-term (a tag, today).
    fn promoted(&self) -> bool;
    /// The app launch the record was last visited in; `0` when never stamped.
    fn last_session_visited(&self) -> u64;
}

/// A node's memory level.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MemoryLevel {
    /// Working memory: auto-captured from browsing, evictable by default.
    ShortTerm,
    /// Promoted memory: kept across sessions because the user tagged it.
    /// Never evicted by any policy.
    LongTerm,
}

/// The memory level of `record`.
pub fn level_of<R: MemoryRecord + ?Sized>(record: &R) -> MemoryLevel {
    if record.promoted() {
        MemoryLevel::LongTerm
    } else {
        MemoryLevel::ShortTerm
    }
}

/// The eviction policy for short-term memory. User-overridable and shown in the
/// Alembic pane (never a silent default). Long-term (promoted) nodes are never
/// evicted by any policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum EvictionPolicy {
    /// Never evict short-term memory (the explicit keep-everything choice).
    KeepForever,
    /// Evict short-term nodes whose last visit is older than `days` days.
    KeepDays(u32),
    /// Evict short-term nodes not visited in the last `sessions` app launches.
    /// A node never stamped (`last_session_visited == 0`) is left alone,
    /// undated, like an unset last-visit time.
    KeepSessions(u32),
}

impl Default for EvictionPolicy {
    /// A visible, conservative default: keep a month of working memory.
    fn default() -> Self {
        EvictionPolicy::KeepDays(30)
    }
}

impl EvictionPolicy {
    /// One-line description for the Recent section header (the visible policy).
    pub fn describe(&self) -> String {
        match self {
            EvictionPolicy::KeepForever => "keeping all recent memory".to_string(),
            EvictionPolicy::KeepDays(d) => format!("evicting recent memory after {d} day(s)"),
            EvictionPolicy::KeepSessions(n) => {
                format!("evicting recent memory after {n} session(s)")
            },
        }
    }

    /// The next policy when the user cycles the control in the Recent header: a small
    /// curated ladder `7d -> 30d -> 90d -> forever -> 7d`. An arbitrary `KeepDays(n)`
    /// snaps to the next rung above it. [`KeepSessions`](Self::KeepSessions) is not on
    /// this ladder yet (no UI sets it), so cycling away from it returns to the ladder's
    /// start rather than looping in place. (Editable eviction policy.)
    pub fn cycled(self) -> EvictionPolicy {
        match self {
            EvictionPolicy::KeepDays(d) if d < 30 => EvictionPolicy::KeepDays(30),
            EvictionPolicy::KeepDays(d) if d < 90 => EvictionPolicy::KeepDays(90),
            EvictionPolicy::KeepDays(_) => EvictionPolicy::KeepForever,
            EvictionPolicy::KeepForever => EvictionPolicy::KeepDays(7),
            EvictionPolicy::KeepSessions(_) => EvictionPolicy::KeepDays(7),
        }
    }

    /// Whether `record` is stale under this policy: by time against its entry
    /// in `last_visit_ms` and `now_ms`, or by session against its launch stamp
    /// and `current_session`. A record with no timing on the relevant axis is
    /// never stale: we don't drop what we can't date.
    pub fn is_stale<R: MemoryRecord + ?Sized>(
        &self,
        record: &R,
        last_visit_ms: &HashMap<String, u64>,
        now_ms: u64,
        current_session: u64,
    ) -> bool {
        match self {
            EvictionPolicy::KeepForever => false,
            EvictionPolicy::KeepDays(days) => {
                let cutoff = now_ms.saturating_sub(u64::from(*days) * 86_400_000);
                last_visit_ms
                    .get(&record.record_id())
                    .is_some_and(|&visited| visited < cutoff)
            },
            EvictionPolicy::KeepSessions(sessions) => {
                let last = record.last_session_visited();
                last != 0 && current_session.saturating_sub(last) >= u64::from(*sessions)
            },
        }
    }
}

/// The ids of the short-term records `policy` would evict as of `now_ms` /
/// `current_session`. Promoted records are never returned, nor any record that
/// cannot be dated on the policy's axis. This decides *what*, never performs it.
pub fn evictable_short_term<R: MemoryRecord>(
    records: impl IntoIterator<Item = R>,
    last_visit_ms: &HashMap<String, u64>,
    policy: EvictionPolicy,
    now_ms: u64,
    current_session: u64,
) -> Vec<String> {
    records
        .into_iter()
        .filter(|record| level_of(record) == MemoryLevel::ShortTerm)
        .filter(|record| policy.is_stale(record, last_visit_ms, now_ms, current_session))
        .map(|record| record.record_id())
        .collect()
}

/// A census of memory levels, for the Alembic pane to show real counts rather
/// than a grounded guess.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct MemoryCensus {
    /// Short-term (working) nodes.
    pub short_term: usize,
    /// Long-term (promoted) nodes.
    pub long_term: usize,
    /// How many of the short-term nodes the current policy would evict now.
    pub evictable: usize,
}

/// Count the memory levels of `records` and how many `policy` would evict now.
pub fn census<R: MemoryRecord>(
    records: impl IntoIterator<Item = R>,
    last_visit_ms: &HashMap<String, u64>,
    policy: EvictionPolicy,
    now_ms: u64,
    current_session: u64,
) -> MemoryCensus {
    let mut census = MemoryCensus::default();
    for record in records {
        match level_of(&record) {
            MemoryLevel::LongTerm => census.long_term += 1,
            MemoryLevel::ShortTerm => {
                census.short_term += 1;
                if policy.is_stale(&record, last_visit_ms, now_ms, current_session) {
                    census.evictable += 1;
                }
            },
        }
    }
    census
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY_MS: u64 = 86_400_000;

    struct Node {
        id: &'static str,
        promoted: bool,
        session: u64,
    }

    impl MemoryRecord for Node {
        fn record_id(&self) -> String {
            self.id.to_owned()
        }
        fn promoted(&self) -> bool {
            self.promoted
        }
        fn last_session_visited(&self) -> u64 {
            self.session
        }
    }

    fn node(id: &'static str, promoted: bool, session: u64) -> Node {
        Node {
            id,
            promoted,
            session,
        }
    }

    #[test]
    fn keep_days_evicts_only_dated_stale_short_term_records() {
        let now = 100 * DAY_MS;
        let times = HashMap::from([
            ("stale".to_string(), now - 40 * DAY_MS),
            ("fresh".to_string(), now - 5 * DAY_MS),
            ("kept".to_string(), now - 90 * DAY_MS),
        ]);
        let records = [
            node("stale", false, 0),
            node("fresh", false, 0),
            node("undated", false, 0),
            node("kept", true, 0),
        ];
        let out = evictable_short_term(records, &times, EvictionPolicy::KeepDays(30), now, 0);
        assert_eq!(out, ["stale"]);
    }

    #[test]
    fn keep_sessions_reads_the_launch_stamp_and_leaves_unstamped_records() {
        let records = [
            node("stale", false, 2),
            node("fresh", false, 9),
            node("unstamped", false, 0),
            node("kept", true, 1),
        ];
        let policy = EvictionPolicy::KeepSessions(3);
        assert_eq!(
            evictable_short_term(records, &HashMap::new(), policy, 0, 10),
            ["stale"]
        );
    }

    #[test]
    fn census_counts_each_level_once() {
        let now = 100 * DAY_MS;
        let times = HashMap::from([
            ("s1".to_string(), now - 60 * DAY_MS),
            ("s2".to_string(), now - 2 * DAY_MS),
        ]);
        let records = [node("s1", false, 0), node("s2", false, 0), node("k", true, 0)];
        let census = census(records, &times, EvictionPolicy::KeepDays(30), now, 0);
        assert_eq!(
            census,
            MemoryCensus {
                short_term: 2,
                long_term: 1,
                evictable: 1
            }
        );
    }

    #[test]
    fn default_policy_is_a_visible_thirty_days() {
        assert_eq!(EvictionPolicy::default(), EvictionPolicy::KeepDays(30));
        assert!(EvictionPolicy::default().describe().contains("30"));
    }

    #[test]
    fn eviction_policy_cycles_through_the_ladder() {
        use EvictionPolicy::*;
        assert_eq!(KeepDays(7).cycled(), KeepDays(30));
        assert_eq!(KeepDays(30).cycled(), KeepDays(90));
        assert_eq!(KeepDays(90).cycled(), KeepForever);
        assert_eq!(KeepForever.cycled(), KeepDays(7));
        // The default (30d) advances to 90d.
        assert_eq!(EvictionPolicy::default().cycled(), KeepDays(90));
    }
}
