// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Policy-before-insert processing for replicated operations.
//!
//! Every ingress lane should pass an operation through [`OperationProcessor`]
//! before it mutates the shared store. The processor owns p2panda's structural
//! checks, retained-frontier continuity, idempotence, and the atomic indexed
//! write with optional prefix removal. A domain policy owns addressing,
//! authorization, body semantics, and permission to prune preceding history.

use std::collections::{BTreeMap, BTreeSet};

use muniment::{Backend, StoreError, WriteOp};
use p2panda_core::operation::validate_operation;
use p2panda_core::prune::validate_prunable_backlink;
use p2panda_core::{Extensions, Hash, LogId, Operation, Topic};

use crate::MunimentStore;
use crate::store::IndexedOperation;

/// The topic and per-author log selected by an admitted operation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoreTarget<L> {
    /// Replication topic through which this log is discovered.
    pub topic: Topic,
    /// Domain-selected log within the operation's author.
    pub log_id: L,
}

/// Authorized treatment of the operation's preceding log history.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HistoryAction {
    /// Keep the existing prefix and require an ordinary backlink.
    #[default]
    Keep,
    /// Delete every operation before the admitted operation.
    PruneBeforeCurrent,
}

impl HistoryAction {
    fn prunes(self) -> bool {
        matches!(self, Self::PruneBeforeCurrent)
    }
}

/// Domain-authorized storage decision for one operation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Admission<L> {
    /// Topic and per-author log selected by the domain.
    pub target: StoreTarget<L>,
    /// What to do with preceding history after accepting this operation.
    pub history: HistoryAction,
    /// Operation payloads to erase in the same backend batch as this insert.
    ///
    /// Missing operations are ignored: a peer may receive a checkpoint after
    /// the named prefix has already been collected locally.
    pub erase_payloads: Vec<Hash>,
}

impl<L> Admission<L> {
    /// Admit an operation while retaining its full preceding log.
    pub fn keep(target: StoreTarget<L>) -> Self {
        Self {
            target,
            history: HistoryAction::Keep,
            erase_payloads: Vec::new(),
        }
    }

    /// Admit an operation as the surviving head of a pruned prefix.
    pub fn prune_before_current(target: StoreTarget<L>) -> Self {
        Self {
            target,
            history: HistoryAction::PruneBeforeCurrent,
            erase_payloads: Vec::new(),
        }
    }

    /// Erase the named payloads if they are present locally.
    pub fn erasing_payloads(mut self, payloads: impl IntoIterator<Item = Hash>) -> Self {
        self.erase_payloads.extend(payloads);
        self
    }
}

impl<L> StoreTarget<L> {
    /// Select a topic and log for an admitted operation.
    pub fn new(topic: Topic, log_id: L) -> Self {
        Self { topic, log_id }
    }
}

/// A domain refusal made before storage mutation.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{code}: {detail}")]
pub struct Reject {
    /// Stable, machine-readable reason.
    pub code: &'static str,
    /// Human-readable diagnostic detail.
    pub detail: String,
}

impl Reject {
    /// Construct a rejection with a stable code and diagnostic detail.
    pub fn new(code: &'static str, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: detail.into(),
        }
    }
}

/// Domain-owned admission policy.
///
/// The shared processor has already verified the operation's signature,
/// header, and body hash before calling this method. Implementations check the
/// addressed space, capabilities, governed policy, and domain body grammar,
/// then return the topic, log, and authorized history action selected for
/// storage.
pub trait OperationPolicy<E>: Send + Sync {
    /// The domain's per-author log identifier.
    type LogId: LogId;

    /// Authorize and address one structurally valid operation.
    fn admit(&self, operation: &Operation<E>) -> Result<Admission<Self::LogId>, Reject>;
}

/// Whether processing inserted a new operation or observed an accepted repeat.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcessOutcome {
    /// The operation and its topic/log index were committed.
    Inserted {
        /// Number of preceding log entries removed in the same backend batch.
        pruned_entries: u64,
        /// Number of retained operation headers whose payloads were erased.
        erased_payloads: u64,
    },
    /// The header was already retained and this call attached its verified body.
    HydratedPayload,
    /// The same operation hash was already present.
    Duplicate,
}

/// Counts returned after one atomic operation batch.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct BatchProcessOutcome {
    pub inserted: u64,
    pub duplicate: u64,
    pub hydrated_payloads: u64,
}

impl ProcessOutcome {
    /// Whether this call committed a new operation.
    pub fn inserted(self) -> bool {
        matches!(self, Self::Inserted { .. })
    }

    /// Whether this call attached a previously absent operation body.
    pub fn hydrated_payload(self) -> bool {
        matches!(self, Self::HydratedPayload)
    }

    /// Number of preceding log entries removed by this call.
    pub fn pruned_entries(self) -> u64 {
        match self {
            Self::Inserted { pruned_entries, .. } => pruned_entries,
            Self::HydratedPayload | Self::Duplicate => 0,
        }
    }

