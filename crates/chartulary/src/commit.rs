// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The attributed commit seam: every mutation is a [`Batch`] in the journal.
//!
//! B0.5 of the participant gate + packs plan (mere design_docs, 2026-07-17):
//! the journal's entry type is an attributed batch, committed atomically
//! against an expected revision. A petition from any participant and a keystroke
//! from the trusted UI travel the same path: the UI's convenience mutators on
//! [`GraphLog`] are single-spec commits at the current revision (no conflict
//! possible, no optimistic retry), while a gate calls
//! [`commit_batch`](GraphLog::commit_batch) with the revision it read, and a
//! stale batch is refused wholesale, carrying the current revision to rebase
//! against.
//!
//! Chartulary **attributes**; it does not authenticate. Whether an author may
//! commit what it commits (grants, scopes) is the gate's job, one layer up.
//! App effects (fetch, navigation, windows) cannot roll back, so a consumer
//! enqueues them only after a commit returns, carrying the [`BatchId`];
//! nothing here runs an effect.
//!
//! A batch either applies in full or not at all: the revision is compared and
//! every spec is prechecked against the graph plus the batch's own earlier
//! specs before anything mutates, and the whole batch lands as one journal
//! entry (one slot write on save).

use std::collections::HashSet;

use muniment::{Journal, LogId};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::assertion::AssertionBasis;
use crate::caps::Identified;
use crate::edit::{DerivationRecord, EdgeId, GraphEdit};
use crate::facet::FacetId;
use crate::spine::GraphLog;

/// An opaque author identity for journal attribution: a participant id, a personae
/// fingerprint, `"ui"`, or the migration's `"pre-gate"`. Caller-chosen, like
/// [`LogId`].
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Author(pub String);

impl Author {
    /// Name an author.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// The identity string.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The synthetic author stamped on entries migrated from a pre-gate log.
    pub fn pre_gate() -> Self {
        Self("pre-gate".into())
    }
}

/// A committed batch's identity: its sequence position in the journal. App
/// effects spawned by a petition carry this id.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BatchId(pub u64);

/// One journal entry: an attributed group of edits that applied atomically.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(bound(
    serialize = "N: Identified + Serialize, N::Id: Serialize, E: Serialize",
    deserialize = "N: Identified + Deserialize<'de>, N::Id: Deserialize<'de>, E: Deserialize<'de>"
))]
pub struct Batch<N: Identified, E> {
    /// Who committed it.
    pub author: Author,
    /// The edits, in application order.
    pub edits: Vec<GraphEdit<N, E>>,
}

/// What a petitioner submits: an edit without internal bookkeeping. Edge ids
/// are minted by the commit (and returned in [`Committed::edges`]), so a spec
/// is authorable without reading the log's counters.
#[derive(Clone, Debug)]
pub enum EditSpec<N: Identified, E> {
    /// Insert (or upsert, by identity) a node.
    InsertNode(N),
    /// Remove a node and its incident edges.
    RemoveNode(N::Id),
    /// Connect two present nodes; the commit mints the edge id.
    Connect {
        /// The source node.
        from: N::Id,
        /// The target node.
        to: N::Id,
        /// The edge payload.
        edge: E,
    },
    /// Assert one relation with caller-supplied source metadata and time.
    Assert {
        /// The source node.
        from: N::Id,
        /// The target node.
        to: N::Id,
        /// The edge payload.
        edge: E,
        /// Source-provided asserter; distinct from the batch author.
        asserter: Option<Author>,
        /// Caller-supplied admission time in Unix milliseconds, if known.
        at_ms: Option<u64>,
        /// Basis of the assertion.
        basis: AssertionBasis,
    },
    /// Confirm an assertion that is still held.
    ConfirmAssertion {
        /// The stable assertion id.
        id: EdgeId,
        /// Caller-supplied confirmation time, if known.
        at_ms: Option<u64>,
    },
    /// Withdraw an assertion that is still held. This is terminal.
    WithdrawAssertion {
        /// The stable assertion id.
        id: EdgeId,
        /// Caller-supplied withdrawal time, if known.
        at_ms: Option<u64>,
    },
    /// Retract a pre-existing edge by its stable id.
    Disconnect(EdgeId),
    /// Record that a node derives from a node in another graph.
    Derive {
        /// The deriving node.
        node: N::Id,
        /// Where it came from.
        from: DerivationRecord<N::Id>,
    },
    /// Set one independently mergeable facet on a live node.
    SetFacet {
        /// The node carrying the facet.
        node: N::Id,
        /// The stable, namespaced facet identity.
        facet: FacetId,
        /// The host-validated facet value.
        value: Value,
    },
    /// Remove one independently mergeable facet from a live node.
    RemoveFacet {
        /// The node carrying the facet.
        node: N::Id,
        /// The stable, namespaced facet identity.
        facet: FacetId,
    },
}

