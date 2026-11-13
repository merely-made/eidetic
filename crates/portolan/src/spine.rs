// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The edit spine: an event-sourced graph over a muniment journal.
//!
//! [`GraphLog`] is the write side of the graph. Every mutation travels one
//! path: build an attributed [`Batch`], apply it to the materialized graph,
//! append it to a [`Journal`]. The graph is the replay of the log, and because
//! live editing and replay share the same apply, they cannot diverge. The
//! convenience mutators here are single-spec commits at the current revision
//! (the trusted-UI path: attributed, no optimistic retry); a gate commits
//! multi-spec petitions through
//! [`commit_batch`](GraphLog::commit_batch) in [`commit`](crate::commit).
//!
//! Loading has two paths, and they agree:
//! - [`load_full`](GraphLog::load_full): read the log, replay every batch.
//! - [`load_checkpointed`](GraphLog::load_checkpointed): read a snapshot, then
//!   replay only the batches after it.
//!
//! Both migrate pre-gate logs (bare [`GraphEdit`] entries) on load, wrapping
//! each edit as a single-edit batch authored
//! [`Author::pre_gate`](crate::commit::Author::pre_gate).
//!
//! A snapshot is a compacted materialization (the nodes and edges as they
//! stand, plus the edge-id bookkeeping and the log length it reflects), stored
//! in a muniment slot. It is an optimization; the log is the authority, so a
//! snapshot names the log prefix it summarizes by the muniment digest of that
//! prefix ([`Journal::prefix_digest`]). A checkpointed load trusts a snapshot
//! only when the journal beside it still begins with that prefix. A snapshot
//! that cannot be shown to (it names no prefix, names another journal's, or runs
//! past the end of this one) is a cache the journal outranks: the log is
//! replayed in full and the next snapshot names its prefix.

use std::collections::{BTreeMap, HashMap};

use muniment::{Backend, Codec, SlotStore, StoreError};
use muniment::{Journal, LogId, Provenance, Seq};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;

use crate::assertion::{AssertionBasis, AssertionRecord, AssertionSpan, AssertionWithdrawal};
use crate::caps::Identified;
use crate::commit::{Author, Batch, EditSpec};
use crate::edit::{DerivationRecord, EdgeId, GraphEdit, WriterId};
use crate::facet::{AcceptAll, FacetId, FacetStore};
use crate::graph::{EdgeKey, Graph, NodeKey};

/// An event-sourced graph: a materialized [`Graph`] plus the [`Journal`] of
/// attributed batches that produced it.
pub struct GraphLog<N: Identified, E> {
    pub(crate) graph: Graph<N, E>,
    pub(crate) log: Journal<Batch<N, E>>,
    pub(crate) by_edge_id: HashMap<EdgeId, EdgeKey>,
    pub(crate) by_edge_key: HashMap<EdgeKey, EdgeId>,
    pub(crate) derivations: HashMap<N::Id, Vec<DerivationRecord<N::Id>>>,
    pub(crate) facets: FacetStore<N::Id>,
    /// Authoritative assertion history keyed by the stable edge id. `graph`
    /// and edge indexes remain the held-only projection.
    pub(crate) assertions: BTreeMap<EdgeId, AssertionRecord<N::Id, E>>,
    /// This replica's identity, the writer half of every id it mints.
    pub(crate) writer: WriterId,
    /// This replica's own counter. Advanced only past ids **this** writer
    /// minted, so replaying another replica's edits never consumes our range.
    pub(crate) next_edge: u64,
}

/// A compacted materialization of a [`GraphLog`], for checkpointed load.
#[derive(Serialize, Deserialize)]
#[serde(bound(
    serialize = "N: Identified + Serialize, N::Id: Serialize, E: Serialize",
    deserialize = "N: Identified + Deserialize<'de>, N::Id: Deserialize<'de>, E: Deserialize<'de>"
))]
struct Snapshot<N: Identified, E> {
    nodes: Vec<N>,
    edges: Vec<(EdgeId, N::Id, N::Id, E)>,
    derivations: Vec<(N::Id, DerivationRecord<N::Id>)>,
    /// Facets are part of the compacted graph materialization.
    #[serde(default)]
    facets: FacetStore<N::Id>,
    /// Absent in 0.1.x snapshots, which were single-writer.
    #[serde(default)]
    writer: WriterId,
    next_edge: u64,
    /// The number of log entries baked into this snapshot; replay resumes here.
    at_seq: u64,
    /// `None` means a legacy snapshot with no assertion ledger. It must be
    /// reconstructed from the retained journal before use. Kept last so the
    /// existing snapshot field order remains stable for self-describing stores.
    #[serde(default)]
    assertions: Option<Vec<AssertionRecord<N::Id, E>>>,
    /// The hex muniment digest of the first `at_seq` log entries. Absent in
    /// snapshots written before it existed, which name no journal and so are not
    /// trusted to summarize this one.
    #[serde(default)]
    journal_digest: Option<String>,
}