    /// Number of retained payloads erased by this call.
    pub fn erased_payloads(self) -> u64 {
        match self {
            Self::Inserted {
                erased_payloads, ..
            } => erased_payloads,
            Self::HydratedPayload | Self::Duplicate => 0,
        }
    }
}

/// A failure before or during accepted-operation processing.
#[derive(Debug, thiserror::Error)]
pub enum ProcessError {
    /// Basic p2panda signature, header, or body validation failed.
    #[error("invalid operation: {0}")]
    InvalidOperation(String),
    /// The domain rejected the operation.
    #[error("operation rejected: {0}")]
    Rejected(#[from] Reject),
    /// A non-root operation arrived before its predecessor.
    #[error("operation at sequence {seq_num} is missing its predecessor")]
    MissingPredecessor { seq_num: u64 },
    /// The operation is at or below the retained author/log frontier.
    #[error("operation at sequence {seq_num} does not advance retained frontier {frontier}")]
    StaleOperation { seq_num: u64, frontier: u64 },
    /// The operation does not extend the current author/log frontier.
    #[error("operation does not extend the current log: {0}")]
    Backlink(String),
    /// Durable storage failed.
    #[error("replication store: {0}")]
    Store(#[from] StoreError),
}

/// Shared admission and storage path for one replicated domain.
#[derive(Clone)]
pub struct OperationProcessor<B, E, P> {
    store: MunimentStore<B, E>,
    policy: P,
}

impl<B, E, P> OperationProcessor<B, E, P> {
    /// Bind a domain policy to its shared operation store.
    pub fn new(store: MunimentStore<B, E>, policy: P) -> Self {
        Self { store, policy }
    }

    /// The store used by this processor.
    pub fn store(&self) -> &MunimentStore<B, E> {
        &self.store
    }

    /// The domain policy used by this processor.
    pub fn policy(&self) -> &P {
        &self.policy
    }
}

impl<B, E, P> OperationProcessor<B, E, P>
where
    B: Backend,
    E: Extensions,
    P: OperationPolicy<E>,
{
    /// Run structural and domain admission checks without mutating storage.
    ///
    /// Batch import uses this for an all-record authorization pass before the
    /// first accepted operation is committed. Continuity is still checked by
    /// [`process`](Self::process) against the live retained frontier.
    pub fn preflight(&self, operation: &Operation<E>) -> Result<Admission<P::LogId>, ProcessError> {
        validate_operation(operation)
            .map_err(|err| ProcessError::InvalidOperation(err.to_string()))?;
        if operation.hash != operation.header.hash() {
            return Err(ProcessError::InvalidOperation(
                "operation id does not match its signed header".into(),
            ));
        }
        Ok(self.policy.admit(operation)?)
    }

    /// Validate, authorize, continuity-check, and atomically index one operation.
    ///
    /// Calls for a given author/log are serialized across all processors built
    /// from clones of the same store. This closes the read-frontier/write race
    /// until the backend grows a compare-and-swap frontier primitive.
    pub async fn process(&self, operation: &Operation<E>) -> Result<ProcessOutcome, ProcessError> {
        let admission = self.preflight(operation)?;
        let ingress = self
            .store
            .ingress_lock(&operation.header.verifying_key, &admission.target.log_id)
            .await?;
        let _guard = ingress.lock().await;

        if self.store.has_operation(&operation.hash).await? {
            if let (Some(payload_hash), Some(body)) = (
                operation.header.payload_hash.as_ref(),
                operation.body.as_ref(),
            ) && self
                .store
                .attach_payload(payload_hash, &body.to_bytes())
                .await?
                > 0
            {
                return Ok(ProcessOutcome::HydratedPayload);
            }
            return Ok(ProcessOutcome::Duplicate);
        }

        let latest = self
            .store
            .get_latest_entry(&operation.header.verifying_key, &admission.target.log_id)
            .await?;
        if let Some(previous) = latest.as_ref()
            && operation.header.seq_num <= previous.header.seq_num
        {
            return Err(ProcessError::StaleOperation {
                seq_num: u64::from(operation.header.seq_num),
                frontier: u64::from(previous.header.seq_num),
            });
        }
        match (operation.header.seq_num, latest.as_ref(), admission.history) {
            (0, None, _) => {}
            (seq_num, None, HistoryAction::Keep) => {
                return Err(ProcessError::MissingPredecessor {
                    seq_num: u64::from(seq_num),
                });
            }
            (_, previous, history) => validate_prunable_backlink(
                previous.map(|operation| &operation.header),
                &operation.header,
                history.prunes(),
            )
            .map_err(|err| ProcessError::Backlink(err.to_string()))?,
        }

        let (inserted, pruned_entries, erased_payloads) = self
            .store
            .insert_indexed_operation_with_effects(
                &admission.target.topic,
                operation,
                &admission.target.log_id,
                admission.history.prunes(),
                &admission.erase_payloads,
            )
            .await?;
        Ok(if inserted {
            ProcessOutcome::Inserted {
                pruned_entries,
                erased_payloads,
            }
        } else {
            ProcessOutcome::Duplicate
        })
    }

    /// Validate, authorize, continuity-check and commit one operation together
    /// with the caller's own writes, in a single backend batch.
    ///
    /// A lane that stages a record outside the store -- a parked record
    /// waiting on an epoch this member does not hold, a staged drop -- retires
    /// its staging key through `writes`, in the commit that stores the record,
    /// so no crash can leave the record both stored and staged. The writes
    /// land whenever the batch commits, an already stored operation included;
    /// an invalid, refused or discontinuous operation returns the error and
    /// applies nothing. Calls for one author log are serialized as in
    /// [`process`](Self::process).
    ///
    /// Returns whether this call stored a new operation. Prune and erase
    /// counts are not reported, because the batch write does not return them.
    pub async fn process_with_writes(
        &self,
        operation: &Operation<E>,
        writes: &[WriteOp],
    ) -> Result<bool, ProcessError> {
        let admission = self.preflight(operation)?;
        let ingress = self
            .store
            .ingress_lock(&operation.header.verifying_key, &admission.target.log_id)
            .await?;
        let _guard = ingress.lock().await;
        let outcome = self
            .process_batch_atomic(std::slice::from_ref(operation), &[], writes)
            .await?;
        Ok(outcome.inserted == 1)
    }

    /// Validate and commit an operation corpus in one backend batch.
    ///
    /// Duplicate ids are reported but omitted from the write set. New
    /// operations are ordered by author, domain log, and sequence, then checked
    /// against a virtual frontier that includes earlier operations in this
    /// corpus. `trailing_writes` lets native-drop import remove its staging keys
    /// and persist its receipt in the same commit.
    pub(crate) async fn process_batch_atomic(
        &self,
        operations: &[Operation<E>],
        leading_writes: &[WriteOp],
        trailing_writes: &[WriteOp],
    ) -> Result<BatchProcessOutcome, ProcessError> {
        let mut prepared = Vec::with_capacity(operations.len());
        for operation in operations {
            let admission = self.preflight(operation)?;
            prepared.push((operation, admission));
        }
        prepared.sort_by(|(left, left_admission), (right, right_admission)| {
            left.header
                .verifying_key
                .cmp(&right.header.verifying_key)
                .then_with(|| {
                    left_admission
                        .target
                        .log_id
                        .cmp(&right_admission.target.log_id)
                })
                .then_with(|| left.header.seq_num.cmp(&right.header.seq_num))
        });

        let mut frontiers: BTreeMap<_, Option<Operation<E>>> = BTreeMap::new();
        let mut seen = BTreeSet::new();
        let mut accepted = Vec::new();
        let mut hydration_writes = leading_writes.to_vec();
        let mut outcome = BatchProcessOutcome::default();
        for (operation, admission) in prepared {
            let id = operation.hash.to_hex();
            if !seen.insert(id) {
                outcome.duplicate += 1;
                continue;
            }
            if self.store.has_operation(&operation.hash).await? {
                if let (Some(payload_hash), Some(body)) = (
                    operation.header.payload_hash.as_ref(),
                    operation.body.as_ref(),
                ) {
                    let (writes, attached) = self
                        .store
                        .payload_attachment_writes(payload_hash, &body.to_bytes())
                        .await?;
                    hydration_writes.extend(writes);
                    outcome.hydrated_payloads += attached;
                }
                outcome.duplicate += 1;
                continue;
            }
            let key = (
                operation.header.verifying_key,
                admission.target.log_id.clone(),
            );
            if !frontiers.contains_key(&key) {
                let latest = self.store.get_latest_entry(&key.0, &key.1).await?;
                frontiers.insert(key.clone(), latest);
            }
            let latest = frontiers.get(&key).and_then(Option::as_ref);
            if let Some(previous) = latest
                && operation.header.seq_num <= previous.header.seq_num
            {
                return Err(ProcessError::StaleOperation {
                    seq_num: u64::from(operation.header.seq_num),
                    frontier: u64::from(previous.header.seq_num),
                });
            }
            match (operation.header.seq_num, latest, admission.history) {
                (0, None, _) => {}
                (seq_num, None, HistoryAction::Keep) => {
                    return Err(ProcessError::MissingPredecessor {
                        seq_num: u64::from(seq_num),
                    });
                }
                (_, previous, history) => validate_prunable_backlink(
                    previous.map(|operation| &operation.header),
                    &operation.header,
                    history.prunes(),
                )
                .map_err(|err| ProcessError::Backlink(err.to_string()))?,
            }
            frontiers.insert(key, Some(operation.clone()));
            accepted.push((operation, admission));
            outcome.inserted += 1;
        }

        let indexed: Vec<_> = accepted
            .iter()
            .map(|(operation, admission)| IndexedOperation {
                topic: &admission.target.topic,
                operation,
                log_id: &admission.target.log_id,
                prune_before_current: admission.history.prunes(),
                erase_payloads: &admission.erase_payloads,
            })
            .collect();
        self.store
            .apply_indexed_operation_batch(&indexed, &hydration_writes, trailing_writes)
            .await?;
        Ok(outcome)
    }
}

#[cfg(test)]
#[path = "processor/tests.rs"]
mod tests;
