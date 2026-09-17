// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The group-key lane: a signed, causally ordered carrier for
//! [`GroupSessionDispatch`] frames beside a [`GroupSession`].
//!
//! [`GroupSession::process`] requires the domain to authenticate and causally
//! order every control frame. This lane is that domain. Each record carries one
//! dispatch (the control frame plus its addressed direct frames) and a Personae
//! attestation for the writer key that signed it, verified through
//! [`stable_writer_subject`] under a group-scoped salt. The verified root is the
//! `authenticated_author_root` handed to `process`.
//!
//! Admission bounds the signed record size, the parent fan-out and the number
//! of direct frames, and decodes every frame through its versioned codec. A
//! record that arrives ahead of its author's previous record is parked until
//! that predecessor lands, then inserted.
//!
//! [`GroupKeyLane::drain_ready`] applies stored frames in causal order. A frame
//! behind its author's session sequence is superseded; one ahead of it, or from
//! an author whose pre-key is not registered yet, stays pending together with
//! its causal descendants. Every settled frame gets a durable mark, written only
//! after the host has persisted the session, so a reopen neither reprocesses
//! frames nor loses a transition to a crash.
//!
//! The sync protocol id is `lane_id(GROUP_KEY_LANE, group)`. The sync topic,
//! [`group_key_sync_topic`], is derived from the group id rather than equal to
//! it, because hosts commonly reuse one 32-byte id (a Moot id, say) as the
//! group id and as other lanes' topic.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use identity::{DerivedKeyAttestation, IdentityError, IdentityProvider};
use muniment::{Backend, StoreError, WriteOp};
use p2panda_core::cbor::{decode_cbor, encode_cbor};
use p2panda_core::{Body, Hash, Header, Operation, SigningKey, Topic, VerifyingKey};
use p2panda_net::{Endpoint, Gossip};
use p2panda_store::topics::TopicStore;
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;

use crate::drop::DropRecord;
use crate::{
    Admission, CausalEntry, CausalError, CausalLimits, GroupControlFrame, GroupControlId,
    GroupDirectFrame, GroupSession, GroupSessionDispatch, GroupSessionError, GroupSessionId,
    GroupSessionProcess, JoinError, JoinedSpace, MunimentStore, OperationPolicy,
    OperationProcessor, ProcessError, Reject, StoreTarget, WriterBindingError, author_head,
    causal_projection, decode_operation_record, lane_id, observed_frontier, operation_record,
    stable_writer_subject, validate_causal_metadata,
};

/// This lane's kind. The protocol id is `lane_id(GROUP_KEY_LANE, group.0)`.
pub const GROUP_KEY_LANE: &str = "stickleback/group-key/v1";

const GROUP_KEY_LOG: u64 = 0;
const GROUP_KEY_RECORD_VERSION: u16 = 1;
const GROUP_KEY_WRITER_DOMAIN: &[u8] = b"stickleback/group-key-writer/v1/";
const GROUP_KEY_TOPIC_DOMAIN: &[u8] = b"stickleback/group-key-topic/v1/";

/// Signed header extensions of one group-key record.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupKeyExt {
    pub group: [u8; 32],
    /// Exact observed frontier when the record was authored.
    pub parents: Vec<[u8; 32]>,
}

/// Admission bounds for group-key records.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GroupKeyLimits {
    /// Parent fan-out and signed record size.
    pub causal: CausalLimits,
    /// Direct frames one dispatch may address.
    pub max_direct_frames: usize,
    /// Records parked ahead of their predecessor, across all authors.
    pub max_parked: usize,
}

impl Default for GroupKeyLimits {
    fn default() -> Self {
        Self {
            causal: CausalLimits {
                max_parents: 64,
                max_payload_bytes: 1024 * 1024,
            },
            max_direct_frames: 1024,
            max_parked: 256,
        }
    }
}

/// What [`GroupKeyLane::accept`] did with one record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GroupKeyAdmission {
    /// Stored, plus any parked successors it released.
    Inserted { released: u64 },
    /// Already stored.
    Duplicate,
    /// Authenticated but ahead of its author's previous record; held aside.
    Parked,
}

/// One frame applied through [`GroupSession::process`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppliedGroupFrame {
    pub operation: [u8; 32],
    pub control: GroupControlId,
    pub process: GroupSessionProcess,
}

/// One frame the session refused; it stays settled and is not retried.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RefusedGroupFrame {
    pub operation: [u8; 32],
    pub reason: String,
}

