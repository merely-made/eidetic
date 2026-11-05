// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Message-scheme engine state: per-member ratchets and the key-agreement
//! edits the engine does not expose.

use std::collections::BTreeMap;

use p2panda_encryption::Rng;
use serde::{Deserialize, Serialize};

use super::GroupCryptoError;
use p2panda_encryption::key_bundle::OneTimeKeyBundle;
use p2panda_encryption::key_manager::{KeyManager, KeyManagerState};
use p2panda_encryption::key_registry::{KeyRegistry, KeyRegistryState};
use p2panda_encryption::message_scheme::dcgka::DcgkaState;
use p2panda_encryption::message_scheme::dcgka::UpdateSecret;
use p2panda_encryption::message_scheme::group::GroupConfig;
use p2panda_encryption::message_scheme::test_utils::dgm::AckedTestDgm;
use p2panda_encryption::message_scheme::{
    DecryptionRatchet, DecryptionRatchetState, Generation, RatchetSecret, RatchetSecretState,
    decrypt_message, encrypt_message,
};
use p2panda_encryption::traits::PreKeyManager;

use crate::{GroupControlId, GroupRecipientId};

pub(crate) type MessageDgm = AckedTestDgm<GroupRecipientId, GroupControlId>;
pub(crate) type MessageDcgka = DcgkaState<
    GroupRecipientId,
    GroupControlId,
    KeyRegistry<GroupRecipientId>,
    MessageDgm,
    KeyManager,
>;

/// Key agreement state of a message-scheme DCGKA.
///
/// The engine keeps the peer registry and the member's own key manager in
/// crate-private fields, so a session that learns a peer's pre-keys or makes
/// its own after `init` edits them through the state's serde form.
fn edit_dcgka_field<T, R>(
    dcgka: &mut MessageDcgka,
    field: &str,
    edit: impl FnOnce(&mut T) -> Result<R, GroupCryptoError>,
) -> Result<R, GroupCryptoError>
where
    T: Serialize + for<'de> Deserialize<'de>,
{
    let codec = |error: cbor_core::SerdeError| GroupCryptoError::Engine(error.to_string());
    let mut value = cbor_core::Value::serialized(&*dcgka).map_err(codec)?;
    let slot = value
        .get_mut(field)
        .ok_or_else(|| GroupCryptoError::Engine(format!("dcgka state has no {field} field")))?;
    let mut inner: T = slot.deserialized().map_err(codec)?;
    let result = edit(&mut inner)?;
    *slot = cbor_core::Value::serialized(&inner).map_err(codec)?;
    *dcgka = value.deserialized().map_err(codec)?;
    Ok(result)
}

/// Make the member's own one-time pre-keys, keeping their secrets in the
/// engine's key manager.
pub(crate) fn generate_onetime_bundles(
    dcgka: &mut MessageDcgka,
    count: usize,
) -> Result<Vec<OneTimeKeyBundle>, GroupCryptoError> {
    let rng = Rng::default();
    edit_dcgka_field(dcgka, "my_keys", |keys: &mut KeyManagerState| {
        let mut bundles = Vec::with_capacity(count);
        let mut state = keys.clone();
        for _ in 0..count {
            let (next, bundle) = KeyManager::generate_onetime_bundle(state, &rng)
                .map_err(|error| GroupCryptoError::Engine(error.to_string()))?;
            state = next;
            bundles.push(bundle);
        }
        *keys = state;
        Ok(bundles)
    })
}

/// Record a peer's one-time pre-keys for later first contact.
pub(crate) fn register_onetime_bundles(
    dcgka: &mut MessageDcgka,
    peer: GroupRecipientId,
    bundles: Vec<OneTimeKeyBundle>,
) -> Result<(), GroupCryptoError> {
    edit_dcgka_field(
        dcgka,
        "pki",
        |registry: &mut KeyRegistryState<GroupRecipientId>| {
            let mut state = registry.clone();
            for bundle in bundles {
                state = KeyRegistry::add_onetime_bundle(state, peer, bundle)
                    .map_err(|error| GroupCryptoError::Engine(error.to_string()))?;
            }
            *registry = state;
            Ok(())
        },
    )
}

