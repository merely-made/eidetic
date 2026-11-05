// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use p2panda_encryption::message_scheme::dcgka::{OperationOutput, ProcessInput};

use super::*;
use crate::group_session::{GROUP_CONTROL_VERSION, GROUP_DIRECT_VERSION};
use crate::{
    GroupControlFrame, GroupDirectFrame, GroupSessionDispatch, GroupSessionProcess,
    MessageCiphertext,
};

type Output = OperationOutput<GroupRecipientId, GroupControlId, MessageDgm>;

impl MessageSession {
    pub fn members(&self) -> Result<BTreeSet<GroupRecipientId>, GroupSessionError> {
        let view = Dcgka::member_view(self.state()?, &self.member)
            .map_err(|error| GroupSessionError::Engine(error.to_string()))?;
        Ok(view.into_iter().collect())
    }

    /// Register a peer's one-time pre-keys, once the containing domain has
    /// established that its Personae root may join. Registering the same batch
    /// again changes nothing.
    pub fn register_prekeys(
        &mut self,
        bundle: &MessagePrekeyBundle,
    ) -> Result<(), GroupSessionError> {
        if bundle.group != self.group {
            return Err(GroupSessionError::WrongGroup {
                expected: self.group,
                actual: bundle.group,
            });
        }
        if bundle.for_peer != self.member {
            return Err(GroupSessionError::PrekeysForAnotherPeer(bundle.for_peer));
        }
        let (decoded, root) = bundle.decode_bundles()?;
        if let Some(known) = self.prekey_roots.get(&bundle.recipient)
            && *known != root
        {
            return Err(GroupSessionError::ConflictingPrekey(bundle.recipient));
        }
        let digest = bundle.digest()?;
        if self.registered_batches.contains(&digest) {
            return Ok(());
        }
        let mut state = self.take_state()?;
        let registered = register_onetime_bundles(&mut state, bundle.recipient, decoded);
        self.dcgka = Some(state);
        registered?;
        self.prekey_roots.insert(bundle.recipient, root);
        self.registered_batches.insert(digest);
        Ok(())
    }

    /// Create the group with `initial_members`, each of whom must have a
    /// registered pre-key batch.
    pub fn create(
        &mut self,
        initial_members: &[GroupRecipientId],
    ) -> Result<GroupSessionDispatch, GroupSessionError> {
        if !self.members()?.is_empty() {
            return Err(GroupSessionError::GroupAlreadyCreated);
        }
        let members: Vec<_> = initial_members.iter().copied().collect();
        self.require_registered(members.iter().copied())?;
        self.local(|state, rng| Dcgka::create(state, members, rng))
    }

    /// Rotate every member's chain secret, for post-compromise recovery.
    pub fn update(&mut self) -> Result<GroupSessionDispatch, GroupSessionError> {
        self.require_active()?;
        self.local(|state, rng| Dcgka::update(state, rng))
    }

    /// Welcome a registered recipient. The newcomer reads nothing sent before.
    pub fn add(
        &mut self,
        added: GroupRecipientId,
    ) -> Result<GroupSessionDispatch, GroupSessionError> {
        let members = self.require_active()?;
        self.require_registered(std::iter::once(added))?;
        if members.contains(&added) {
            return Err(GroupSessionError::MemberAlreadyPresent(added));
        }
        self.local(|state, rng| Dcgka::add(state, added, rng))
    }

    /// Remove a member. They read nothing sent after.
    pub fn remove(
        &mut self,
        removed: GroupRecipientId,
    ) -> Result<GroupSessionDispatch, GroupSessionError> {
        let members = self.require_active()?;
        if removed == self.member {
            return Err(GroupSessionError::CannotRemoveSelf);
        }
        if !members.contains(&removed) {
            return Err(GroupSessionError::UnknownMember(removed));
        }
        self.local(|state, rng| Dcgka::remove(state, removed, rng))
    }

    /// Apply one frame after the domain has authenticated it and delivered it
    /// after everything it causally follows. State changes commit only after
    /// the whole transition succeeds.
    pub fn process(
        &mut self,
        authenticated_author_root: [u8; 32],
        control: &GroupControlFrame,
        direct: Option<&GroupDirectFrame>,
    ) -> Result<GroupSessionProcess, GroupSessionError> {
        let mut next = self.try_clone()?;
        let output = next.process_inner(authenticated_author_root, control, direct)?;
        *self = next;
        Ok(output)
    }

