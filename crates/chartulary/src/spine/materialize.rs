// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl<N: Identified + Clone, E: Clone> GraphLog<N, E> {
    /// Apply one edit to the materialized graph and its bookkeeping. The single
    /// mutation path, shared by live editing and replay, so the two never diverge.
    pub(super) fn apply_edit(&mut self, edit: &GraphEdit<N, E>, author: &Author) {
        match edit {
            GraphEdit::InsertNode(node) => {
                self.graph.insert(node.clone());
            }
            GraphEdit::RemoveNode(id) => {
                if let Some(key) = self.graph.key_of(id) {
                    for edge_key in self.graph.incident_edges(key) {
                        if let Some(edge_id) = self.by_edge_key.remove(&edge_key) {
                            self.by_edge_id.remove(&edge_id);
                            self.graph.disconnect(edge_key);
                            if let Some(assertion) = self.assertions.get_mut(&edge_id) {
                                assertion.span.withdrawn = Some(AssertionWithdrawal {
                                    at_ms: None,
                                    by: author.clone(),
                                });
                            }
                        }
                    }
                    self.graph.remove(key);
                }
                self.derivations.remove(id);
                self.facets.remove_node(id);
            }
            GraphEdit::Connect { id, from, to, edge } => {
                // Assertion identity is first-write-wins. A stale duplicate
                // connect in a merged/replayed journal cannot replace a
                // retained tombstone and revive the edge.
                if !self.assertions.contains_key(id) {
                    if let (Some(from_key), Some(to_key)) =
                        (self.graph.key_of(from), self.graph.key_of(to))
                    {
                        let edge_key = self.graph.connect(from_key, to_key, edge.clone());
                        self.by_edge_id.insert(*id, edge_key);
                        self.by_edge_key.insert(edge_key, *id);
                        self.assertions.insert(
                            *id,
                            AssertionRecord {
                                id: *id,
                                from: from.clone(),
                                to: to.clone(),
                                edge: edge.clone(),
                                asserter: Some(author.clone()),
                                span: AssertionSpan::new(None),
                                basis: AssertionBasis::default(),
                            },
                        );
                    }
                }
                // Advance past this id even on a dangling connect, so replay keeps
                // assigning ids monotonically. Only for ids THIS replica minted:
                // replaying a peer's edits must not consume our counter range,
                // which is what keeps two writers' ids disjoint after a merge.
                if id.writer == self.writer {
                    self.next_edge = self.next_edge.max(id.counter + 1);
                }
            }
            GraphEdit::Assert {
                id,
                from,
                to,
                edge,
                asserter,
                at_ms,
                basis,
            } => {
                if !self.assertions.contains_key(id) {
                    if let (Some(from_key), Some(to_key)) =
                        (self.graph.key_of(from), self.graph.key_of(to))
                    {
                        let edge_key = self.graph.connect(from_key, to_key, edge.clone());
                        self.by_edge_id.insert(*id, edge_key);
                        self.by_edge_key.insert(edge_key, *id);
                        self.assertions.insert(
                            *id,
                            AssertionRecord {
                                id: *id,
                                from: from.clone(),
                                to: to.clone(),
                                edge: edge.clone(),
                                asserter: asserter.clone(),
                                span: AssertionSpan::new(*at_ms),
                                basis: basis.clone(),
                            },
                        );
                    }
                }
                if id.writer == self.writer {
                    self.next_edge = self.next_edge.max(id.counter.saturating_add(1));
                }
            }
            GraphEdit::Disconnect(id) => {
                if let Some(edge_key) = self.by_edge_id.remove(id) {
                    self.by_edge_key.remove(&edge_key);
                    self.graph.disconnect(edge_key);
                    if let Some(assertion) = self.assertions.get_mut(id) {
                        assertion.span.withdrawn = Some(AssertionWithdrawal {
                            at_ms: None,
                            by: author.clone(),
                        });
                    }
                }
            }
            GraphEdit::ConfirmAssertion { id, at_ms } => {
                if let Some(assertion) = self.assertions.get_mut(id) {
                    if assertion.is_held() {
                        if at_ms.is_some() {
                            assertion.span.last_asserted_ms = *at_ms;
                        }
                    }
                }
            }
            GraphEdit::WithdrawAssertion { id, at_ms } => {
                if let Some(edge_key) = self.by_edge_id.remove(id) {
                    self.by_edge_key.remove(&edge_key);
                    self.graph.disconnect(edge_key);
                }
                if let Some(assertion) = self.assertions.get_mut(id) {
                    if assertion.is_held() {
                        assertion.span.withdrawn = Some(AssertionWithdrawal {
                            at_ms: *at_ms,
                            by: author.clone(),
                        });
                    }
                }
            }
            GraphEdit::Derive { node, from } => {
                self.derivations
                    .entry(node.clone())
                    .or_default()
                    .push(from.clone());
            }
            GraphEdit::SetFacet { node, facet, value } => {
                if self.graph.key_of(node).is_some() {
                    self.facets
                        .set(node.clone(), facet.clone(), value.clone(), &AcceptAll)
                        .expect("replay preserves an already-admitted facet value");
                }
            }
            GraphEdit::RemoveFacet { node, facet } => {
                if self.graph.key_of(node).is_some() {
                    self.facets.remove(node, facet);
                }
            }
        }
    }

    /// Apply every edit in a batch, in order.
    pub(crate) fn apply_batch(&mut self, batch: &Batch<N, E>) {
        for edit in &batch.edits {
            self.apply_edit(edit, &batch.author);
        }
    }

    /// Commit a single spec at the current revision (cannot conflict), for the
    /// convenience mutators. Returns whether it committed.
    pub(super) fn commit_one(
        &mut self,
        author: &Author,
        spec: EditSpec<N, E>,
    ) -> Option<crate::commit::Committed> {
        let revision = self.revision();
        self.commit_batch(author.clone(), revision, vec![spec]).ok()
    }

    /// Insert (or upsert, by identity) a node, attributed. Returns its graph key.
    pub fn insert_node(&mut self, author: &Author, node: N) -> NodeKey {
        let id = node.id().clone();
        self.commit_one(author, EditSpec::InsertNode(node))
            .expect("a single insert cannot conflict or reference unknowns");
        self.graph.key_of(&id).expect("just inserted")
    }

    /// Remove the node with `id` and its incident edges, attributed. A no-op
    /// (and no journal entry) if absent.
    pub fn remove_node(&mut self, author: &Author, id: &N::Id) {
        let _ = self.commit_one(author, EditSpec::RemoveNode(id.clone()));
    }

    /// Connect `from` to `to` with `edge`, attributed, returning the new edge's
    /// stable id, or `None` if either endpoint is absent.
    pub fn connect(
        &mut self,
        author: &Author,
        from: &N::Id,
        to: &N::Id,
        edge: E,
    ) -> Option<EdgeId> {
        self.commit_one(
            author,
            EditSpec::Connect {
                from: from.clone(),
                to: to.clone(),
                edge,
            },
        )
        .map(|committed| committed.edges[0])
    }

    /// Retract the edge with stable id `id`, attributed. Returns whether it was
    /// present.
    pub fn disconnect(&mut self, author: &Author, id: EdgeId) -> bool {
        self.commit_one(author, EditSpec::Disconnect(id)).is_some()
    }

    /// Assert a relation with explicit source metadata. Time and asserter are
    /// supplied by the caller; Chartulary does not consult a clock or infer
    /// source identity from the batch author.
    pub fn assert_relation(
        &mut self,
        author: Author,
        from: N::Id,
        to: N::Id,
        edge: E,
        asserter: Option<Author>,
        at_ms: Option<u64>,
        basis: AssertionBasis,
    ) -> Result<EdgeId, crate::commit::CommitError<N::Id>> {
        let committed = self.commit_batch(
            author,
            self.revision(),
            vec![EditSpec::Assert {
                from,
                to,
                edge,
                asserter,
                at_ms,
                basis,
            }],
        )?;
        Ok(committed.edges[0])
    }

    /// Confirm a currently held assertion. A withdrawn assertion cannot be
    /// revived; `None` preserves its last known confirmation time.
    pub fn confirm_assertion(&mut self, author: &Author, id: EdgeId, at_ms: Option<u64>) -> bool {
        self.commit_one(author, EditSpec::ConfirmAssertion { id, at_ms })
            .is_some()
    }

    /// Withdraw a currently held assertion with caller-supplied optional time.
    pub fn withdraw_assertion(&mut self, author: &Author, id: EdgeId, at_ms: Option<u64>) -> bool {
        self.commit_one(author, EditSpec::WithdrawAssertion { id, at_ms })
            .is_some()
    }

    /// Record that `node` derives from a node in another graph, attributed. A
    /// no-op (and no journal entry) if `node` is absent.
    pub fn derive(&mut self, author: &Author, node: &N::Id, from: DerivationRecord<N::Id>) {
        let _ = self.commit_one(
            author,
            EditSpec::Derive {
                node: node.clone(),
                from,
            },
        );
    }

    /// Set one facet as its own attributed journal edit.
    pub fn set_facet(
        &mut self,
        author: &Author,
        node: &N::Id,
        facet: FacetId,
        value: Value,
    ) -> bool {
        self.commit_one(
            author,
            EditSpec::SetFacet {
                node: node.clone(),
                facet,
                value,
            },
        )
        .is_some()
    }

    /// Remove one facet as its own attributed journal edit.
    pub fn remove_facet(&mut self, author: &Author, node: &N::Id, facet: FacetId) -> bool {
        self.commit_one(
            author,
            EditSpec::RemoveFacet {
                node: node.clone(),
                facet,
            },
        )
        .is_some()
    }

    /// Fork this graph under a new log identity. The fork is self-contained: it
    /// copies the whole edit history and carries a provenance header pointing back
    /// at the source (see [`provenance`](GraphLog::provenance)), then diverges
    /// independently.
    pub fn fork(&self, new_id: LogId) -> Self {
        Self::replay(self.log.fork(new_id))
    }

    /// Rebuild a graph by replaying a batch log from empty.
    pub fn replay(log: Journal<Batch<N, E>>) -> Self {
        Self::replay_for_writer(log, WriterId::LOCAL)
    }

    /// Rebuild a graph for a specific writer.
    ///
    /// Setting the writer before replay lets every historical edge minted by
    /// this replica restore its counter frontier. This is the multi-writer
    /// recovery path; [`replay`](Self::replay) remains the legacy
    /// single-writer path.
    pub fn replay_for_writer(log: Journal<Batch<N, E>>, writer: WriterId) -> Self {
        let mut this = Self::new().for_writer(writer);
        for batch in log.entries() {
            this.apply_batch(batch);
        }
        this.log = log;
        this
    }

    pub(super) fn from_snapshot(snapshot: Snapshot<N, E>) -> Result<(Self, u64), StoreError>
    where
        N::Id: Serialize,
        E: Serialize,
    {
        let Snapshot {
            nodes,
            edges,
            derivations,
            facets,
            writer,
            next_edge,
            at_seq,
            assertions,
            journal_digest: _,
        } = snapshot;
        let assertions = assertions.ok_or_else(|| {
            StoreError::Codec(
                "snapshot has no assertion ledger; replay the retained journal".into(),
            )
        })?;

        let node_ids: std::collections::BTreeSet<_> =
            nodes.iter().map(|node| node.id().clone()).collect();
        let mut records = BTreeMap::new();
        let mut expected_edges = BTreeMap::new();
        for record in assertions {
            let id = record.id;
            if records.contains_key(&id) {
                return Err(StoreError::Codec(format!(
                    "snapshot contains duplicate assertion id {id:?}"
                )));
            }
            if record.is_held() {
                if !node_ids.contains(&record.from) || !node_ids.contains(&record.to) {
                    return Err(StoreError::Codec(format!(
                        "held assertion {id:?} references an endpoint absent from the snapshot"
                    )));
                }
                expected_edges.insert(id, (record.from.clone(), record.to.clone()));
            }
            records.insert(id, record);
        }

        // The compatibility edge tuples are validated only for their identity
        // and endpoints, then ignored for materialization. Their payloads may
        // have nondeterministic serialization (for example, HashMap iteration)
        // and the retained ledger is the authoritative payload source.
        let mut serialized_edges = BTreeMap::new();
        for (id, from, to, _edge) in edges {
            if serialized_edges.insert(id, (from, to)).is_some() {
                return Err(StoreError::Codec(format!(
                    "snapshot contains duplicate held edge id {id:?}"
                )));
            }
        }
        if serialized_edges != expected_edges {
            return Err(StoreError::Codec(
                "snapshot held edge tuples do not match its assertion ledger".into(),
            ));
        }

        // The local writer counter is a mint frontier, not just a hint. A
        // retained record remains reserved after withdrawal, so the next id
        // must be strictly above every retained id owned by this writer.
        if let Some((id, _)) = records
            .iter()
            .filter(|(id, _)| id.writer == writer)
            .max_by_key(|(id, _)| id.counter)
        {
            let minimum_next = id.counter.checked_add(1).ok_or_else(|| {
                StoreError::Codec(format!(
                    "snapshot assertion id {id:?} exhausts its writer counter"
                ))
            })?;
            if next_edge < minimum_next {
                return Err(StoreError::Codec(format!(
                    "snapshot next_edge {next_edge} would reuse retained assertion id {id:?}"
                )));
            }
        }

        let mut this = Self::new();
        for node in nodes {
            this.graph.insert(node);
        }
        for record in records.values().filter(|record| record.is_held()) {
            let from_key = this.graph.key_of(&record.from).expect("validated endpoint");
            let to_key = this.graph.key_of(&record.to).expect("validated endpoint");
            let edge_key = this.graph.connect(from_key, to_key, record.edge.clone());
            this.by_edge_id.insert(record.id, edge_key);
            this.by_edge_key.insert(edge_key, record.id);
        }
        for (node, record) in derivations {
            this.derivations.entry(node).or_default().push(record);
        }
        this.facets = facets;
        this.assertions = records;
        this.writer = writer;
        this.next_edge = next_edge;
        Ok((this, at_seq))
    }
}
