// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Serializable DCGKA group sessions and addressed control payloads.
//!
//! These payloads are carrier-neutral but not authority-bearing. A domain such
//! as Gemot still authenticates and causally orders each control frame before
//! passing it here. Recipient ids are derived from the DCGKA identity key,
//! while a group-scoped Personae-derived key signs that recipient and its
//! pre-key bundle. A host therefore cannot relabel either half of the binding.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::convert::Infallible;
use std::fmt;

use identity::{Ed25519PublicKey, Ed25519Signature, IdentityProvider};
use insigne::CheckFault;
use insigne::DerivedKeyAttestation;
use p2panda_core::cbor::{decode_cbor, decode_cbor_strict, encode_cbor};
use p2panda_encryption::Rng;
use p2panda_encryption::crypto::x25519::SecretKey;
use p2panda_encryption::data_scheme::GroupSecretId;
use p2panda_encryption::data_scheme::dcgka::{
    ControlMessage, Dcgka, DcgkaState, DirectMessage, GroupSecretOutput, OperationOutput,
    ProcessInput,
};
use p2panda_encryption::key_bundle::{Lifetime, LongTermKeyBundle};
use p2panda_encryption::key_manager::KeyManager;
use p2panda_encryption::key_registry::KeyRegistry;
use p2panda_encryption::traits::{
    GroupMembership, IdentityHandle, KeyBundle, OperationId, PreKeyManager,
};
use serde::{Deserialize, Serialize};

use crate::{DataKeyring, GroupCiphertext, GroupCryptoError};

const GROUP_SESSION_VERSION: u16 = 1;
const GROUP_PREKEY_VERSION: u16 = 1;
pub(crate) const GROUP_CONTROL_VERSION: u16 = 1;
pub(crate) const GROUP_DIRECT_VERSION: u16 = 1;
const GROUP_PREKEY_IDENTITY_DOMAIN: &[u8] = b"stickleback/group-prekey-identity/v1";

/// Stable id for one encrypted group session.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct GroupSessionId(pub [u8; 32]);

impl fmt::Display for GroupSessionId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&hex::encode(self.0))
    }
}

/// Self-authenticating address of one DCGKA recipient.
///
/// This is the X25519 identity public key carried by the recipient's signed
/// pre-key bundle. It is deliberately distinct from a Personae root id.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct GroupRecipientId(pub [u8; 32]);

impl fmt::Display for GroupRecipientId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&hex::encode(self.0))
    }
}

impl IdentityHandle for GroupRecipientId {}

/// Per-author control sequence used by authenticated causal delivery.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct GroupControlId {
    pub author: GroupRecipientId,
    pub sequence: u64,
}

impl OperationId for GroupControlId {}

impl fmt::Display for GroupControlId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}#{}", self.author, self.sequence)
    }
}

/// Inspectable group-control grammar carried inside a domain operation.
///
/// The tags `Ack` and `AddAck` belonged to the message scheme, removed on
/// 2026-11-14. They stay reserved and are never reused: a frame carrying one
/// decodes to [`GroupSessionError::RetiredControlAction`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "WireControlAction")]
pub enum GroupControlAction {
    Create {
        initial_members: Vec<GroupRecipientId>,
    },
    Update,
    Add {
        added: GroupRecipientId,
    },
    Remove {
        removed: GroupRecipientId,
    },
}

/// Decode-side grammar: the live actions plus the reserved, retired tags.
#[derive(Deserialize)]
enum WireControlAction {
    Create {
        initial_members: Vec<GroupRecipientId>,
    },
    Update,
    Add {
        added: GroupRecipientId,
    },
    Remove {
        removed: GroupRecipientId,
    },
    Ack(serde::de::IgnoredAny),
    AddAck(serde::de::IgnoredAny),
}

/// A retired control-action tag, named in the decode error.
#[derive(Debug)]
pub(crate) struct RetiredTag(&'static str);

impl fmt::Display for RetiredTag {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{RETIRED_ACTION_PREFIX}{}", self.0)
    }
}

const RETIRED_ACTION_PREFIX: &str = "retired group control action ";

impl TryFrom<WireControlAction> for GroupControlAction {
    type Error = RetiredTag;

    fn try_from(value: WireControlAction) -> Result<Self, Self::Error> {
        Ok(match value {
            WireControlAction::Create { initial_members } => Self::Create { initial_members },
            WireControlAction::Update => Self::Update,
            WireControlAction::Add { added } => Self::Add { added },
            WireControlAction::Remove { removed } => Self::Remove { removed },
            WireControlAction::Ack(_) => return Err(RetiredTag("Ack")),
            WireControlAction::AddAck(_) => return Err(RetiredTag("AddAck")),
        })
    }
}

