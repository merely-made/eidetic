// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;
use crate::{GroupEncryptionMode, GroupEncryptionProfile};

impl GroupSession {
    /// Prepare a local recipient and its publishable pre-key bundle.
    pub fn new(
        group: GroupSessionId,
        identity: &dyn IdentityProvider,
    ) -> Result<(Self, GroupPrekeyBundle), GroupSessionError> {
        let rng = Rng::default();
        let identity_secret = SecretKey::from_rng(&rng)
            .map_err(|error| GroupSessionError::Engine(error.to_string()))?;
        let keys = KeyManager::init(&identity_secret)
            .map_err(|error| GroupSessionError::Engine(error.to_string()))?;
        let keys = KeyManager::rotate_prekey(keys, Lifetime::default(), &rng)
            .map_err(|error| GroupSessionError::Engine(error.to_string()))?;
        let prekey = KeyManager::prekey_bundle(&keys)
            .map_err(|error| GroupSessionError::Engine(error.to_string()))?;
        let member = GroupRecipientId(prekey.identity_key().to_bytes());
        let payload =
            encode_cbor(&prekey).map_err(|error| GroupSessionError::Encode(error.to_string()))?;
        let salt = group_prekey_identity_salt(group);
        let attestation = identity
            .attest_derived_key(&salt)
            .map_err(|error| GroupSessionError::Identity(error.to_string()))?;
        let signer = identity
            .derive_keypair(&salt)
            .map_err(|error| GroupSessionError::Identity(error.to_string()))?;
        let mut public_bundle = GroupPrekeyBundle {
            version: GROUP_PREKEY_VERSION,
            group,
            recipient: member,
            payload,
            attestation,
            identity_signature: Vec::new(),
        };
        public_bundle.identity_signature = signer
            .sign(&public_bundle.identity_signing_bytes())
            .to_bytes()
            .to_vec();
        let personae_root = public_bundle.personae_root()?;
        let pki = KeyRegistry::add_longterm_bundle(KeyRegistry::init(), member, prekey)
            .map_err(|error| GroupSessionError::Engine(error.to_string()))?;
        let dcgka = Dcgka::init(member, keys, pki, OrderedMembership::init(member));
        let session = Self {
            version: GROUP_SESSION_VERSION,
            group,
            member,
            personae_root,
            dcgka: Some(dcgka),
            keyring: DataKeyring::new(),
            next_local_sequence: 0,
            next_remote_sequence: BTreeMap::new(),
            prekey_digests: BTreeMap::from([(member, public_bundle.digest()?)]),
            prekey_roots: BTreeMap::from([(member, personae_root)]),
        };
        Ok((session, public_bundle))
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

    pub fn current_epoch(&self) -> Option<GroupSecretId> {
        self.keyring.current_epoch()
    }

    pub fn epoch_count(&self) -> usize {
        self.keyring.epoch_count()
    }

    /// Serialize the retained data epochs for a domain replica opened beside
    /// this group session.
    ///
    /// The returned bytes remain secret state and must stay within the same
    /// Personae-sealed storage boundary as [`Self::to_bytes`].
    pub fn data_keyring_state(&self) -> Result<Vec<u8>, GroupSessionError> {
        self.keyring.to_bytes().map_err(GroupSessionError::Crypto)
    }

    /// The recipient `root` is known by, from the pre-keys this session
    /// registered plus its own. An unregistered root is `None`.
    ///
    /// A root that registered several recipients -- it reinstalled, or joined
    /// twice -- resolves to the one that is a current member, and to `None`
    /// when none or several of them are. A root with one recipient resolves to
    /// it either way, so resolving is still not membership: a removed sole
    /// member resolves, and callers check [`Self::members`].
    pub fn recipient_for_root(&self, root: [u8; 32]) -> Option<GroupRecipientId> {
        let bound: Vec<_> = self
            .prekey_roots
            .iter()
            .filter(|(_, bound_root)| **bound_root == root)
            .map(|(recipient, _)| *recipient)
            .collect();
        if let [only] = bound[..] {
            return Some(only);
        }
        let members = self.members().ok()?;
        let mut current = bound
            .into_iter()
            .filter(|recipient| members.contains(recipient));
        let recipient = current.next()?;
        current.next().is_none().then_some(recipient)
    }

    pub fn members(&self) -> Result<BTreeSet<GroupRecipientId>, GroupSessionError> {
        let state = self.state()?;
        let members =
            Dcgka::members(state).map_err(|error| GroupSessionError::Engine(error.to_string()))?;
        Ok(members.into_iter().collect())
    }

    /// Register a cryptographically bound pre-key only after the containing
    /// domain has established that its Personae root is eligible to join.
    pub fn register_prekey(&mut self, bundle: &GroupPrekeyBundle) -> Result<(), GroupSessionError> {
        if bundle.group != self.group {
            return Err(GroupSessionError::WrongGroup {
                expected: self.group,
                actual: bundle.group,
            });
        }
        let (decoded, personae_root) = bundle.decode_bundle()?;
        let digest = bundle.digest()?;
        if let Some(existing) = self.prekey_digests.get(&bundle.recipient) {
            return if existing == &digest {
                Ok(())
            } else {
                Err(GroupSessionError::ConflictingPrekey(bundle.recipient))
            };
        }

        let mut next = self.try_clone()?;
        let state = next.take_state()?;
        let pki = KeyRegistry::add_longterm_bundle(state.pki, bundle.recipient, decoded)
            .map_err(|error| GroupSessionError::Engine(error.to_string()))?;
        next.dcgka = Some(SessionDcgka { pki, ..state });
        next.prekey_digests.insert(bundle.recipient, digest);
        next.prekey_roots.insert(bundle.recipient, personae_root);
        next.validate()?;
        *self = next;
        Ok(())
    }

    /// Create the initial group and return its broadcast and addressed frames.
    pub fn create(
        &mut self,
        initial_members: &[GroupRecipientId],
    ) -> Result<GroupSessionDispatch, GroupSessionError> {
        if !self.members()?.is_empty() {
            return Err(GroupSessionError::GroupAlreadyCreated);
        }
        let members: BTreeSet<_> = initial_members
            .iter()
            .copied()
            .chain(std::iter::once(self.member))
            .collect();
        self.require_registered(members.iter().copied())?;

        let mut next = self.try_clone()?;
        let secret = next.keyring.rotate_random()?;
        let state = next.take_state()?;
        let (state, output) = Dcgka::create(
            state,
            members.into_iter().collect(),
            &secret,
            &Rng::default(),
        )
        .map_err(|error| GroupSessionError::Engine(error.to_string()))?;
        next.dcgka = Some(state);
        let dispatch = next.finish_local(output)?;
        *self = next;
        Ok(dispatch)
    }

    /// Rotate the group epoch for all current members.
    pub fn update(&mut self) -> Result<GroupSessionDispatch, GroupSessionError> {
        self.require_active_author()?;
        let mut next = self.try_clone()?;
        let secret = next.keyring.rotate_random()?;
        let state = next.take_state()?;
        let (state, output) = Dcgka::update(state, &secret, &Rng::default())
            .map_err(|error| GroupSessionError::Engine(error.to_string()))?;
        next.dcgka = Some(state);
        let dispatch = next.finish_local(output)?;
        *self = next;
        Ok(dispatch)
    }

    /// Welcome a registered recipient with the retained data-epoch bundle.
    pub fn add(
        &mut self,
        added: GroupRecipientId,
    ) -> Result<GroupSessionDispatch, GroupSessionError> {
        // One membership snapshot answers both the active-author check and the
        // already-present check; `members()` walks the DCGKA state and rebuilds
        // two sets on every call, so the checks share one view here rather than
        // each asking for their own.
        let members = self.members()?;
        if !members.contains(&self.member) {
            return Err(GroupSessionError::LocalMemberInactive);
        }
        self.require_registered(std::iter::once(added))?;
        if members.contains(&added) {
            return Err(GroupSessionError::MemberAlreadyPresent(added));
        }
        let mut next = self.try_clone()?;
        let state = next.take_state()?;
        let (state, output) =
            Dcgka::add(state, added, next.keyring.secret_bundle(), &Rng::default())
                .map_err(|error| GroupSessionError::Engine(error.to_string()))?;
        next.dcgka = Some(state);
        let dispatch = next.finish_local(output)?;
        *self = next;
        Ok(dispatch)
    }

    /// Remove a member and rotate the epoch away from that recipient.
    pub fn remove(
        &mut self,
        removed: GroupRecipientId,
    ) -> Result<GroupSessionDispatch, GroupSessionError> {
        // As in `add`: one membership snapshot serves both checks.
        let members = self.members()?;
        if !members.contains(&self.member) {
            return Err(GroupSessionError::LocalMemberInactive);
        }
        if removed == self.member {
            return Err(GroupSessionError::CannotRemoveSelf);
        }
        if !members.contains(&removed) {
            return Err(GroupSessionError::UnknownMember(removed));
        }
        let mut next = self.try_clone()?;
        let secret = next.keyring.rotate_random()?;
        let state = next.take_state()?;
        let (state, output) = Dcgka::remove(state, removed, &secret, &Rng::default())
            .map_err(|error| GroupSessionError::Engine(error.to_string()))?;
        next.dcgka = Some(state);
        let dispatch = next.finish_local(output)?;
        *self = next;
        Ok(dispatch)
    }

    /// Apply one frame after the domain has authenticated and causally ordered
    /// it. `authenticated_author_root` must come from that verified domain
    /// operation and must match the pre-key's Personae binding. State changes
    /// commit only after the full crypto transition succeeds.
    pub fn process(
        &mut self,
        authenticated_author_root: [u8; 32],
        control: &GroupControlFrame,
        direct: Option<&GroupDirectFrame>,
    ) -> Result<GroupSessionProcess, GroupSessionError> {
        let mut next = self.try_clone()?;
        let output = next.process_inner(authenticated_author_root, control, direct)?;
        next.validate()?;
        *self = next;
        Ok(output)
    }

    /// Forget retained data epochs the host has released, all or nothing. The
    /// exported keyring and later welcomes stop carrying them.
    pub fn forget_epochs(&mut self, epochs: &[GroupSecretId]) -> Result<(), GroupSessionError> {
        let mut next = self.try_clone()?;
        for epoch in epochs {
            if next.keyring.current_epoch() == Some(*epoch) {
                return Err(GroupSessionError::CurrentEpochNotForgettable(*epoch));
            }
            if !next.keyring.forget_authorized(epoch) {
                return Err(GroupSessionError::EpochNotHeld(*epoch));
            }
        }
        next.validate()?;
        *self = next;
        Ok(())
    }

    /// Apply a durable profile's retention window: forget epochs older than
    /// its newest `retained_data_epochs`, the current epoch always kept.
    pub fn enforce_retention(
        &mut self,
        profile: GroupEncryptionProfile,
    ) -> Result<Vec<GroupSecretId>, GroupSessionError> {
        if profile.mode != GroupEncryptionMode::Data {
            return Ok(Vec::new());
        }
        let mut next = self.try_clone()?;
        let forgotten = next
            .keyring
            .forget_outside_window(profile.retained_data_epochs);
        next.validate()?;
        *self = next;
        Ok(forgotten)
    }

    pub fn seal_random(&self, plaintext: &[u8]) -> Result<GroupCiphertext, GroupSessionError> {
        self.keyring
            .seal_random(plaintext)
            .map_err(GroupSessionError::Crypto)
    }

    pub fn open(&self, ciphertext: &GroupCiphertext) -> Result<Vec<u8>, GroupSessionError> {
        self.keyring
            .open(ciphertext)
            .map_err(GroupSessionError::Crypto)
    }

    /// Serialize secret session state for storage in a Personae-sealed record.
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

    pub(super) fn state(&self) -> Result<&SessionDcgka, GroupSessionError> {
        self.dcgka.as_ref().ok_or(GroupSessionError::MissingState)
    }

    pub(super) fn take_state(&mut self) -> Result<SessionDcgka, GroupSessionError> {
        self.dcgka.take().ok_or(GroupSessionError::MissingState)
    }

    pub(super) fn require_registered(
        &self,
        recipients: impl IntoIterator<Item = GroupRecipientId>,
    ) -> Result<(), GroupSessionError> {
        for recipient in recipients {
            if !self.prekey_digests.contains_key(&recipient) {
                return Err(GroupSessionError::MissingPrekey(recipient));
            }
        }
        Ok(())
    }

    pub(super) fn require_active_author(&self) -> Result<(), GroupSessionError> {
        if !self.members()?.contains(&self.member) {
            return Err(GroupSessionError::LocalMemberInactive);
        }
        Ok(())
    }

    pub(super) fn finish_local(
        &mut self,
        output: OperationOutput<GroupRecipientId, GroupControlId, OrderedMembership>,
    ) -> Result<GroupSessionDispatch, GroupSessionError> {
        let id = GroupControlId {
            author: self.member,
            sequence: self.next_local_sequence,
        };
        let control = GroupControlFrame {
            version: GROUP_CONTROL_VERSION,
            group: self.group,
            id,
            action: output.control_message.into(),
        };
        let mut direct = Vec::with_capacity(output.direct_messages.len());
        for message in output.direct_messages {
            let recipient = message.recipient;
            let payload = encode_cbor(&message)
                .map_err(|error| GroupSessionError::Encode(error.to_string()))?;
            direct.push(GroupDirectFrame {
                version: GROUP_DIRECT_VERSION,
                group: self.group,
                control: id,
                recipient,
                payload,
            });
        }
        let dispatch = GroupSessionDispatch { control, direct };
        self.process_inner(self.personae_root, &dispatch.control, None)?;
        Ok(dispatch)
    }

    pub(super) fn process_inner(
        &mut self,
        authenticated_author_root: [u8; 32],
        control: &GroupControlFrame,
        direct: Option<&GroupDirectFrame>,
    ) -> Result<GroupSessionProcess, GroupSessionError> {
        control.validate()?;
        if control.group != self.group {
            return Err(GroupSessionError::WrongGroup {
                expected: self.group,
                actual: control.group,
            });
        }
        if control.id.author != self.member && !self.prekey_digests.contains_key(&control.id.author)
        {
            return Err(GroupSessionError::MissingPrekey(control.id.author));
        }
        let expected_root = self
            .prekey_roots
            .get(&control.id.author)
            .ok_or(GroupSessionError::MissingPrekey(control.id.author))?;
        if expected_root != &authenticated_author_root {
            return Err(GroupSessionError::ControlAuthorRootMismatch {
                author: control.id.author,
            });
        }

        // Only Update and Remove consult membership: Create and Add decide from
        // the frame alone, so the membership snapshot is built inside the arms
        // that read it instead of once for every frame processed.
        let needs_direct = match &control.action {
            GroupControlAction::Create { initial_members } => {
                control.id.author != self.member && initial_members.contains(&self.member)
            }
            GroupControlAction::Update => {
                control.id.author != self.member && self.members()?.contains(&self.member)
            }
            GroupControlAction::Add { added } => {
                control.id.author != self.member && *added == self.member
            }
            GroupControlAction::Remove { removed } => {
                control.id.author != self.member
                    && self.members()?.contains(&self.member)
                    && *removed != self.member
            }
        };
        match (needs_direct, direct) {
            (true, None) => return Err(GroupSessionError::MissingAddressedDirect(self.member)),
            (false, Some(_)) => return Err(GroupSessionError::UnexpectedDirect(self.member)),
            _ => {}
        }

        let expected = self
            .next_remote_sequence
            .get(&control.id.author)
            .copied()
            .unwrap_or(0);
        let joining_at_current_sequence =
            !self.next_remote_sequence.contains_key(&control.id.author)
                && matches!(
                    control.action,
                    GroupControlAction::Add { added } if added == self.member
                );
        if control.id.sequence != expected && !joining_at_current_sequence {
            return Err(GroupSessionError::UnexpectedSequence {
                author: control.id.author,
                expected,
                actual: control.id.sequence,
            });
        }

        let direct_message = match direct {
            Some(frame) => Some(self.decode_direct(control, frame)?),
            None => None,
        };
        let state = self.take_state()?;
        let input = ProcessInput {
            seq: control.id,
            sender: control.id.author,
            control_message: control.action.clone().into(),
            direct_message,
        };
        let (state, output) = Dcgka::process(state, input)
            .map_err(|error| GroupSessionError::Engine(error.to_string()))?;
        self.dcgka = Some(state);
        let installed_epochs = self.install_output(output);
        let next_sequence = control
            .id
            .sequence
            .checked_add(1)
            .ok_or(GroupSessionError::SequenceExhausted(control.id.author))?;
        self.next_remote_sequence
            .insert(control.id.author, next_sequence);
        if control.id.author == self.member {
            self.next_local_sequence = next_sequence;
        }
        Ok(GroupSessionProcess { installed_epochs })
    }

    pub(super) fn decode_direct(
        &self,
        control: &GroupControlFrame,
        direct: &GroupDirectFrame,
    ) -> Result<SessionDirect, GroupSessionError> {
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
        let message: SessionDirect = decode_cbor_strict(direct.payload.as_slice())
            .map_err(|error| GroupSessionError::Decode(error.to_string()))?;
        if message.recipient != direct.recipient {
            return Err(GroupSessionError::DirectRecipientMismatch {
                expected: direct.recipient,
                actual: message.recipient,
            });
        }
        Ok(message)
    }

    pub(super) fn install_output(&mut self, output: GroupSecretOutput) -> Vec<GroupSecretId> {
        match output {
            GroupSecretOutput::None => Vec::new(),
            GroupSecretOutput::Secret(secret) => {
                let epoch = secret.id();
                self.keyring.install(secret);
                vec![epoch]
            }
            GroupSecretOutput::Bundle(bundle) => self.keyring.install_bundle(bundle),
        }
    }

    pub(crate) fn try_clone(&self) -> Result<Self, GroupSessionError> {
        Self::from_bytes(&self.to_bytes()?)
    }

    pub(super) fn validate(&self) -> Result<(), GroupSessionError> {
        if self.version != GROUP_SESSION_VERSION {
            return Err(GroupSessionError::UnsupportedSessionVersion(self.version));
        }
        self.keyring.validate()?;
        let state = self.state()?;
        if state.my_id != self.member || state.dgm.my_id != self.member {
            return Err(GroupSessionError::StateRecipientMismatch);
        }
        if !self.prekey_digests.contains_key(&self.member) {
            return Err(GroupSessionError::MissingPrekey(self.member));
        }
        if self.prekey_roots.get(&self.member) != Some(&self.personae_root)
            || self.prekey_roots.len() != self.prekey_digests.len()
            || self
                .prekey_digests
                .keys()
                .any(|recipient| !self.prekey_roots.contains_key(recipient))
        {
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