impl<N: Identified, E> Default for GraphLog<N, E> {
    fn default() -> Self {
        Self {
            graph: Graph::new(),
            log: Journal::new(),
            by_edge_id: HashMap::new(),
            by_edge_key: HashMap::new(),
            derivations: HashMap::new(),
            facets: FacetStore::new(),
            assertions: BTreeMap::new(),
            writer: WriterId::LOCAL,
            next_edge: 0,
        }
    }
}

impl<N: Identified, E> GraphLog<N, E> {
    /// A fresh, empty log-backed graph.
    pub fn new() -> Self {
        Self::default()
    }

    /// A fresh, empty log-backed graph with a stable log identity, so it can later
    /// be forked with provenance pointing back at it.
    pub fn with_id(id: LogId) -> Self {
        let mut this = Self::new();
        this.log = Journal::with_id(id);
        this
    }

    /// Declare which replica this is, so the ids it mints cannot collide with
    /// another replica's.
    ///
    /// Required for any container with more than one writer; a graph left at
    /// [`WriterId::LOCAL`] is asserting it is the only one. Bind `writer` to
    /// the replication identity, not to an [`Author`] label.
    pub fn for_writer(mut self, writer: WriterId) -> Self {
        self.writer = writer;
        // `for_writer` is also the recovery seam after a full replay. Rebuild
        // the minting frontier from the journal rather than from live edges:
        // a previously disconnected edge still consumed its counter.
        self.next_edge = self
            .log
            .entries()
            .iter()
            .flat_map(|batch| batch.edits.iter())
            .filter_map(|edit| match edit {
                GraphEdit::Connect { id, .. } | GraphEdit::Assert { id, .. }
                    if id.writer == writer =>
                {
                    Some(id.counter.saturating_add(1))
                }
                _ => None,
            })
            .max()
            .unwrap_or(0);
        self
    }

    /// This replica's identity.
    pub fn writer(&self) -> WriterId {
        self.writer
    }

    /// The materialized graph (read-only; mutate through the edit methods).
    pub fn graph(&self) -> &Graph<N, E> {
        &self.graph
    }

    /// The edit log.
    pub fn log(&self) -> &Journal<Batch<N, E>> {
        &self.log
    }

    /// The current revision: the number of committed batches. What
    /// [`commit_batch`](GraphLog::commit_batch) compares `expected` against.
    pub fn revision(&self) -> u64 {
        self.log.len() as u64
    }

    /// The current graph key for a live edge id, if the edge is present.
    pub fn edge_key(&self, id: EdgeId) -> Option<EdgeKey> {
        self.by_edge_id.get(&id).copied()
    }

    /// This graph's log identity, if any.
    pub fn id(&self) -> Option<&LogId> {
        self.log.id()
    }

    /// Where this graph forked from, if it is a fork.
    pub fn provenance(&self) -> Option<&Provenance> {
        self.log.provenance()
    }

    /// The derivation records for a node: the other-graph nodes it was derived
    /// from. Empty for a natively-minted node.
    pub fn derivations(&self, node: &N::Id) -> &[DerivationRecord<N::Id>] {
        self.derivations.get(node).map(Vec::as_slice).unwrap_or(&[])
    }

    /// Independently mergeable node facets materialized from the journal.
    pub fn facets(&self) -> &FacetStore<N::Id> {
        &self.facets
    }

    /// The retained assertion record for a stable edge id, whether held or
    /// withdrawn.
    pub fn assertion(&self, id: EdgeId) -> Option<&AssertionRecord<N::Id, E>> {
        self.assertions.get(&id)
    }

    /// All assertion records in stable id order, including withdrawals.
    pub fn assertions(&self) -> impl Iterator<Item = &AssertionRecord<N::Id, E>> {
        self.assertions.values()
    }
}