/// Map a decode failure, naming a retired control-action tag when it was one.
fn control_decode_error(detail: String) -> GroupSessionError {
    for tag in ["AddAck", "Ack"] {
        if detail.contains(&format!("{RETIRED_ACTION_PREFIX}{tag}")) {
            return GroupSessionError::RetiredControlAction(tag);
        }
    }
    GroupSessionError::Decode(detail)
}

/// Broadcast half of one DCGKA operation.
///
/// The containing domain operation must authenticate the author and provide
/// causal ordering. This frame only carries the crypto transition.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupControlFrame {
    pub(crate) version: u16,
    pub group: GroupSessionId,
    pub id: GroupControlId,
    pub action: GroupControlAction,
}

impl GroupControlFrame {
    pub fn to_bytes(&self) -> Result<Vec<u8>, GroupSessionError> {
        encode_cbor(self).map_err(|error| GroupSessionError::Encode(error.to_string()))
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, GroupSessionError> {
        let frame: Self = decode_cbor_strict(bytes)
            .map_err(|error| control_decode_error(error.to_string()))?;
        frame.validate()?;
        Ok(frame)
    }

    pub(crate) fn validate(&self) -> Result<(), GroupSessionError> {
        if self.version != GROUP_CONTROL_VERSION {
            return Err(GroupSessionError::UnsupportedControlVersion(self.version));
        }
        Ok(())
    }
}

/// Recipient-bound half of one DCGKA operation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupDirectFrame {
    pub(crate) version: u16,
    pub group: GroupSessionId,
    pub control: GroupControlId,
    pub recipient: GroupRecipientId,
    pub(crate) payload: Vec<u8>,
}

impl GroupDirectFrame {
    pub fn to_bytes(&self) -> Result<Vec<u8>, GroupSessionError> {
        encode_cbor(self).map_err(|error| GroupSessionError::Encode(error.to_string()))
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, GroupSessionError> {
        let frame: Self = decode_cbor_strict(bytes)
            .map_err(|error| GroupSessionError::Decode(error.to_string()))?;
        frame.validate()?;
        Ok(frame)
    }

    pub(crate) fn validate(&self) -> Result<(), GroupSessionError> {
        if self.version != GROUP_DIRECT_VERSION {
            return Err(GroupSessionError::UnsupportedDirectVersion(self.version));
        }
        Ok(())
    }
}

/// Broadcast control plus its separately routable recipient messages.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupSessionDispatch {
    pub control: GroupControlFrame,
    pub direct: Vec<GroupDirectFrame>,
}

impl GroupSessionDispatch {
    /// Return the sole direct message addressed to `recipient`, when present.
    pub fn direct_for(&self, recipient: GroupRecipientId) -> Option<&GroupDirectFrame> {
        self.direct
            .iter()
            .find(|message| message.recipient == recipient)
    }
}

/// Publishable long-term pre-key bundle for one group recipient.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupPrekeyBundle {
    version: u16,
    pub group: GroupSessionId,
    pub recipient: GroupRecipientId,
    payload: Vec<u8>,
    attestation: DerivedKeyAttestation,
    identity_signature: Vec<u8>,
}