    /// Encrypt for the group under the local sending ratchet. The key is
    /// deleted as it is used.
    pub fn seal_message(
        &mut self,
        plaintext: &[u8],
    ) -> Result<MessageCiphertext, GroupSessionError> {
        self.require_active()?;
        Ok(self.ratchets.seal(plaintext)?)
    }

    /// Decrypt a message from `author`, the domain's authenticated writer.
    /// Messages may arrive out of order within [`MessageLimits`]; a replay, a
    /// forgery or a message beyond the limits is refused and costs nothing.
    pub fn open_message(
        &mut self,
        author: GroupRecipientId,
        message: &MessageCiphertext,
    ) -> Result<Vec<u8>, GroupSessionError> {
        if !self.members()?.contains(&author) {
            return Err(GroupSessionError::AuthorNotMember(author));
        }
        Ok(self.ratchets.open(author, message)?)
    }

    fn local(
        &mut self,
        operation: impl FnOnce(MessageDcgka, &Rng) -> Result<(MessageDcgka, Output), DcgkaFault>,
    ) -> Result<GroupSessionDispatch, GroupSessionError> {
        let rng = Rng::default();
        let mut next = self.try_clone()?;
        let state = next.take_state()?;
        let (state, output) = operation(state, &rng).map_err(engine)?;
        let id = next.next_control_id();
        let (state, output) = Dcgka::process_local(state, id, output, &rng).map_err(engine)?;
        next.dcgka = Some(state);
        if let Some(secret) = output.me_update_secret {
            next.ratchets.reset_send(secret);
        }
        let dispatch = next.frames(id, output.control_message, output.direct_messages)?;
        next.next_remote_sequence
            .insert(next.member, next.next_local_sequence);
        *self = next;
        Ok(dispatch)
    }

    fn process_inner(
        &mut self,
        root: [u8; 32],
        control: &GroupControlFrame,
        direct: Option<&GroupDirectFrame>,
    ) -> Result<GroupSessionProcess, GroupSessionError> {
        control.validate()?;
        let author = control.id.author;
        if control.group != self.group {
            return Err(GroupSessionError::WrongGroup {
                expected: self.group,
                actual: control.group,
            });
        }
        let expected_root = self
            .prekey_roots
            .get(&author)
            .ok_or(GroupSessionError::MissingPrekey(author))?;
        if *expected_root != root {
            return Err(GroupSessionError::ControlAuthorRootMismatch { author });
        }
        let expected = self.next_remote_sequence.get(&author).copied();
        match expected {
            Some(expected) if control.id.sequence != expected => {
                return Err(GroupSessionError::UnexpectedSequence {
                    author,
                    expected,
                    actual: control.id.sequence,
                });
            }
            _ => {}
        }
        if author == self.member {
            return Err(GroupSessionError::OwnControlFrame);
        }
        let direct_message = match direct {
            Some(frame) => Some(self.decode_direct(control, frame)?),
            None => None,
        };
        let next_sequence = control
            .id
            .sequence
            .checked_add(1)
            .ok_or(GroupSessionError::SequenceExhausted(author))?;
        let rng = Rng::default();
        let state = self.take_state()?;
        let (state, output) = Dcgka::process_remote(
            state,
            ProcessInput {
                seq: control.id,
                sender: author,
                control_message: control.action.clone().into(),
                direct_message,
            },
            &rng,
        )
        .map_err(engine)?;
        self.dcgka = Some(state);
        if let Some(secret) = output.me_update_secret {
            self.ratchets.reset_send(secret);
        }
        if let Some(secret) = output.sender_update_secret {
            self.ratchets.reset_receive(author, secret);
        }
        self.next_remote_sequence.insert(author, next_sequence);
        let outbound = match output.control_message {
            Some(message) => {
                let id = self.next_control_id();
                let dispatch = self.frames(id, message, output.direct_messages)?;
                self.next_remote_sequence
                    .insert(self.member, self.next_local_sequence);
                Some(dispatch)
            }
            None => None,
        };
        Ok(GroupSessionProcess {
            installed_epochs: Vec::new(),
            outbound,
        })
    }

    fn next_control_id(&self) -> GroupControlId {
        GroupControlId {
            author: self.member,
            sequence: self.next_local_sequence,
        }
    }

