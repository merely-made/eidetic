// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Forward-secure group sessions: p2panda's message scheme behind the same
//! frames, lane and sequencing as [`GroupSession`](crate::GroupSession).
//!
//! Every member keeps one sending ratchet and one receiving ratchet per other
//! author. Each message key is deleted as it is used, so a stolen session
//! state reads nothing sent before the theft, apart from messages skipped
//! within [`MessageLimits`]. A member who is removed, or who joins later,
//! never holds the ratchets of the period they were outside.
//!
//! Key agreement is answered with acknowledgements, which the session hands
//! back as [`GroupSessionProcess::outbound`](crate::GroupSessionProcess) for
//! the host to author on the lane. The domain must authenticate every frame
//! and deliver it after the frames it causally follows, application messages
//! included: a ciphertext opened before the control frame that keyed its
//! chain fails, and may simply be retried afterwards.
//!
//! The scheme has no cross-device story: one recipient is one device, and a
//! second device is a second recipient with its own pre-keys.

use std::collections::{BTreeMap, BTreeSet};

use identity::{Ed25519PublicKey, Ed25519Signature, IdentityProvider};
use insigne::DerivedKeyAttestation;
use p2panda_core::cbor::{decode_cbor, decode_cbor_strict, encode_cbor};
use p2panda_encryption::Rng;
use p2panda_encryption::crypto::x25519::SecretKey;
use p2panda_encryption::key_bundle::{Lifetime, OneTimeKeyBundle};
use p2panda_encryption::key_manager::KeyManager;
use p2panda_encryption::key_registry::KeyRegistry;
use p2panda_encryption::message_scheme::dcgka::Dcgka;
use p2panda_encryption::message_scheme::{ControlMessage, DirectMessage};
use p2panda_encryption::traits::{KeyBundle, PreKeyManager};
use serde::{Deserialize, Serialize};

use crate::group_crypto::{
    MessageDcgka, MessageDgm, generate_onetime_bundles, register_onetime_bundles,
};
use crate::{
    GroupControlAction, GroupControlId, GroupRecipientId, GroupSessionError, GroupSessionId,
    MessageLimits,
};

#[path = "message_session/operations.rs"]
mod operations;
#[cfg(test)]
#[path = "message_session/tests.rs"]
mod tests;

pub(crate) const MESSAGE_SESSION_VERSION: u16 = 1;
const MESSAGE_PREKEY_VERSION: u16 = 1;
const MESSAGE_PREKEY_IDENTITY_DOMAIN: &[u8] = b"stickleback/message-prekey-identity/v1";
/// One-time pre-keys in a batch for one peer. First contact spends one, and a
/// key spent twice is refused, so a batch is addressed to a single peer.
pub const MESSAGE_PREKEY_BATCH: usize = 4;

pub(crate) type MessageDirect = DirectMessage<GroupRecipientId, GroupControlId, MessageDgm>;
pub(crate) type MessageControl = ControlMessage<GroupRecipientId, GroupControlId>;

/// One-time pre-keys one member makes for one peer to spend, bound to a group-scoped Personae key as [`GroupPrekeyBundle`] is.
///
/// [`GroupPrekeyBundle`]: crate::GroupPrekeyBundle
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessagePrekeyBundle {
    version: u16,
    pub group: GroupSessionId,
    pub recipient: GroupRecipientId,
    /// The only member who may spend these keys.
    pub for_peer: GroupRecipientId,
    payload: Vec<u8>,
    attestation: DerivedKeyAttestation,
    identity_signature: Vec<u8>,
}