impl GroupPrekeyBundle {
    /// Personae root that authenticated this group recipient and pre-key.
    pub fn personae_root(&self) -> Result<[u8; 32], GroupSessionError> {
        self.verify_identity()
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, GroupSessionError> {
        encode_cbor(self).map_err(|error| GroupSessionError::Encode(error.to_string()))
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, GroupSessionError> {
        let bundle: Self = decode_cbor_strict(bytes)
            .map_err(|error| GroupSessionError::Decode(error.to_string()))?;
        bundle.decode_bundle()?;
        Ok(bundle)
    }

    fn decode_bundle(&self) -> Result<(LongTermKeyBundle, [u8; 32]), GroupSessionError> {
        if self.version != GROUP_PREKEY_VERSION {
            return Err(GroupSessionError::UnsupportedPrekeyVersion(self.version));
        }
        let bundle: LongTermKeyBundle = decode_cbor_strict(self.payload.as_slice())
            .map_err(|error| GroupSessionError::Decode(error.to_string()))?;
        let derived = GroupRecipientId(bundle.identity_key().to_bytes());
        if self.recipient != derived {
            return Err(GroupSessionError::PrekeyRecipientMismatch {
                claimed: self.recipient,
                derived,
            });
        }
        let root = self.verify_identity()?;
        Ok((bundle, root))
    }

    fn verify_identity(&self) -> Result<[u8; 32], GroupSessionError> {
        let salt = group_prekey_identity_salt(self.group);
        let attestation = self
            .attestation
            .check(&salt)
            .map_err(GroupSessionError::InvalidPersonaeAttestation)?;
        let root = *attestation.master();
        let derived = Ed25519PublicKey::from_bytes(attestation.derived())
            .map_err(|error| GroupSessionError::Identity(error.to_string()))?;
        let signature_bytes: [u8; 64] = self
            .identity_signature
            .as_slice()
            .try_into()
            .map_err(|_| GroupSessionError::InvalidPersonaeSignature)?;
        let signature = Ed25519Signature::from_bytes(&signature_bytes);
        if !derived.verify(&self.identity_signing_bytes(), &signature) {
            return Err(GroupSessionError::InvalidPersonaeSignature);
        }
        Ok(root)
    }

    fn identity_signing_bytes(&self) -> Vec<u8> {
        let mut message = Vec::with_capacity(
            GROUP_PREKEY_IDENTITY_DOMAIN.len() + 2 + 32 + 32 + 8 + self.payload.len(),
        );
        message.extend_from_slice(GROUP_PREKEY_IDENTITY_DOMAIN);
        message.extend_from_slice(&self.version.to_le_bytes());
        message.extend_from_slice(&self.group.0);
        message.extend_from_slice(&self.recipient.0);
        message.extend_from_slice(&(self.payload.len() as u64).to_le_bytes());
        message.extend_from_slice(&self.payload);
        message
    }

    fn digest(&self) -> Result<[u8; 32], GroupSessionError> {
        let bytes =
            encode_cbor(self).map_err(|error| GroupSessionError::Encode(error.to_string()))?;
        Ok(*blake3::hash(&bytes).as_bytes())
    }
}

fn group_prekey_identity_salt(group: GroupSessionId) -> Vec<u8> {
    let mut salt = Vec::with_capacity(GROUP_PREKEY_IDENTITY_DOMAIN.len() + 32);
    salt.extend_from_slice(GROUP_PREKEY_IDENTITY_DOMAIN);
    salt.extend_from_slice(&group.0);
    salt
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct OrderedMembershipState {
    my_id: GroupRecipientId,
    members: BTreeSet<GroupRecipientId>,
}

#[derive(Clone, Debug)]
struct OrderedMembership;

impl OrderedMembership {
    fn init(my_id: GroupRecipientId) -> OrderedMembershipState {
        OrderedMembershipState {
            my_id,
            members: BTreeSet::new(),
        }
    }
}

impl GroupMembership<GroupRecipientId, GroupControlId> for OrderedMembership {
    type State = OrderedMembershipState;
    type Error = Infallible;

    fn create(
        my_id: GroupRecipientId,
        initial_members: &[GroupRecipientId],
    ) -> Result<Self::State, Self::Error> {
        Ok(OrderedMembershipState {
            my_id,
            members: initial_members.iter().copied().collect(),
        })
    }

    fn from_welcome(
        my_id: GroupRecipientId,
        mut history: Self::State,
    ) -> Result<Self::State, Self::Error> {
        history.my_id = my_id;
        history.members.insert(my_id);
        Ok(history)
    }

    fn add(
        mut state: Self::State,
        _adder: GroupRecipientId,
        added: GroupRecipientId,
        _operation_id: GroupControlId,
    ) -> Result<Self::State, Self::Error> {
        state.members.insert(added);
        Ok(state)
    }

    fn remove(
        mut state: Self::State,
        _remover: GroupRecipientId,
        removed: &GroupRecipientId,
        _operation_id: GroupControlId,
    ) -> Result<Self::State, Self::Error> {
        state.members.remove(removed);
        Ok(state)
    }

    fn members(state: &Self::State) -> Result<HashSet<GroupRecipientId>, Self::Error> {
        Ok(state.members.iter().copied().collect())
    }
}

type SessionDcgka = DcgkaState<
    GroupRecipientId,
    GroupControlId,
    KeyRegistry<GroupRecipientId>,
    OrderedMembership,
    KeyManager,
>;
type SessionDirect = DirectMessage<GroupRecipientId, GroupControlId, OrderedMembership>;

/// Result of applying one authenticated, causally ordered control frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GroupSessionProcess {
    /// Epochs learned from the addressed direct message, oldest first.
    pub installed_epochs: Vec<GroupSecretId>,
}

/// Durable state for one DCGKA group member.
///
/// Persist [`GroupSession::to_bytes`] through Personae's sealed-record store.
/// The encoded state contains long-term private keys, ratchet state, and data
/// epochs and must never be written to plaintext storage.
#[derive(Debug, Serialize, Deserialize)]
pub struct GroupSession {
    version: u16,
    group: GroupSessionId,
    member: GroupRecipientId,
    personae_root: [u8; 32],
    dcgka: Option<SessionDcgka>,
    keyring: DataKeyring,
    next_local_sequence: u64,
    next_remote_sequence: BTreeMap<GroupRecipientId, u64>,
    prekey_digests: BTreeMap<GroupRecipientId, [u8; 32]>,
    prekey_roots: BTreeMap<GroupRecipientId, [u8; 32]>,
}

impl From<ControlMessage<GroupRecipientId>> for GroupControlAction {
    fn from(value: ControlMessage<GroupRecipientId>) -> Self {
        match value {
            ControlMessage::Create { initial_members } => Self::Create { initial_members },
            ControlMessage::Update => Self::Update,
            ControlMessage::Add { added } => Self::Add { added },
            ControlMessage::Remove { removed } => Self::Remove { removed },
        }
    }
}

impl From<GroupControlAction> for ControlMessage<GroupRecipientId> {
    fn from(value: GroupControlAction) -> Self {
        match value {
            GroupControlAction::Create { initial_members } => Self::Create { initial_members },
            GroupControlAction::Update => Self::Update,
            GroupControlAction::Add { added } => Self::Add { added },
            GroupControlAction::Remove { removed } => Self::Remove { removed },
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum GroupSessionError {
    #[error("group session has no DCGKA state")]
    MissingState,
    #[error("group session already has membership state")]
    GroupAlreadyCreated,
    /// The frame carries a control-action tag retired with the message scheme
    /// (2026-11-14). The tag stays reserved and is never reused.
    #[error("unsupported group control action {0}: retired with the message scheme")]
    RetiredControlAction(&'static str),
    #[error("local group recipient is not an active member")]
    LocalMemberInactive,
    #[error("cannot remove the local author with its own rotation")]
    CannotRemoveSelf,
    #[error("group recipient {0} is already present")]
    MemberAlreadyPresent(GroupRecipientId),
    #[error("group recipient {0} is not a current member")]
    UnknownMember(GroupRecipientId),
    #[error("cannot forget the current data epoch {}", hex::encode(.0))]
    CurrentEpochNotForgettable(GroupSecretId),
    #[error("group session does not hold data epoch {}", hex::encode(.0))]
    EpochNotHeld(GroupSecretId),
    #[error("missing authenticated pre-key for group recipient {0}")]
    MissingPrekey(GroupRecipientId),
    #[error("conflicting pre-key for group recipient {0}")]
    ConflictingPrekey(GroupRecipientId),
    #[error("pre-key recipient mismatch: claimed {claimed}, derived {derived}")]
    PrekeyRecipientMismatch {
        claimed: GroupRecipientId,
        derived: GroupRecipientId,
    },
    #[error("group pre-key has an invalid Personae derived-key attestation: {0}")]
    InvalidPersonaeAttestation(CheckFault),
    #[error("group pre-key has an invalid Personae binding signature")]
    InvalidPersonaeSignature,
    #[error("group session has inconsistent pre-key identity bindings")]
    InvalidPrekeyBindings,
    #[error("authenticated Personae root does not own control author {author}")]
    ControlAuthorRootMismatch { author: GroupRecipientId },
    #[error("Personae identity boundary: {0}")]
    Identity(String),
    #[error("wrong group session: expected {expected}, got {actual}")]
    WrongGroup {
        expected: GroupSessionId,
        actual: GroupSessionId,
    },
    #[error("addressed direct message for {0} is required")]
    MissingAddressedDirect(GroupRecipientId),
    #[error("group recipient {0} was given an unexpected direct message")]
    UnexpectedDirect(GroupRecipientId),
    #[error("direct message names a different control frame")]
    DirectControlMismatch,
    #[error("direct recipient mismatch: expected {expected}, got {actual}")]
    DirectRecipientMismatch {
        expected: GroupRecipientId,
        actual: GroupRecipientId,
    },
    #[error("unexpected control sequence for {author}: expected {expected}, got {actual}")]
    UnexpectedSequence {
        author: GroupRecipientId,
        expected: u64,
        actual: u64,
    },
    #[error("control sequence exhausted for {0}")]
    SequenceExhausted(GroupRecipientId),
    #[error("group session recipient state does not match its public recipient")]
    StateRecipientMismatch,
    #[error("local sequence state mismatch: expected {expected}, got {actual}")]
    InvalidLocalSequence { expected: u64, actual: u64 },
    #[error("unsupported group-session version {0}")]
    UnsupportedSessionVersion(u16),
    #[error("unsupported group pre-key version {0}")]
    UnsupportedPrekeyVersion(u16),
    #[error("unsupported group control version {0}")]
    UnsupportedControlVersion(u16),
    #[error("unsupported group direct-message version {0}")]
    UnsupportedDirectVersion(u16),
    #[error("group-session encryption engine: {0}")]
    Engine(String),
    #[error("encode group-session state: {0}")]
    Encode(String),
    #[error("decode group-session state: {0}")]
    Decode(String),
    #[error(transparent)]
    Crypto(#[from] GroupCryptoError),
}

#[cfg(test)]
#[path = "group_session/tests.rs"]
mod tests;

#[path = "group_session/operations.rs"]
mod operations;