/// Message-scheme ciphertext carried as a domain operation body. The author
/// is the domain's authenticated writer, not a field of the ciphertext.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageCiphertext {
    pub generation: Generation,
    pub ciphertext: Vec<u8>,
}

/// Bounds on skipped and out-of-order messages within one sender chain.
///
/// A message may be skipped (lost) up to `maximum_forward_distance`
/// generations ahead of the chain head, and one that arrives late is readable
/// only while it is within `out_of_order_tolerance` generations behind it.
/// Each skipped key stays in memory until used or aged out, so these numbers
/// are also the window in which a stolen state reads skipped messages.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageLimits {
    pub maximum_forward_distance: u32,
    pub out_of_order_tolerance: u32,
}

impl Default for MessageLimits {
    fn default() -> Self {
        let config = GroupConfig::default();
        Self {
            maximum_forward_distance: config.maximum_forward_distance,
            out_of_order_tolerance: config.out_of_order_tolerance,
        }
    }
}

/// Per-member message ratchets: one sending chain and one receiving chain per
/// other author. A chain is replaced when key agreement hands over a fresh
/// update secret; each message key is deleted as it is used.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct MessageRatchets {
    limits: MessageLimits,
    send: Option<RatchetSecretState>,
    receive: BTreeMap<GroupRecipientId, DecryptionRatchetState>,
}

impl MessageRatchets {
    pub(crate) fn new(limits: MessageLimits) -> Self {
        Self {
            limits,
            send: None,
            receive: BTreeMap::new(),
        }
    }

    pub(crate) fn reset_send(&mut self, secret: UpdateSecret) {
        self.send = Some(RatchetSecret::init(secret.into()));
    }

    pub(crate) fn reset_receive(&mut self, sender: GroupRecipientId, secret: UpdateSecret) {
        self.receive
            .insert(sender, DecryptionRatchet::init(secret.into()));
    }

    pub(crate) fn can_send(&self) -> bool {
        self.send.is_some()
    }

    pub(crate) fn seal(&mut self, plaintext: &[u8]) -> Result<MessageCiphertext, GroupCryptoError> {
        let ratchet = self
            .send
            .take()
            .ok_or(GroupCryptoError::MissingSendRatchet)?;
        let (next, generation, material) = RatchetSecret::ratchet_forward(ratchet)
            .map_err(|error| GroupCryptoError::Engine(error.to_string()))?;
        self.send = Some(next);
        let ciphertext = encrypt_message(plaintext, material)
            .map_err(|error| GroupCryptoError::Engine(error.to_string()))?;
        Ok(MessageCiphertext {
            generation,
            ciphertext,
        })
    }

    /// Decrypt one message. The chain advances only when the message
    /// authenticates, so a forged or replayed ciphertext cannot burn keys.
    pub(crate) fn open(
        &mut self,
        sender: GroupRecipientId,
        message: &MessageCiphertext,
    ) -> Result<Vec<u8>, GroupCryptoError> {
        let ratchet = self
            .receive
            .get(&sender)
            .ok_or(GroupCryptoError::MissingReceiveRatchet(sender))?
            .clone();
        let (next, material) = DecryptionRatchet::secret_for_decryption(
            ratchet,
            message.generation,
            self.limits.maximum_forward_distance,
            self.limits.out_of_order_tolerance,
        )
        .map_err(|error| GroupCryptoError::Engine(error.to_string()))?;
        let plaintext = decrypt_message(&message.ciphertext, material)
            .map_err(|error| GroupCryptoError::Engine(error.to_string()))?;
        self.receive.insert(sender, next);
        Ok(plaintext)
    }
}
