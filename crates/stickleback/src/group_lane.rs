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

use identity::{IdentityError, IdentityProvider};
use insigne::DerivedKeyAttestation;
use muniment::{Backend, StoreError, WriteOp};
use p2panda_core::cbor::{decode_cbor, decode_cbor_strict, encode_cbor};
use p2panda_core::{Body, Hash, Header, Operation, SigningKey, Topic, VerifyingKey};
use p2panda_store::topics::TopicStore;
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;

use crate::drop::DropRecord;
use crate::{
    Admission, CausalEntry, CausalError, CausalLimits, GroupControlFrame, GroupControlId,
    GroupDirectFrame, GroupRecipientId, GroupSession, GroupSessionDispatch, GroupSessionError,
    GroupSessionId, GroupSessionProcess, JoinError, MessageSession, MunimentStore, OperationPolicy,
    OperationProcessor, ProcessError, Reject, StoreTarget, WriterBindingError, author_head,
    causal_projection, decode_operation_record, observed_frontier, operation_record,
    stable_writer_subject, validate_causal_metadata,
};

/// A session the lane can drain frames into: the durable data scheme or the
/// forward-secure message scheme.
pub trait GroupKeySession: Sized {
    fn group(&self) -> GroupSessionId;
    fn member(&self) -> GroupRecipientId;
    /// Apply one authenticated, causally ordered frame.
    ///
    /// # Errors
    /// The session's own refusal; it leaves the session unchanged.
    fn process_frame(
        &mut self,
        authenticated_author_root: [u8; 32],
        control: &GroupControlFrame,
        direct: Option<&GroupDirectFrame>,
    ) -> Result<GroupSessionProcess, GroupSessionError>;
    /// A working copy, for transactional draining.
    ///
    /// # Errors
    /// The state could not be re-encoded.
    fn try_clone(&self) -> Result<Self, GroupSessionError>;
}

impl GroupKeySession for GroupSession {
    fn group(&self) -> GroupSessionId {
        GroupSession::group(self)
    }
    fn member(&self) -> GroupRecipientId {
        GroupSession::member(self)
    }
    fn process_frame(
        &mut self,
        root: [u8; 32],
        control: &GroupControlFrame,
        direct: Option<&GroupDirectFrame>,
    ) -> Result<GroupSessionProcess, GroupSessionError> {
        self.process(root, control, direct)
    }
    fn try_clone(&self) -> Result<Self, GroupSessionError> {
        GroupSession::try_clone(self)
    }
}

impl GroupKeySession for MessageSession {
    fn group(&self) -> GroupSessionId {
        MessageSession::group(self)
    }
    fn member(&self) -> GroupRecipientId {
        MessageSession::member(self)
    }
    fn process_frame(
        &mut self,
        root: [u8; 32],
        control: &GroupControlFrame,
        direct: Option<&GroupDirectFrame>,
    ) -> Result<GroupSessionProcess, GroupSessionError> {
        self.process(root, control, direct)
    }
    fn try_clone(&self) -> Result<Self, GroupSessionError> {
        MessageSession::try_clone(self)
    }
}

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
    let record: GroupKeyRecord = decode_cbor_strict(body.to_bytes().as_slice())
        .map_err(|error| invalid(error.to_string()))?;
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
        }
        Ok(_) => return Ok(GroupKeyAdmission::Duplicate),
        Err(ProcessError::MissingPredecessor { .. }) => {}
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
        }
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

/// Admission capability for a group-key lane, without its signing material.
///
/// A network task can clone this receiver and feed arrivals through the same
/// policy-before-insert and parking path as [`GroupKeyLane::accept`].
#[derive(Clone)]
pub struct GroupKeyReceiver<B: Backend + Clone> {
    processor: OperationProcessor<B, GroupKeyExt, GroupKeyPolicy>,
}

impl<B: Backend + Clone> GroupKeyReceiver<B> {
    /// Authenticate, validate and store one received group-key operation.
    pub async fn accept(
        &self,
        operation: &Operation<GroupKeyExt>,
    ) -> Result<GroupKeyAdmission, GroupKeyLaneError> {
        accept_operation(&self.processor, operation).await
    }
}

#[cfg(test)]
#[path = "group_lane/tests.rs"]
mod tests;

#[cfg(test)]
#[path = "group_lane/message_tests.rs"]
mod message_tests;

#[path = "group_lane/operations.rs"]
mod operations;
