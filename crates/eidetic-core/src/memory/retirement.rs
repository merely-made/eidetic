// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0


//! Retirement: purging deleted-node tombstones whose time in the recycle bin
//! has passed the retention window, as a pure proposal and its apply (engine
//! split D45).

use crate::{DeletedNode, Result, Store, purge_deleted};

/// A proposal to retire (permanently forget) staged deleted-node tombstones
/// whose time in the recycle bin has passed the retention window — the bin's
/// steady-heat auto-empty, the third of Athanor's passes.
///
/// Athanor emits it; the host applies it (the R0 propose/apply split, so a
/// "what will be forgotten" preview is possible before the oven runs). Unlike
/// the on-command "empty the bin" (which clears everything), retirement drops
/// only the aged-out subset.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RetirementProposal {
    /// The node ids whose tombstone(s) are old enough to purge, de-duplicated
    /// (a node deleted twice has two tombstones but one id).
    pub node_ids: Vec<String>,
}

impl RetirementProposal {
    pub fn is_empty(&self) -> bool {
        self.node_ids.is_empty()
    }

    pub fn len(&self) -> usize {
        self.node_ids.len()
    }
}

/// Propose retirement: the ids of tombstones deleted at or before
/// `now_ms - keep_ms` (the retention window). Pure (R0): it reads the listed
/// records ([`crate::list_deleted`]'s output) and mutates nothing.
pub fn propose_retirement(
    deleted: &[DeletedNode],
    keep_ms: u64,
    now_ms: u64,
) -> RetirementProposal {
    let cutoff = now_ms.saturating_sub(keep_ms);
    let mut node_ids: Vec<String> = deleted
        .iter()
        .filter(|d| d.deleted_at_ms <= cutoff)
        .map(|d| d.node_id.clone())
        .collect();
    node_ids.sort();
    node_ids.dedup();
    RetirementProposal { node_ids }
}

/// Apply a retirement proposal: permanently forget each proposed node
/// ([`crate::purge_deleted`], which drops all of a node's tombstones — R0:
/// an accepted proposal the host hands back). Returns how many tombstones were
/// removed, genuine feedback (a node with two tombstones removes both).
pub async fn apply_retirement(
    store: &mut dyn Store,
    proposal: &RetirementProposal,
) -> Result<usize> {
    let mut retired = 0;
    for node_id in &proposal.node_ids {
        retired += purge_deleted(store, node_id).await?;
    }
    Ok(retired)
}

#[cfg(test)]
mod tests {
    use super::*;
    use muniment::MemoryBackend as MemStore;

    const DAY_MS: u64 = 86_400_000;

    use crate::{list_deleted, record_deleted};

    fn deleted(node_id: &str, at_ms: u64) -> DeletedNode {
        DeletedNode {
            node_id: node_id.to_string(),
            url: format!("https://{node_id}.test"),
            title: None,
            tags: Vec::new(),
            graph_id: None,
            deleted_at_ms: at_ms,
            nested: None,
            facets: None,
        }
    }

    #[test]
    fn propose_retirement_names_only_the_aged_out_tombstones() {
        let now = 100 * DAY_MS;
        let records = [
            deleted("old", now - 40 * DAY_MS),  // 40d old -> retire
            deleted("fresh", now - 5 * DAY_MS), // 5d old -> keep
            deleted("edge", now - 30 * DAY_MS), // exactly the window -> retire (<=)
        ];
        let proposal = propose_retirement(&records, 30 * DAY_MS, now);
        assert_eq!(
            proposal.node_ids,
            vec!["edge".to_string(), "old".to_string()]
        );
        // A generous window keeps everything; a zero window keeps only the future.
        assert!(propose_retirement(&records, 365 * DAY_MS, now).is_empty());
    }

    #[test]
    fn apply_retirement_purges_the_aged_out_and_lists_the_survivors() {
        pollster::block_on(async {
            let mut store = MemStore::default();
            let now = 100 * DAY_MS;
            record_deleted(&mut store, &deleted("old", now - 40 * DAY_MS))
                .await
                .unwrap();
            record_deleted(&mut store, &deleted("fresh", now - 5 * DAY_MS))
                .await
                .unwrap();

            let listed = list_deleted(&mut store).await.unwrap();
            let proposal = propose_retirement(&listed, 30 * DAY_MS, now);
            let retired = apply_retirement(&mut store, &proposal).await.unwrap();
            assert_eq!(retired, 1, "only the aged-out tombstone purged");

            let survivors = list_deleted(&mut store).await.unwrap();
            assert_eq!(survivors.len(), 1);
            assert_eq!(survivors[0].node_id, "fresh", "the fresh one stays");
        });
    }
}