// Persistence through muniment.
impl<N, E> GraphLog<N, E>
where
    N: Identified + Clone + Serialize + DeserializeOwned,
    N::Id: Serialize + DeserializeOwned,
    E: Clone + Serialize + DeserializeOwned,
{
    /// Load the log at `key`, migrating a pre-gate edit log (bare `GraphEdit`
    /// entries) into single-edit batches if that is what the slot holds.
    async fn load_log<B: Backend, C: Codec>(
        slots: &SlotStore<B, C>,
        key: &str,
    ) -> Result<Journal<Batch<N, E>>, StoreError> {
        match Journal::load(slots, key).await {
            Ok(log) => Ok(log),
            Err(StoreError::Codec(_)) => {
                let legacy: Journal<GraphEdit<N, E>> = Journal::load(slots, key).await?;
                Ok(crate::commit::migrate_pre_gate(legacy))
            }
            Err(other) => Err(other),
        }
    }

    /// Persist the edit log to a muniment slot.
    pub async fn save_log<B: Backend, C: Codec>(
        &self,
        slots: &SlotStore<B, C>,
        key: &str,
    ) -> Result<(), StoreError> {
        self.log.save(slots, key).await
    }

    /// Write a snapshot of the current state to a muniment slot.
    pub async fn snapshot<B: Backend, C: Codec>(
        &self,
        slots: &SlotStore<B, C>,
        key: &str,
    ) -> Result<(), StoreError> {
        let nodes: Vec<N> = self.graph.nodes().map(|(_, node)| node.clone()).collect();
        let keys: Vec<NodeKey> = self.graph.nodes().map(|(key, _)| key).collect();
        let mut edges = Vec::new();
        for key in keys {
            let from_id = self.graph.node(key).expect("node present").id().clone();
            for (edge_key, target, edge) in self.graph.out_edges(key) {
                if let Some(&edge_id) = self.by_edge_key.get(&edge_key) {
                    let to_id = self
                        .graph
                        .node(target)
                        .expect("target present")
                        .id()
                        .clone();
                    edges.push((edge_id, from_id.clone(), to_id, edge.clone()));
                }
            }
        }
        let derivations: Vec<(N::Id, DerivationRecord<N::Id>)> = self
            .derivations
            .iter()
            .flat_map(|(node, records)| {
                records
                    .iter()
                    .map(move |record| (node.clone(), record.clone()))
            })
            .collect();
        let snapshot = Snapshot {
            nodes,
            edges,
            derivations,
            facets: self.facets.clone(),
            writer: self.writer,
            next_edge: self.next_edge,
            assertions: Some(self.assertions.values().cloned().collect()),
            at_seq: self.log.len() as u64,
            journal_digest: self
                .log
                .prefix_digest(self.log.len())?
                .map(|digest| digest.to_hex()),
        };
        slots.save(key, &snapshot).await
    }

    /// Load by replaying the whole log from the slot at `log_key`.
    pub async fn load_full<B: Backend, C: Codec>(
        slots: &SlotStore<B, C>,
        log_key: &str,
    ) -> Result<Self, StoreError> {
        let log = Self::load_log(slots, log_key).await?;
        Ok(Self::replay(log))
    }

    /// Load a multi-writer journal for the replica identified by `writer`.
    ///
    /// Unlike [`load_full`](Self::load_full), this restores the writer-scoped
    /// edge counter so the next mint cannot reuse an id from before restart.
    pub async fn load_full_for_writer<B: Backend, C: Codec>(
        slots: &SlotStore<B, C>,
        log_key: &str,
        writer: WriterId,
    ) -> Result<Self, StoreError> {
        let log = Self::load_log(slots, log_key).await?;
        Ok(Self::replay_for_writer(log, writer))
    }

    /// Whether `snapshot` is a checkpoint of this journal: it names the digest of
    /// the first `at_seq` entries, and `log` still begins with them.
    fn summarizes_prefix_of(
        snapshot: &Snapshot<N, E>,
        log: &Journal<Batch<N, E>>,
    ) -> Result<bool, StoreError> {
        let Some(stored) = snapshot.journal_digest.as_deref() else {
            return Ok(false);
        };
        let prefix = usize::try_from(snapshot.at_seq)
            .ok()
            .and_then(|at| log.prefix_digest(at).transpose())
            .transpose()?;
        Ok(prefix.is_some_and(|prefix| prefix.to_hex() == stored))
    }

    /// Load from a snapshot at `snap_key` plus the tail of the log at `log_key`. If
    /// no snapshot is present, this is a full replay. A snapshot is used only if it
    /// names this journal's first `at_seq` entries by digest; one that names none,
    /// names another journal's, or runs past this one is skipped for a full replay.
    pub async fn load_checkpointed<B: Backend, C: Codec>(
        slots: &SlotStore<B, C>,
        log_key: &str,
        snap_key: &str,
    ) -> Result<Self, StoreError> {
        let snapshot: Option<Snapshot<N, E>> = slots.load(snap_key).await?;
        let log = Self::load_log(slots, log_key).await?;
        let (mut this, tail_start) = match snapshot {
            Some(snapshot) if snapshot.assertions.is_none() => {
                // Legacy snapshots had no retained assertion metadata. The full
                // journal is retained by this store, so replay it rather than
                // reconstructing historical lifecycle values from the snapshot.
                if snapshot.at_seq > log.len() as u64 {
                    return Err(StoreError::Codec(format!(
                        "legacy snapshot sequence {} exceeds journal length {}",
                        snapshot.at_seq,
                        log.len()
                    )));
                }
                return Ok(Self::replay_for_writer(log, snapshot.writer));
            }
            Some(snapshot) if !Self::summarizes_prefix_of(&snapshot, &log)? => {
                // A snapshot that cannot be shown to summarize this journal's
                // prefix is a cache the journal outranks, so the log is replayed
                // in full; the next snapshot names its prefix.
                return Ok(Self::replay_for_writer(log, snapshot.writer));
            }
            Some(snapshot) => Self::from_snapshot(snapshot)?,
            None => (Self::new(), 0),
        };
        if tail_start > log.len() as u64 {
            return Err(StoreError::Codec(format!(
                "snapshot sequence {tail_start} exceeds journal length {}",
                log.len()
            )));
        }
        for batch in log.from(Seq(tail_start)) {
            this.apply_batch(batch);
        }
        this.log = log;
        Ok(this)
    }
}

#[cfg(test)]
mod tests;

mod materialize;