/// Outcome of one [`GroupKeyLane::drain_ready`] pass. Settled frames from
/// earlier passes appear in none of these.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GroupKeyDrain {
    /// Applied, in causal order.
    pub applied: Vec<AppliedGroupFrame>,
    /// Already reflected in the session: authored locally, or behind its
    /// author's sequence (a welcome that arrived by invitation, say).
    pub superseded: Vec<[u8; 32]>,
    /// Not yet applicable: missing causal history, parked, ahead of its
    /// author's sequence, or from an author with no registered pre-key.
    pub pending: Vec<[u8; 32]>,
    pub refused: Vec<RefusedGroupFrame>,
}

#[derive(Debug, thiserror::Error)]
pub enum GroupKeyLaneError {
    #[error(transparent)]
    Identity(#[from] IdentityError),
    #[error(transparent)]
    WriterBinding(#[from] WriterBindingError),
    #[error("identity provider attested a different stable Personae root")]
    RootMismatch,
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Process(#[from] ProcessError),
    #[error(transparent)]
    Causal(#[from] CausalError),
    #[error(transparent)]
    Session(#[from] GroupSessionError),
    #[error(transparent)]
    Join(#[from] JoinError),
    #[error("group-key lane for {expected} was given a session for {actual}")]
    WrongGroup {
        expected: GroupSessionId,
        actual: GroupSessionId,
    },
    #[error("group-key lane already parks {0} records")]
    ParkedLimit(usize),
    #[error("authored group-key record was not inserted: {0:?}")]
    NotInserted(GroupKeyAdmission),
    #[error("group-key wire: {0}")]
    Wire(String),
    #[error("persist group session: {0}")]
    Persist(String),
}

/// Signed body: one dispatch through the frames' own codecs, plus the writer's
/// Personae attestation.
#[derive(Debug, Serialize, Deserialize)]
struct GroupKeyRecord {
    version: u16,
    #[serde(with = "serde_bytes")]
    control: Vec<u8>,
    direct: Vec<ByteBuf>,
    author_attestation: DerivedKeyAttestation,
}

/// Durable mark for a frame no later drain revisits.
#[derive(Serialize)]
enum SettledFrame {
    Applied,
    Superseded,
    Refused(String),
}

fn group_key_writer_salt(group: GroupSessionId) -> Vec<u8> {
    [GROUP_KEY_WRITER_DOMAIN, &group.0].concat()
}

/// This group's key-lane LogSync topic, which a dialer tags as an overlay.
pub fn group_key_sync_topic(group: GroupSessionId) -> [u8; 32] {
    *blake3::hash(&[GROUP_KEY_TOPIC_DOMAIN, &group.0].concat()).as_bytes()
}

fn settled_prefix(group: GroupSessionId) -> String {
    format!("group-key/{group}/settled/")
}

fn parked_prefix(group: GroupSessionId) -> String {
    format!("group-key/{group}/parked/")
}

fn parked_key(group: GroupSessionId, author: &VerifyingKey, seq_num: u32) -> String {
    format!("{}{}/{seq_num:08x}", parked_prefix(group), author.to_hex())
}

fn wire(error: impl fmt::Display) -> GroupKeyLaneError {
    GroupKeyLaneError::Wire(error.to_string())
}

#[derive(Clone)]
struct GroupKeyPolicy {
    group: GroupSessionId,
    limits: GroupKeyLimits,
}

impl OperationPolicy<GroupKeyExt> for GroupKeyPolicy {
    type LogId = u64;

    fn admit(&self, operation: &Operation<GroupKeyExt>) -> Result<Admission<u64>, Reject> {
        if operation.header.extensions.group != self.group.0 {
            return Err(Reject::new(
                "wrong-group-key-group",
                "record addresses another group",
            ));
        }
        validate_causal_metadata(
            operation,
            &operation.header.extensions.parents,
            self.limits.causal,
        )
        .map_err(|error| Reject::new("invalid-group-key-causality", error.to_string()))?;
        open_record(operation, self.group, &self.limits)?;
        Ok(Admission::keep(StoreTarget::new(
            Topic::from(group_key_sync_topic(self.group)),
            GROUP_KEY_LOG,
        )))
    }
}

/// Verify a record's author and frames; return the author's stable root and
/// the dispatch it carries.
fn open_record(
    operation: &Operation<GroupKeyExt>,
    group: GroupSessionId,
    limits: &GroupKeyLimits,
) -> Result<([u8; 32], GroupSessionDispatch), Reject> {
    let invalid = |detail: String| Reject::new("invalid-group-key-record", detail);
    let body = operation
        .body
        .as_ref()
        .ok_or_else(|| invalid("record body is absent".into()))?;
    let record: GroupKeyRecord =
        decode_cbor(body.to_bytes().as_slice()).map_err(|error| invalid(error.to_string()))?;
    if record.version != GROUP_KEY_RECORD_VERSION {
        return Err(invalid(format!("unsupported version {}", record.version)));
    }
    let root = stable_writer_subject(
        *operation.header.verifying_key.as_bytes(),
        Some(&record.author_attestation),
        &group_key_writer_salt(group),
    )
    .map_err(|error| Reject::new(error.code(), error.to_string()))?;
    if record.direct.len() > limits.max_direct_frames {
        return Err(invalid(format!(
            "{} direct frames; maximum is {}",
            record.direct.len(),
            limits.max_direct_frames
        )));
    }
    let control = GroupControlFrame::from_bytes(&record.control)
        .map_err(|error| invalid(error.to_string()))?;
    if control.group != group {
        return Err(invalid("control frame addresses another group".into()));
    }
    let mut recipients = BTreeSet::new();
    let mut direct = Vec::with_capacity(record.direct.len());
    for bytes in &record.direct {
        let frame =
            GroupDirectFrame::from_bytes(bytes).map_err(|error| invalid(error.to_string()))?;
        if frame.group != group
            || frame.control != control.id
            || !recipients.insert(frame.recipient)
        {
            return Err(invalid(
                "direct frame names another control frame or repeats a recipient".into(),
            ));
        }
        direct.push(frame);
    }
    Ok((root, GroupSessionDispatch { control, direct }))
}

async fn load_operations<B: Backend>(
    store: &MunimentStore<B, GroupKeyExt>,
    group: GroupSessionId,
) -> Result<Vec<Operation<GroupKeyExt>>, GroupKeyLaneError> {
    let topic = Topic::from(group_key_sync_topic(group));
    let logs: BTreeMap<VerifyingKey, Vec<u64>> = store.resolve(&topic).await?;
    let mut operations = Vec::new();
    for author in logs.keys() {
        let entries = store
            .get_log_entries(author, &GROUP_KEY_LOG, None, None)
            .await?
            .unwrap_or_default();
        operations.extend(entries.into_iter().map(|(operation, _)| operation));
    }
    Ok(operations)
}

fn causal_entries(operations: &[Operation<GroupKeyExt>]) -> Vec<CausalEntry<u64>> {
    operations
        .iter()
        .map(|operation| {
            CausalEntry::from_operation(
                operation,
                GROUP_KEY_LOG,
                operation.header.extensions.parents.clone(),
            )
        })
        .collect()
}

/// Admit one record, parking it when its author's predecessor is missing and
/// releasing parked successors once it lands.
async fn accept_operation<B: Backend>(
    processor: &OperationProcessor<B, GroupKeyExt, GroupKeyPolicy>,
    operation: &Operation<GroupKeyExt>,
) -> Result<GroupKeyAdmission, GroupKeyLaneError> {
    let store = processor.store();
    let author = operation.header.verifying_key;
    match processor.process(operation).await {
        Ok(outcome) if outcome.inserted() => {
            let released = release_parked(processor, author).await?;
            return Ok(GroupKeyAdmission::Inserted { released });
        },
        Ok(_) => return Ok(GroupKeyAdmission::Duplicate),
        Err(ProcessError::MissingPredecessor { .. }) => {},
        // A gap past the retained head parks; a fork at the next slot does not.
        Err(ProcessError::Backlink(detail)) => {
            let latest = store.get_latest_entry(&author, &GROUP_KEY_LOG).await?;
            if !latest.is_some_and(|previous| {
                previous
                    .header
                    .seq_num
                    .checked_add(1)
                    .is_some_and(|next| operation.header.seq_num > next)
            }) {
                return Err(ProcessError::Backlink(detail).into());
            }
        },
        Err(error) => return Err(error.into()),
    }
    let group = processor.policy().group;
    let key = parked_key(group, &author, operation.header.seq_num);
    let parked = store.backend().list(&parked_prefix(group)).await?;
    if !parked.contains(&key) && parked.len() >= processor.policy().limits.max_parked {
        return Err(GroupKeyLaneError::ParkedLimit(parked.len()));
    }
    let bytes = encode_cbor(&operation_record(operation, true)).map_err(wire)?;
    store.backend().put(&key, &bytes).await?;
    Ok(GroupKeyAdmission::Parked)
}

fn decode_parked(bytes: &[u8]) -> Result<Operation<GroupKeyExt>, GroupKeyLaneError> {
    let record: DropRecord = decode_cbor(bytes).map_err(wire)?;
    decode_operation_record(&record)
        .map_err(wire)?
        .ok_or_else(|| wire("parked record is not an operation"))
}

async fn release_parked<B: Backend>(
    processor: &OperationProcessor<B, GroupKeyExt, GroupKeyPolicy>,
    author: VerifyingKey,
) -> Result<u64, GroupKeyLaneError> {
    let store = processor.store();
    let group = processor.policy().group;
    let mut released = 0;
    while let Some(latest) = store.get_latest_entry(&author, &GROUP_KEY_LOG).await? {
        let Some(next) = latest.header.seq_num.checked_add(1) else {
            break;
        };
        let key = parked_key(group, &author, next);
        let Some(bytes) = store.backend().get(&key).await? else {
            break;
        };
        let unpark = WriteOp::Delete { key };
        let inserted = match decode_parked(&bytes) {
            Ok(operation) => processor
                .process_batch_atomic(&[operation], &[], std::slice::from_ref(&unpark))
                .await
                .is_ok_and(|outcome| outcome.inserted == 1),
            Err(_) => false,
        };
        if !inserted {
            // A parked record that no longer extends the log is discarded.
            store.backend().apply(&[unpark]).await?;
            break;
        }
        released += 1;
    }
    Ok(released)
}

/// One member's replica of a group's key lane.
pub struct GroupKeyLane<B: Backend + Clone> {
    store: MunimentStore<B, GroupKeyExt>,
    group: GroupSessionId,
    signing_seed: [u8; 32],
    stable_author: [u8; 32],
    attestation: DerivedKeyAttestation,
    limits: GroupKeyLimits,
}

impl<B: Backend + Clone> GroupKeyLane<B> {
    /// Replica writing under this group's Personae-derived key.
    pub fn for_identity<P: IdentityProvider + ?Sized>(
        backend: B,
        group: GroupSessionId,
        identity: &P,
    ) -> Result<Self, GroupKeyLaneError> {
        let salt = group_key_writer_salt(group);
        let keypair = identity.derive_keypair(&salt)?;
        let attestation = identity.attest_derived_key(&salt)?;
        let stable_author =
            stable_writer_subject(keypair.public_key().to_bytes(), Some(&attestation), &salt)?;
        if stable_author != identity.master_public_key().to_bytes() {
            return Err(GroupKeyLaneError::RootMismatch);
        }
        Ok(Self {
            store: MunimentStore::new(backend),
            group,
            signing_seed: keypair.to_seed(),
            stable_author,
            attestation,
            limits: GroupKeyLimits::default(),
        })
    }

    pub fn with_limits(mut self, limits: GroupKeyLimits) -> Self {
        self.limits = limits;
        self
    }

    pub fn group(&self) -> GroupSessionId {
        self.group
    }

    /// Stable Personae root this replica authors under.
    pub fn stable_author(&self) -> [u8; 32] {
        self.stable_author
    }

    pub fn sync_store(&self) -> MunimentStore<B, GroupKeyExt> {
        self.store.clone()
    }

    fn processor(&self) -> OperationProcessor<B, GroupKeyExt, GroupKeyPolicy> {
        OperationProcessor::new(
            self.store.clone(),
            GroupKeyPolicy {
                group: self.group,
                limits: self.limits,
            },
        )
    }

    /// Sign and store one dispatch the local session produced.
    ///
    /// The session applied the dispatch when it produced it, so persist the
    /// session before authoring: a record on the lane whose transition the
    /// author lost would fork the author's control sequence.
    pub async fn author_dispatch(
        &self,
        dispatch: &GroupSessionDispatch,
    ) -> Result<Operation<GroupKeyExt>, GroupKeyLaneError> {
        let entries = causal_entries(&load_operations(&self.store, self.group).await?);
        let parents = observed_frontier(&entries)?;
        let author = SigningKey::from_bytes(&self.signing_seed).verifying_key();
        let (seq_num, backlink) = author_head(&entries, *author.as_bytes(), &GROUP_KEY_LOG)?;
        let operation = self.build_operation(
            dispatch,
            self.attestation.clone(),
            parents,
            seq_num,
            backlink,
        )?;
        match self.accept(&operation).await? {
            GroupKeyAdmission::Inserted { .. } => Ok(operation),
            other => Err(GroupKeyLaneError::NotInserted(other)),
        }
    }

    fn build_operation(
        &self,
        dispatch: &GroupSessionDispatch,
        author_attestation: DerivedKeyAttestation,
        parents: Vec<[u8; 32]>,
        seq_num: u32,
        backlink: Option<[u8; 32]>,
    ) -> Result<Operation<GroupKeyExt>, GroupKeyLaneError> {
        let record = GroupKeyRecord {
            version: GROUP_KEY_RECORD_VERSION,
            control: dispatch.control.to_bytes()?,
            direct: dispatch
                .direct
                .iter()
                .map(|frame| frame.to_bytes().map(ByteBuf::from))
                .collect::<Result<_, _>>()?,
            author_attestation,
        };
        let body_bytes = encode_cbor(&record).map_err(wire)?;
        let header = Header::builder()
            .body(&body_bytes)
            .seq_num(seq_num)
            .backlink(backlink.map(Hash::from))
            .build(
                &SigningKey::from_bytes(&self.signing_seed),
                GroupKeyExt {
                    group: self.group.0,
                    parents,
                },
            );
        Ok(Operation {
            hash: header.hash(),
            header,
            body: Some(Body::from_bytes(&body_bytes)),
        })
    }

    /// Verify and store one received record.
    pub async fn accept(
        &self,
        operation: &Operation<GroupKeyExt>,
    ) -> Result<GroupKeyAdmission, GroupKeyLaneError> {
        accept_operation(&self.processor(), operation).await
    }

    /// Apply every ready, unsettled frame to `session` in causal order.
    ///
    /// When anything was applied, `persist` receives the advanced session
    /// before the settled marks are written. If it fails, no mark is written
    /// and the next pass judges those frames again: the session's own sequence
    /// check finds them superseded, or applies them to a reloaded session.
    pub async fn drain_ready<F, E>(
        &self,
        session: &mut GroupSession,
        persist: F,
    ) -> Result<GroupKeyDrain, GroupKeyLaneError>
    where
        F: FnOnce(&GroupSession) -> Result<(), E>,
        E: fmt::Display,
    {
        if session.group() != self.group {
            return Err(GroupKeyLaneError::WrongGroup {
                expected: self.group,
                actual: session.group(),
            });
        }
        let operations = load_operations(&self.store, self.group).await?;
        let entries = causal_entries(&operations);
        let causal = causal_projection(&entries)?;
        let prefix = settled_prefix(self.group);
        let settled: BTreeSet<String> = self
            .store
            .backend()
            .list(&prefix)
            .await?
            .into_iter()
            .collect();
        let member = session.member();
        let mut drain = GroupKeyDrain::default();
        let mut blocked = BTreeSet::new();
        let mut marks = Vec::new();
        for index in causal.order {
            let entry = &entries[index];
            let key = format!("{prefix}{}", hex::encode(entry.operation));
            if settled.contains(&key) {
                continue;
            }
            if entry
                .backlink
                .iter()
                .chain(&entry.parents)
                .any(|dependency| blocked.contains(dependency))
            {
                blocked.insert(entry.operation);
                drain.pending.push(entry.operation);
                continue;
            }
            let frame = match open_record(&operations[index], self.group, &self.limits) {
                Err(reject) => SettledFrame::Refused(reject.to_string()),
                Ok((root, dispatch)) => {
                    match session.process(root, &dispatch.control, dispatch.direct_for(member)) {
                        Ok(process) => {
                            drain.applied.push(AppliedGroupFrame {
                                operation: entry.operation,
                                control: dispatch.control.id,
                                process,
                            });
                            SettledFrame::Applied
                        },
                        Err(GroupSessionError::UnexpectedSequence {
                            expected, actual, ..
                        }) if actual < expected => {
                            drain.superseded.push(entry.operation);
                            SettledFrame::Superseded
                        },
                        Err(
                            GroupSessionError::UnexpectedSequence { .. }
                            | GroupSessionError::MissingPrekey(_),
                        ) => {
                            blocked.insert(entry.operation);
                            drain.pending.push(entry.operation);
                            continue;
                        },
                        Err(error) => SettledFrame::Refused(error.to_string()),
                    }
                },
            };
            if let SettledFrame::Refused(reason) = &frame {
                drain.refused.push(RefusedGroupFrame {
                    operation: entry.operation,
                    reason: reason.clone(),
                });
            }
            let value = encode_cbor(&frame).map_err(wire)?;
            marks.push(WriteOp::Put { key, value });
        }
        drain
            .pending
            .extend(causal.pending.iter().map(|pending| pending.operation));
        for key in self
            .store
            .backend()
            .list(&parked_prefix(self.group))
            .await?
        {
            if let Some(bytes) = self.store.backend().get(&key).await? {
                drain.pending.push(*decode_parked(&bytes)?.hash.as_bytes());
            }
        }
        if !drain.applied.is_empty() {
            persist(session).map_err(|error| GroupKeyLaneError::Persist(error.to_string()))?;
        }
        if !marks.is_empty() {
            self.store.backend().apply(&marks).await?;
        }
        Ok(drain)
    }
}

impl<B: Backend + Clone + Send + Sync + 'static> GroupKeyLane<B> {
    /// Join this group's key lane, admitting arrivals through [`Self::accept`].
    ///
    /// Protocol id `lane_id(GROUP_KEY_LANE, group.0)`; the topic is derived
    /// from the group id (see the module docs). Arrivals are only stored: the
    /// host runs [`Self::drain_ready`] when the lane reports new records.
    pub async fn join(
        &self,
        endpoint: Endpoint,
        gossip: Gossip,
    ) -> Result<JoinedSpace<GroupKeyExt>, JoinError> {
        let processor = self.processor();
        JoinedSpace::join::<_, u64, _, _>(
            lane_id(GROUP_KEY_LANE, self.group.0),
            self.sync_store(),
            endpoint,
            gossip,
            group_key_sync_topic(self.group),
            move |operation: Operation<GroupKeyExt>| {
                let processor = processor.clone();
                async move {
                    matches!(
                        accept_operation(&processor, &operation).await,
                        Ok(GroupKeyAdmission::Inserted { .. })
                    )
                }
            },
        )
        .await
    }

    /// Author a fresh dispatch, then push it to connected peers.
    ///
    /// Storing is what makes it survive, publishing is what makes it arrive
    /// now; a failed publish still leaves it stored for reconciliation.
    pub async fn publish(
        &self,
        joined: &JoinedSpace<GroupKeyExt>,
        dispatch: &GroupSessionDispatch,
    ) -> Result<Operation<GroupKeyExt>, GroupKeyLaneError> {
        let operation = self.author_dispatch(dispatch).await?;
        joined.publish(operation.clone())?;
        Ok(operation)
    }
}

#[cfg(test)]
mod tests {
    use identity::InMemoryProvider;
    use muniment::MemoryBackend;
    use pollster::block_on;

    use super::*;
    use crate::{GroupCryptoError, GroupPrekeyBundle};

    const GROUP: GroupSessionId = GroupSessionId([0x4b; 32]);

    struct Member {
        identity: InMemoryProvider,
        backend: MemoryBackend,
        lane: GroupKeyLane<MemoryBackend>,
        session: GroupSession,
        /// What the last drain's persist step saved.
        persisted: Vec<u8>,
    }

    impl Member {
        fn new(seed: u8) -> (Self, GroupPrekeyBundle) {
            let identity = InMemoryProvider::from_seed([seed; 32]);
            let backend = MemoryBackend::new();
            let lane = GroupKeyLane::for_identity(backend.clone(), GROUP, &identity).unwrap();
            let (session, prekey) = GroupSession::new(GROUP, &identity).unwrap();
            let persisted = session.to_bytes().unwrap();
            let member = Self {
                identity,
                backend,
                lane,
                session,
                persisted,
            };
            (member, prekey)
        }

        fn receive<'a>(&self, operations: impl IntoIterator<Item = &'a Operation<GroupKeyExt>>) {
            for operation in operations {
                block_on(self.lane.accept(operation)).unwrap();
            }
        }

        fn drain(&mut self) -> GroupKeyDrain {
            let mut persisted = None;
            let drain = block_on(self.lane.drain_ready(&mut self.session, |session| {
                persisted = Some(session.to_bytes()?);
                Ok::<_, GroupSessionError>(())
            }))
            .unwrap();
            if let Some(bytes) = persisted {
                self.persisted = bytes;
            }
            drain
        }
    }

    struct Founded {
        a: Member,
        b: Member,
        c: Member,
        adds: Vec<Operation<GroupKeyExt>>,
    }

    fn hashes<'a>(
        operations: impl IntoIterator<Item = &'a Operation<GroupKeyExt>>,
    ) -> Vec<[u8; 32]> {
        operations
            .into_iter()
            .map(|operation| *operation.hash.as_bytes())
            .collect()
    }

    fn applied(drain: &GroupKeyDrain) -> Vec<[u8; 32]> {
        drain.applied.iter().map(|frame| frame.operation).collect()
    }

    /// A founds alone, as Turnstone does, then invites B and C. Each welcome
    /// travels by invitation; the lane carries the same dispatch to everyone.
    /// B registers only A's pre-key, as an invitee does.
    fn founded() -> Founded {
        let (mut a, a_prekey) = Member::new(0xa1);
        let (mut b, b_prekey) = Member::new(0xb2);
        let (mut c, c_prekey) = Member::new(0xc3);
        a.session.register_prekey(&b_prekey).unwrap();
        a.session.register_prekey(&c_prekey).unwrap();
        b.session.register_prekey(&a_prekey).unwrap();
        c.session.register_prekey(&a_prekey).unwrap();
        a.session.create(&[]).unwrap();
        let mut adds = Vec::new();
        for joiner in [&mut b, &mut c] {
            let dispatch = a.session.add(joiner.session.member()).unwrap();
            let welcome = dispatch.direct_for(joiner.session.member());
            joiner
                .session
                .process(a.lane.stable_author(), &dispatch.control, welcome)
                .unwrap();
            adds.push(block_on(a.lane.author_dispatch(&dispatch)).unwrap());
        }
        Founded { a, b, c, adds }
    }

    /// Every member has drained the two add records.
    fn caught_up() -> Founded {
        let mut founded = founded();
        founded.b.receive(&founded.adds);
        founded.c.receive(&founded.adds);
        founded.a.drain();
        founded.b.drain();
        founded.c.drain();
        founded
    }

    /// A removes C and rotates, authoring both dispatches on its lane.
    fn remove_and_rotate(
        founded: &mut Founded,
    ) -> (Operation<GroupKeyExt>, Operation<GroupKeyExt>) {
        let removal = founded
            .a
            .session
            .remove(founded.c.session.member())
            .unwrap();
        let removal = block_on(founded.a.lane.author_dispatch(&removal)).unwrap();
        let rotation = founded.a.session.update().unwrap();
        let rotation = block_on(founded.a.lane.author_dispatch(&rotation)).unwrap();
        (removal, rotation)
    }

    /// Hosts copy this value into their overlay tags; it must never move.
    #[test]
    fn the_sync_topic_is_pinned_and_is_the_topic_records_store_under() {
        let expected = "c9e8ae9fc574d4a2612db89bf5e15958e57ec31f2fce04701fa7f1ca55802236";
        let topic = group_key_sync_topic(GROUP);
        assert_eq!(hex::encode(topic), expected);
        let derived =
            blake3::hash(&[b"stickleback/group-key-topic/v1/".as_slice(), &GROUP.0].concat());
        assert_eq!(topic, *derived.as_bytes());

        let f = founded();
        let keys = block_on(f.a.lane.sync_store().backend().list("topic/")).unwrap();
        let prefix = format!("topic/{}/", hex::encode(topic));
        assert!(
            !keys.is_empty() && keys.iter().all(|key| key.starts_with(&prefix)),
            "{keys:?}"
        );
    }

    #[test]
    fn founded_members_open_a_message_sealed_before_removal() {
        let mut f = founded();
        assert_eq!(f.a.lane.stable_author(), f.a.session.personae_root());
        f.b.receive(&f.adds);
        f.c.receive(&f.adds);

        let b = f.b.drain();
        assert_eq!(applied(&b), hashes([&f.adds[1]]), "B applies C's add");
        assert_eq!(
            b.superseded,
            hashes([&f.adds[0]]),
            "B's own welcome came by invitation"
        );
        let c = f.c.drain();
        assert!(c.applied.is_empty());
        assert_eq!(c.superseded, hashes(&f.adds));
        let a = f.a.drain();
        assert!(a.applied.is_empty());
        assert_eq!(a.superseded, hashes(&f.adds), "A authored both");
        assert_eq!(
            f.b.session.members().unwrap(),
            f.a.session.members().unwrap()
        );

        let sealed = f.a.session.seal_random(b"before removal").unwrap();
        for member in [&f.a, &f.b, &f.c] {
            assert_eq!(member.session.open(&sealed).unwrap(), b"before removal");
        }
    }

    #[test]
    fn rotation_after_removal_reaches_b_and_not_c() {
        let mut f = caught_up();
        let before = f.a.session.seal_random(b"before removal").unwrap();
        let (removal, rotation) = remove_and_rotate(&mut f);
        f.b.receive([&removal, &rotation]);
        f.c.receive([&removal, &rotation]);

        let b = f.b.drain();
        assert_eq!(applied(&b), hashes([&removal, &rotation]));
        assert!(
            b.applied
                .iter()
                .all(|frame| frame.process.installed_epochs.len() == 1)
        );
        let c = f.c.drain();
        assert_eq!(applied(&c), hashes([&removal, &rotation]));
        assert!(
            c.applied
                .iter()
                .all(|frame| frame.process.installed_epochs.is_empty())
        );
        assert!(b.refused.is_empty() && c.refused.is_empty());

        let after = f.a.session.seal_random(b"after rotation").unwrap();
        assert_eq!(f.b.session.open(&after).unwrap(), b"after rotation");
        assert!(matches!(
            f.c.session.open(&after),
            Err(GroupSessionError::Crypto(GroupCryptoError::UnknownEpoch(_)))
        ));
        // Positive control in the same run: C still opens what it was sent.
        assert_eq!(f.c.session.open(&before).unwrap(), b"before removal");
        assert_eq!(f.b.session.current_epoch(), f.a.session.current_epoch());
        assert_ne!(f.c.session.current_epoch(), f.a.session.current_epoch());
    }

    #[test]
    fn out_of_order_delivery_applies_in_causal_order() {
        let mut f = caught_up();
        let (removal, rotation) = remove_and_rotate(&mut f);

        assert_eq!(
            block_on(f.b.lane.accept(&rotation)).unwrap(),
            GroupKeyAdmission::Parked
        );
        let waiting = f.b.drain();
        assert!(waiting.applied.is_empty() && waiting.refused.is_empty());
        assert_eq!(waiting.pending, hashes([&rotation]));

        assert_eq!(
            block_on(f.b.lane.accept(&removal)).unwrap(),
            GroupKeyAdmission::Inserted { released: 1 }
        );
        let drain = f.b.drain();
        assert_eq!(applied(&drain), hashes([&removal, &rotation]));
        assert!(drain.applied[0].control.sequence < drain.applied[1].control.sequence);
        assert!(drain.pending.is_empty());
        let after = f.a.session.seal_random(b"after rotation").unwrap();
        assert_eq!(f.b.session.open(&after).unwrap(), b"after rotation");
    }

    #[test]
    fn a_frame_ahead_of_its_author_sequence_stays_pending() {
        let mut f = caught_up();
        f.a.session.update().unwrap(); // never carried on the lane
        let rotation = f.a.session.update().unwrap();
        let rotation = block_on(f.a.lane.author_dispatch(&rotation)).unwrap();
        f.b.receive([&rotation]);
        let epoch = f.b.session.current_epoch();
        for _ in 0..2 {
            let drain = f.b.drain();
            assert!(drain.applied.is_empty() && drain.refused.is_empty());
            assert_eq!(drain.pending, hashes([&rotation]));
        }
        assert_eq!(f.b.session.current_epoch(), epoch);
    }

    #[test]
    fn replayed_records_are_ignored() {
        let mut f = caught_up();
        let (removal, rotation) = remove_and_rotate(&mut f);
        f.b.receive([&removal, &rotation]);
        assert_eq!(f.b.drain().applied.len(), 2);
        let epoch = f.b.session.current_epoch();

        for operation in f.adds.iter().chain([&removal, &rotation]) {
            assert_eq!(
                block_on(f.b.lane.accept(operation)).unwrap(),
                GroupKeyAdmission::Duplicate
            );
        }
        assert_eq!(f.b.drain(), GroupKeyDrain::default());
        assert_eq!(f.b.session.current_epoch(), epoch);
    }

    #[test]
    fn a_reopened_store_does_not_reprocess_frames() {
        let mut f = caught_up();
        let (removal, rotation) = remove_and_rotate(&mut f);
        f.b.receive([&removal, &rotation]);
        assert_eq!(f.b.drain().applied.len(), 2);
        let after = f.a.session.seal_random(b"after reopen").unwrap();

        // Drop B's lane and live session; reopen both from what was persisted.
        let Member {
            identity,
            backend,
            persisted,
            ..
        } = f.b;
        let lane = GroupKeyLane::for_identity(backend, GROUP, &identity).unwrap();
        let mut session = GroupSession::from_bytes(&persisted).unwrap();
        let reopened =
            block_on(lane.drain_ready(&mut session, |_| Ok::<_, GroupSessionError>(()))).unwrap();
        assert_eq!(reopened, GroupKeyDrain::default());
        assert_eq!(session.open(&after).unwrap(), b"after reopen");

        // Positive control: without settled marks the same records are judged.
        let unmarked = GroupKeyLane::for_identity(MemoryBackend::new(), GROUP, &identity).unwrap();
        for operation in f.adds.iter().chain([&removal, &rotation]) {
            block_on(unmarked.accept(operation)).unwrap();
        }
        let judged =
            block_on(unmarked.drain_ready(&mut session, |_| Ok::<_, GroupSessionError>(())))
                .unwrap();
        assert_eq!(judged.superseded.len(), 4);
    }

    #[test]
    fn a_record_whose_author_attestation_does_not_verify_is_refused() {
        let mut f = caught_up();
        let (removal, _) = remove_and_rotate(&mut f);
        let (_, dispatch) = open_record(&removal, GROUP, &GroupKeyLimits::default()).unwrap();
        let (mallory, _) = Member::new(0xd4);
        let other_group = mallory
            .identity
            .attest_derived_key(&group_key_writer_salt(GroupSessionId([0x99; 32])))
            .unwrap();
        let borrowed = f.a.lane.attestation.clone();

        for (attestation, code) in [
            (other_group, "invalid-writer-attestation"),
            (borrowed, "writer-attestation-mismatch"),
        ] {
            let forged = mallory
                .lane
                .build_operation(&dispatch, attestation, Vec::new(), 0, None)
                .unwrap();
            let refusal = block_on(f.b.lane.accept(&forged)).unwrap_err();
            assert!(
                matches!(
                    &refusal,
                    GroupKeyLaneError::Process(ProcessError::Rejected(reject)) if reject.code == code
                ),
                "{refusal}"
            );
            assert!(!block_on(f.b.lane.store.has_operation(&forged.hash)).unwrap());
        }
        // Positive control: the genuine record is admitted by the same replica.
        assert_eq!(
            block_on(f.b.lane.accept(&removal)).unwrap(),
            GroupKeyAdmission::Inserted { released: 0 }
        );
        assert_eq!(applied(&f.b.drain()), hashes([&removal]));
    }
}