impl MessagePrekeyBundle {
    /// Personae root that authenticated this recipient and its pre-keys.
    pub fn personae_root(&self) -> Result<[u8; 32], GroupSessionError> {
        self.verify_identity()
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, GroupSessionError> {
        encode_cbor(self).map_err(|error| GroupSessionError::Encode(error.to_string()))
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, GroupSessionError> {
        let bundle: Self = decode_cbor_strict(bytes)
            .map_err(|error| GroupSessionError::Decode(error.to_string()))?;
        bundle.decode_bundles()?;
        Ok(bundle)
    }

    fn seal(
        group: GroupSessionId,
        recipient: GroupRecipientId,
        for_peer: GroupRecipientId,
        bundles: &[OneTimeKeyBundle],
        identity: &dyn IdentityProvider,
    ) -> Result<Self, GroupSessionError> {
        let payload =
            encode_cbor(&bundles).map_err(|error| GroupSessionError::Encode(error.to_string()))?;
        let salt = identity_salt(group);
        let attestation = identity
            .attest_derived_key(&salt)
            .map_err(|error| GroupSessionError::Identity(error.to_string()))?;
        let signer = identity
            .derive_keypair(&salt)
            .map_err(|error| GroupSessionError::Identity(error.to_string()))?;
        let mut bundle = Self {
            version: MESSAGE_PREKEY_VERSION,
            group,
            recipient,
            for_peer,
            payload,
            attestation,
            identity_signature: Vec::new(),
        };
        bundle.identity_signature = signer
            .sign(&bundle.identity_signing_bytes())
            .to_bytes()
            .to_vec();
        Ok(bundle)
    }

    pub(crate) fn decode_bundles(
        &self,
    ) -> Result<(Vec<OneTimeKeyBundle>, [u8; 32]), GroupSessionError> {
        if self.version != MESSAGE_PREKEY_VERSION {
            return Err(GroupSessionError::UnsupportedPrekeyVersion(self.version));
        }
        let bundles: Vec<OneTimeKeyBundle> = decode_cbor_strict(self.payload.as_slice())
            .map_err(|error| GroupSessionError::Decode(error.to_string()))?;
        for bundle in &bundles {
            let derived = GroupRecipientId(bundle.identity_key().to_bytes());
            if self.recipient != derived {
                return Err(GroupSessionError::PrekeyRecipientMismatch {
                    claimed: self.recipient,
                    derived,
                });
            }
        }
        let root = self.verify_identity()?;
        Ok((bundles, root))
    }

    fn verify_identity(&self) -> Result<[u8; 32], GroupSessionError> {
        let attestation = self
            .attestation
            .check(&identity_salt(self.group))
            .map_err(GroupSessionError::InvalidPersonaeAttestation)?;
        let root = *attestation.master();
        let derived = Ed25519PublicKey::from_bytes(attestation.derived())
            .map_err(|error| GroupSessionError::Identity(error.to_string()))?;
        let signature_bytes: [u8; 64] = self
            .identity_signature
            .as_slice()
            .try_into()
            .map_err(|_| GroupSessionError::InvalidPersonaeSignature)?;
        if !derived.verify(
            &self.identity_signing_bytes(),
            &Ed25519Signature::from_bytes(&signature_bytes),
        ) {
            return Err(GroupSessionError::InvalidPersonaeSignature);
        }
        Ok(root)
    }

    fn identity_signing_bytes(&self) -> Vec<u8> {
        let mut message = Vec::with_capacity(MESSAGE_PREKEY_IDENTITY_DOMAIN.len() + 106);
        message.extend_from_slice(MESSAGE_PREKEY_IDENTITY_DOMAIN);
        message.extend_from_slice(&self.version.to_le_bytes());
        message.extend_from_slice(&self.group.0);
        message.extend_from_slice(&self.recipient.0);
        message.extend_from_slice(&self.for_peer.0);
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

fn identity_salt(group: GroupSessionId) -> Vec<u8> {
    [MESSAGE_PREKEY_IDENTITY_DOMAIN, &group.0].concat()
}

/// Durable state for one forward-secure group member.
///
/// Persist [`MessageSession::to_bytes`] through Personae's sealed-record
/// store. The encoded state holds long-term private keys and live ratchets and
/// must never reach plaintext storage. Persist after every operation: a
/// restored older state would reuse message keys it had already deleted.
#[derive(Debug, Serialize, Deserialize)]
pub struct MessageSession {
    version: u16,
    group: GroupSessionId,
    member: GroupRecipientId,
    personae_root: [u8; 32],
    dcgka: Option<MessageDcgka>,
    ratchets: crate::group_crypto::MessageRatchets,
    next_local_sequence: u64,
    next_remote_sequence: BTreeMap<GroupRecipientId, u64>,
    prekey_roots: BTreeMap<GroupRecipientId, [u8; 32]>,
    registered_batches: BTreeSet<[u8; 32]>,
}

impl From<MessageControl> for GroupControlAction {
    fn from(value: MessageControl) -> Self {
        match value {
            ControlMessage::Create { initial_members } => Self::Create { initial_members },
            ControlMessage::Update => Self::Update,
            ControlMessage::Add { added } => Self::Add { added },
            ControlMessage::Remove { removed } => Self::Remove { removed },
            ControlMessage::Ack {
                ack_sender,
                ack_seq,
            } => Self::Ack {
                ack_sender,
                ack_seq,
            },
            ControlMessage::AddAck {
                ack_sender,
                ack_seq,
            } => Self::AddAck {
                ack_sender,
                ack_seq,
            },
        }
    }
}

impl From<GroupControlAction> for MessageControl {
    fn from(value: GroupControlAction) -> Self {
        match value {
            GroupControlAction::Create { initial_members } => Self::Create { initial_members },
            GroupControlAction::Update => Self::Update,
            GroupControlAction::Add { added } => Self::Add { added },
            GroupControlAction::Remove { removed } => Self::Remove { removed },
            GroupControlAction::Ack {
                ack_sender,
                ack_seq,
            } => Self::Ack {
                ack_sender,
                ack_seq,
            },
            GroupControlAction::AddAck {
                ack_sender,
                ack_seq,
            } => Self::AddAck {
                ack_sender,
                ack_seq,
            },
        }
    }
}

impl MessageSession {
    /// Prepare a local recipient. Make a batch of one-time pre-keys for each
    /// peer with [`Self::prekeys_for`] and hand it over.
    pub fn new(
        group: GroupSessionId,
        identity: &dyn IdentityProvider,
    ) -> Result<Self, GroupSessionError> {
        Self::with_limits(group, identity, MessageLimits::default())
    }

    /// As [`Self::new`], with explicit skipped-message bounds.
    pub fn with_limits(
        group: GroupSessionId,
        identity: &dyn IdentityProvider,
        limits: MessageLimits,
    ) -> Result<Self, GroupSessionError> {
        let rng = Rng::default();
        let engine = |error: &dyn std::fmt::Display| GroupSessionError::Engine(error.to_string());
        let secret = SecretKey::from_rng(&rng).map_err(|error| engine(&error))?;
        let keys = KeyManager::init(&secret).map_err(|error| engine(&error))?;
        let keys = KeyManager::rotate_prekey(keys, Lifetime::default(), &rng)
            .map_err(|error| engine(&error))?;
        let member = GroupRecipientId(
            KeyManager::prekey_bundle(&keys)
                .map_err(|error| engine(&error))?
                .identity_key()
                .to_bytes(),
        );
        let dgm = MessageDgm::init(member);
        let mut dcgka: MessageDcgka = Dcgka::init(member, keys, KeyRegistry::init(), dgm);
        // The engine looks the member's own identity key up in its registry.
        let own = generate_onetime_bundles(&mut dcgka, 1)?;
        let personae_root =
            MessagePrekeyBundle::seal(group, member, member, &own, identity)?.personae_root()?;
        register_onetime_bundles(&mut dcgka, member, own)?;
        Ok(Self {
            version: MESSAGE_SESSION_VERSION,
            group,
            member,
            personae_root,
            dcgka: Some(dcgka),
            ratchets: crate::group_crypto::MessageRatchets::new(limits),
            next_local_sequence: 0,
            next_remote_sequence: BTreeMap::new(),
            prekey_roots: BTreeMap::from([(member, personae_root)]),
            registered_batches: BTreeSet::new(),
        })
    }

    /// Make `count` one-time pre-keys for `peer` alone to spend on first
    /// contact. Make another batch if the peer joins again or runs out.
    pub fn prekeys_for(
        &mut self,
        identity: &dyn IdentityProvider,
        peer: GroupRecipientId,
        count: usize,
    ) -> Result<MessagePrekeyBundle, GroupSessionError> {
        let mut state = self.take_state()?;
        let bundles = generate_onetime_bundles(&mut state, count);
        self.dcgka = Some(state);
        MessagePrekeyBundle::seal(self.group, self.member, peer, &bundles?, identity)
    }

    pub fn group(&self) -> GroupSessionId {
        self.group
    }

    pub fn member(&self) -> GroupRecipientId {
        self.member
    }

    pub fn personae_root(&self) -> [u8; 32] {
        self.personae_root
    }

    /// Whether the local member can currently send.
    pub fn can_send(&self) -> bool {
        self.ratchets.can_send()
    }

    /// Serialize secret session state for a Personae-sealed record.
    pub fn to_bytes(&self) -> Result<Vec<u8>, GroupSessionError> {
        encode_cbor(self).map_err(|error| GroupSessionError::Encode(error.to_string()))
    }

    /// Restore and validate secret session state loaded from a sealed record.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, GroupSessionError> {
        let session: Self =
            decode_cbor(bytes).map_err(|error| GroupSessionError::Decode(error.to_string()))?;
        session.validate()?;
        Ok(session)
    }
}