/// Why a commit was refused. Nothing applied.
#[derive(Clone, Debug, PartialEq)]
pub enum CommitError<Id> {
    /// `expected` did not match the current revision; carries the current
    /// revision so the petitioner can rebase and resubmit.
    RevisionConflict {
        /// The revision the graph is actually at.
        current: u64,
    },
    /// A spec referenced a node that is neither live nor inserted earlier in
    /// the batch.
    UnknownNode(Id),
    /// A `Disconnect` referenced an edge that is not live (or was already
    /// disconnected earlier in the batch).
    UnknownEdge(EdgeId),
    /// An explicit lifecycle operation referenced no known assertion.
    UnknownAssertion(EdgeId),
    /// An explicit lifecycle operation attempted to revive a withdrawn assertion.
    AssertionAlreadyWithdrawn(EdgeId),
    /// A node tried to bear the graph it lives in (see
    /// [`commit_bearing_batch`](GraphLog::commit_bearing_batch)).
    SelfBearing(LogId),
}

/// A successful commit's receipt: the batch id plus the edge ids minted for
/// its `Connect` and `Assert` specs, in spec order.
#[derive(Clone, Debug, PartialEq)]
pub struct Committed {
    /// The journal position the batch landed at.
    pub batch: BatchId,
    /// Minted edge ids, one per `Connect` or `Assert` spec, in order.
    pub edges: Vec<EdgeId>,
}