    /// Wrap one engine output as frames and spend the sequence number.
    fn frames(
        &mut self,
        id: GroupControlId,
        control: MessageControl,
        direct_messages: Vec<MessageDirect>,
    ) -> Result<GroupSessionDispatch, GroupSessionError> {
        let mut direct = Vec::with_capacity(direct_messages.len());
        for message in direct_messages {
            let payload = encode_cbor(&message)
                .map_err(|error| GroupSessionError::Encode(error.to_string()))?;
            direct.push(GroupDirectFrame {
                version: GROUP_DIRECT_VERSION,
                group: self.group,
                control: id,
                recipient: message.recipient,
                payload,
            });
        }
        self.next_local_sequence = id
            .sequence
            .checked_add(1)
            .ok_or(GroupSessionError::SequenceExhausted(self.member))?;
        Ok(GroupSessionDispatch {
            control: GroupControlFrame {
                version: GROUP_CONTROL_VERSION,
                group: self.group,
                id,
                action: control.into(),
            },
            direct,
        })
    }

    fn decode_direct(
        &self,
        control: &GroupControlFrame,
        direct: &GroupDirectFrame,
    ) -> Result<MessageDirect, GroupSessionError> {
        direct.validate()?;
        if direct.group != self.group {
            return Err(GroupSessionError::WrongGroup {
                expected: self.group,
                actual: direct.group,
            });
        }
        if direct.control != control.id {
            return Err(GroupSessionError::DirectControlMismatch);
        }
        if direct.recipient != self.member {
            return Err(GroupSessionError::DirectRecipientMismatch {
                expected: self.member,
                actual: direct.recipient,
            });
        }
        let message: MessageDirect = decode_cbor_strict(direct.payload.as_slice())
            .map_err(|error| GroupSessionError::Decode(error.to_string()))?;
        if message.recipient != direct.recipient {
            return Err(GroupSessionError::DirectRecipientMismatch {
                expected: direct.recipient,
                actual: message.recipient,
            });
        }
        Ok(message)
    }

    pub(crate) fn try_clone(&self) -> Result<Self, GroupSessionError> {
        Self::from_bytes(&self.to_bytes()?)
    }

    pub(super) fn state(&self) -> Result<&MessageDcgka, GroupSessionError> {
        self.dcgka.as_ref().ok_or(GroupSessionError::MissingState)
    }

    pub(super) fn take_state(&mut self) -> Result<MessageDcgka, GroupSessionError> {
        self.dcgka.take().ok_or(GroupSessionError::MissingState)
    }

    fn require_registered(
        &self,
        recipients: impl IntoIterator<Item = GroupRecipientId>,
    ) -> Result<(), GroupSessionError> {
        for recipient in recipients {
            if recipient != self.member && !self.prekey_roots.contains_key(&recipient) {
                return Err(GroupSessionError::MissingPrekey(recipient));
            }
        }
        Ok(())
    }

    fn require_active(&self) -> Result<BTreeSet<GroupRecipientId>, GroupSessionError> {
        let members = self.members()?;
        if !members.contains(&self.member) {
            return Err(GroupSessionError::LocalMemberInactive);
        }
        Ok(members)
    }

    pub(super) fn validate(&self) -> Result<(), GroupSessionError> {
        if self.version != MESSAGE_SESSION_VERSION {
            return Err(GroupSessionError::UnsupportedSessionVersion(self.version));
        }
        self.state()?;
        if self.prekey_roots.get(&self.member) != Some(&self.personae_root) {
            return Err(GroupSessionError::InvalidPrekeyBindings);
        }
        let expected_local = self
            .next_remote_sequence
            .get(&self.member)
            .copied()
            .unwrap_or(0);
        if self.next_local_sequence != expected_local {
            return Err(GroupSessionError::InvalidLocalSequence {
                expected: expected_local,
                actual: self.next_local_sequence,
            });
        }
        Ok(())
    }
}

type DcgkaFault = p2panda_encryption::message_scheme::dcgka::DcgkaError<
    GroupRecipientId,
    GroupControlId,
    KeyRegistry<GroupRecipientId>,
    MessageDgm,
    KeyManager,
>;

fn engine(error: DcgkaFault) -> GroupSessionError {
    GroupSessionError::Engine(error.to_string())
}