impl<N, E> GraphLog<N, E>
where
    N: Identified + Clone,
    E: Clone,
{
    /// Commit an attributed batch against an expected revision. The whole
    /// batch applies atomically or not at all: the revision is compared and
    /// every spec prechecked (against the live graph plus the batch's own
    /// earlier specs) before anything mutates.
    pub fn commit_batch(
        &mut self,
        author: Author,
        expected: u64,
        specs: Vec<EditSpec<N, E>>,
    ) -> Result<Committed, CommitError<N::Id>> {
        let current = self.revision();
        if expected != current {
            return Err(CommitError::RevisionConflict { current });
        }

        // Precheck: batch-local view of node presence and edge liveness.
        let mut added: HashSet<N::Id> = HashSet::new();
        let mut removed: HashSet<N::Id> = HashSet::new();
        let mut dropped_edges: HashSet<EdgeId> = HashSet::new();
        let mut withdrawn_assertions: HashSet<EdgeId> = HashSet::new();
        let present =
            |id: &N::Id, added: &HashSet<N::Id>, removed: &HashSet<N::Id>, this: &Self| {
                !removed.contains(id) && (added.contains(id) || this.graph().key_of(id).is_some())
            };
        for spec in &specs {
            match spec {
                EditSpec::InsertNode(node) => {
                    removed.remove(node.id());
                    added.insert(node.id().clone());
                }
                EditSpec::RemoveNode(id) => {
                    if !present(id, &added, &removed, self) {
                        return Err(CommitError::UnknownNode(id.clone()));
                    }
                    // Applying removal withdraws every incident assertion.
                    // Reinserting the same node identity cannot revive them.
                    if let Some(key) = self.graph().key_of(id) {
                        for edge_key in self.graph().incident_edges(key) {
                            if let Some(edge_id) = self.by_edge_key.get(&edge_key) {
                                dropped_edges.insert(*edge_id);
                                withdrawn_assertions.insert(*edge_id);
                            }
                        }
                    }
                    added.remove(id);
                    removed.insert(id.clone());
                }
                EditSpec::Connect { from, to, .. } => {
                    if !present(from, &added, &removed, self) {
                        return Err(CommitError::UnknownNode(from.clone()));
                    }
                    if !present(to, &added, &removed, self) {
                        return Err(CommitError::UnknownNode(to.clone()));
                    }
                }
                EditSpec::Assert { from, to, .. } => {
                    if !present(from, &added, &removed, self) {
                        return Err(CommitError::UnknownNode(from.clone()));
                    }
                    if !present(to, &added, &removed, self) {
                        return Err(CommitError::UnknownNode(to.clone()));
                    }
                }
                EditSpec::ConfirmAssertion { id, .. } => {
                    if withdrawn_assertions.contains(id) || dropped_edges.contains(id) {
                        return Err(CommitError::AssertionAlreadyWithdrawn(*id));
                    }
                    let record = self
                        .assertion(*id)
                        .ok_or(CommitError::UnknownAssertion(*id))?;
                    if !record.is_held()
                        || !present(&record.from, &added, &removed, self)
                        || !present(&record.to, &added, &removed, self)
                    {
                        return Err(CommitError::AssertionAlreadyWithdrawn(*id));
                    }
                }
                EditSpec::WithdrawAssertion { id, .. } => {
                    if withdrawn_assertions.contains(id) || dropped_edges.contains(id) {
                        return Err(CommitError::AssertionAlreadyWithdrawn(*id));
                    }
                    let record = self
                        .assertion(*id)
                        .ok_or(CommitError::UnknownAssertion(*id))?;
                    if !record.is_held()
                        || !present(&record.from, &added, &removed, self)
                        || !present(&record.to, &added, &removed, self)
                    {
                        return Err(CommitError::AssertionAlreadyWithdrawn(*id));
                    }
                    withdrawn_assertions.insert(*id);
                    dropped_edges.insert(*id);
                }
                EditSpec::Disconnect(id) => {
                    if dropped_edges.contains(id) || self.edge_key(*id).is_none() {
                        return Err(CommitError::UnknownEdge(*id));
                    }
                    dropped_edges.insert(*id);
                }
                EditSpec::Derive { node, .. } => {
                    if !present(node, &added, &removed, self) {
                        return Err(CommitError::UnknownNode(node.clone()));
                    }
                }
                EditSpec::SetFacet { node, .. } | EditSpec::RemoveFacet { node, .. } => {
                    if !present(node, &added, &removed, self) {
                        return Err(CommitError::UnknownNode(node.clone()));
                    }
                }
            }
        }

        // Lower: mint edge ids for connects, in spec order.
        let mut edits = Vec::with_capacity(specs.len());
        let mut minted = Vec::new();
        for spec in specs {
            edits.push(match spec {
                EditSpec::InsertNode(node) => GraphEdit::InsertNode(node),
                EditSpec::RemoveNode(id) => GraphEdit::RemoveNode(id),
                EditSpec::Connect { from, to, edge } => {
                    let id = EdgeId::new(self.writer, self.next_edge);
                    self.next_edge += 1;
                    minted.push(id);
                    GraphEdit::Connect { id, from, to, edge }
                }
                EditSpec::Assert {
                    from,
                    to,
                    edge,
                    asserter,
                    at_ms,
                    basis,
                } => {
                    let id = EdgeId::new(self.writer, self.next_edge);
                    self.next_edge += 1;
                    minted.push(id);
                    GraphEdit::Assert {
                        id,
                        from,
                        to,
                        edge,
                        asserter,
                        at_ms,
                        basis,
                    }
                }
                EditSpec::ConfirmAssertion { id, at_ms } => {
                    GraphEdit::ConfirmAssertion { id, at_ms }
                }
                EditSpec::WithdrawAssertion { id, at_ms } => {
                    GraphEdit::WithdrawAssertion { id, at_ms }
                }
                EditSpec::Disconnect(id) => GraphEdit::Disconnect(id),
                EditSpec::Derive { node, from } => GraphEdit::Derive { node, from },
                EditSpec::SetFacet { node, facet, value } => {
                    GraphEdit::SetFacet { node, facet, value }
                }
                EditSpec::RemoveFacet { node, facet } => GraphEdit::RemoveFacet { node, facet },
            });
        }

        // Apply, then append as one journal entry.
        let batch = Batch { author, edits };
        self.apply_batch(&batch);
        let seq = self.log.append(batch);
        Ok(Committed {
            batch: BatchId(seq.index() as u64),
            edges: minted,
        })
    }
}

/// Wrap a pre-gate edit log (entries are bare [`GraphEdit`]s) as an attributed
/// batch log: one single-edit batch per entry, authored
/// [`Author::pre_gate`]. The log's identity carries over; fork provenance
/// cannot (the journal exposes no parts-constructor), which open question 7 of the
/// plan records.
pub fn migrate_pre_gate<N, E>(legacy: Journal<GraphEdit<N, E>>) -> Journal<Batch<N, E>>
where
    N: Identified + Clone,
    E: Clone,
{
    let mut log = match legacy.id() {
        Some(id) => Journal::with_id(id.clone()),
        None => Journal::new(),
    };
    for edit in legacy.entries() {
        log.append(Batch {
            author: Author::pre_gate(),
            edits: vec![edit.clone()],
        });
    }
    log
}

#[cfg(test)]
mod tests;
