// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use identity::{InMemoryProvider, SealedRecordStorage};

use super::*;

fn register_all(sessions: &mut [&mut GroupSession], bundles: &[GroupPrekeyBundle]) {
    for session in sessions {
        for bundle in bundles {
            session.register_prekey(bundle).unwrap();
        }
    }
}

// Strictness control for the wire decodes (device pairing plan, ruling 24):
// the same frame with `version` in a non-shortest integer form is valid
// CBOR that a lenient decode accepts, and a peer frame must be refused.
#[test]
fn control_frame_refuses_non_canonical_cbor() {
    let frame = GroupControlFrame {
        version: GROUP_CONTROL_VERSION,
        group: GroupSessionId([0x47; 32]),
        id: GroupControlId {
            author: GroupRecipientId([0xa1; 32]),
            sequence: 7,
        },
        action: GroupControlAction::Update,
    };
    let canonical = frame.to_bytes().unwrap();
    assert_eq!(GroupControlFrame::from_bytes(&canonical).unwrap(), frame);

    let key = b"\x67version";
    let at = canonical
        .windows(key.len())
        .position(|window| window == key)
        .unwrap()
        + key.len();
    assert_eq!(canonical[at], GROUP_CONTROL_VERSION as u8);
    let mut long_form = canonical[..at].to_vec();
    long_form.extend_from_slice(&[0x19, 0x00, GROUP_CONTROL_VERSION as u8]);
    long_form.extend_from_slice(&canonical[at + 1..]);

    let lenient: GroupControlFrame = decode_cbor(long_form.as_slice()).unwrap();
    assert_eq!(lenient, frame);
    assert!(matches!(
        GroupControlFrame::from_bytes(&long_form),
        Err(GroupSessionError::Decode(_))
    ));
}

#[test]
fn addressed_welcome_update_removal_and_sealed_restart() {
    let group = GroupSessionId([0x47; 32]);
    let alice_identity = InMemoryProvider::from_seed([0xa1; 32]);
    let bob_identity = InMemoryProvider::from_seed([0xb2; 32]);
    let charlie_identity = InMemoryProvider::from_seed([0xc3; 32]);
    let (mut alice, alice_prekey) = GroupSession::new(group, &alice_identity).unwrap();
    let (mut bob, bob_prekey) = GroupSession::new(group, &bob_identity).unwrap();
    let (mut charlie, charlie_prekey) = GroupSession::new(group, &charlie_identity).unwrap();
    let alice_prekey = GroupPrekeyBundle::from_bytes(&alice_prekey.to_bytes().unwrap()).unwrap();
    assert_eq!(alice_prekey.personae_root().unwrap(), alice.personae_root());
    assert_eq!(bob_prekey.personae_root().unwrap(), bob.personae_root());
    assert_eq!(
        charlie_prekey.personae_root().unwrap(),
        charlie.personae_root()
    );
    assert_ne!(alice.personae_root(), bob.personae_root());
    let mut forged_prekey = alice_prekey.clone();
    forged_prekey.identity_signature[0] ^= 0x01;
    assert!(matches!(
        GroupPrekeyBundle::from_bytes(&forged_prekey.to_bytes().unwrap()),
        Err(GroupSessionError::InvalidPersonaeSignature)
    ));
    register_all(
        &mut [&mut alice, &mut bob, &mut charlie],
        &[
            alice_prekey.clone(),
            bob_prekey.clone(),
            charlie_prekey.clone(),
        ],
    );

    let create = alice.create(&[bob.member(), charlie.member()]).unwrap();
    let encoded_control = create.control.to_bytes().unwrap();
    let create_control = GroupControlFrame::from_bytes(&encoded_control).unwrap();
    let bob_direct = create.direct_for(bob.member()).unwrap();
    let encoded_direct = bob_direct.to_bytes().unwrap();
    let bob_direct = GroupDirectFrame::from_bytes(&encoded_direct).unwrap();
    let charlie_direct = create.direct_for(charlie.member()).unwrap();

    let bob_before = bob.to_bytes().unwrap();
    let mut wrong_recipient = GroupSession::from_bytes(&bob_before).unwrap();
    assert!(matches!(
        wrong_recipient.process(alice.personae_root(), &create_control, Some(charlie_direct)),
        Err(GroupSessionError::DirectRecipientMismatch { .. })
    ));
    assert_eq!(wrong_recipient.current_epoch(), None);

    let mut wrong_root = GroupSession::from_bytes(&bob_before).unwrap();
    assert!(matches!(
        wrong_root.process(bob.personae_root(), &create_control, Some(&bob_direct)),
        Err(GroupSessionError::ControlAuthorRootMismatch { .. })
    ));
    assert_eq!(wrong_root.current_epoch(), None);

    let bob_result = bob
        .process(alice.personae_root(), &create_control, Some(&bob_direct))
        .unwrap();
    assert_eq!(bob_result.installed_epochs.len(), 1);
    let charlie_result = charlie
        .process(alice.personae_root(), &create_control, Some(charlie_direct))
        .unwrap();
    assert_eq!(charlie_result.installed_epochs.len(), 1);
    assert_eq!(bob.current_epoch(), alice.current_epoch());
    assert_eq!(charlie.current_epoch(), alice.current_epoch());

    let before_update = alice.seal_random(b"before update").unwrap();
    assert_eq!(bob.open(&before_update).unwrap(), b"before update");

    let update = alice.update().unwrap();
    bob.process(
        alice.personae_root(),
        &update.control,
        update.direct_for(bob.member()),
    )
    .unwrap();
    charlie
        .process(
            alice.personae_root(),
            &update.control,
            update.direct_for(charlie.member()),
        )
        .unwrap();
    let after_update = alice.seal_random(b"after update").unwrap();
    assert_eq!(bob.open(&after_update).unwrap(), b"after update");
    assert_eq!(charlie.open(&after_update).unwrap(), b"after update");

    let directory = tempfile::tempdir().unwrap();
    let storage = SealedRecordStorage::open_with_key(directory.path(), [0x53; 32]);
    storage
        .save_record("groups/main.session", &bob.to_bytes().unwrap())
        .unwrap();
    let sealed_bytes: Vec<u8> = storage
        .load_record("groups/main.session")
        .unwrap()
        .expect("sealed group session");
    let mut reopened_bob = GroupSession::from_bytes(&sealed_bytes).unwrap();
    assert_eq!(reopened_bob.member(), bob.member());
    assert_eq!(reopened_bob.members().unwrap(), bob.members().unwrap());
    assert_eq!(reopened_bob.current_epoch(), bob.current_epoch());
    let reopened_keys = DataKeyring::from_bytes(&reopened_bob.data_keyring_state().unwrap())
        .expect("group session exposes the same sealed data epochs");
    assert_eq!(
        reopened_keys.epochs_oldest_first(),
        bob.keyring.epochs_oldest_first()
    );
    assert_eq!(reopened_bob.open(&after_update).unwrap(), b"after update");

    let removal = alice.remove(bob.member()).unwrap();
    assert!(removal.direct_for(bob.member()).is_none());
    assert!(removal.direct_for(charlie.member()).is_some());
    let bob_removal = reopened_bob
        .process(
            alice.personae_root(),
            &removal.control,
            removal.direct_for(reopened_bob.member()),
        )
        .unwrap();
    assert!(bob_removal.installed_epochs.is_empty());
    charlie
        .process(
            alice.personae_root(),
            &removal.control,
            removal.direct_for(charlie.member()),
        )
        .unwrap();

    let after_removal = alice.seal_random(b"after removal").unwrap();
    assert!(matches!(
        reopened_bob.open(&after_removal),
        Err(GroupSessionError::Crypto(GroupCryptoError::UnknownEpoch(_)))
    ));
    assert_eq!(charlie.open(&after_removal).unwrap(), b"after removal");
    assert_eq!(
        alice.members().unwrap(),
        BTreeSet::from([alice.member(), charlie.member()])
    );
    assert_eq!(alice.current_epoch(), charlie.current_epoch());
    assert_ne!(alice.current_epoch(), reopened_bob.current_epoch());
    assert_eq!(reopened_bob.open(&before_update).unwrap(), b"before update");

    let dave_identity = InMemoryProvider::from_seed([0xd4; 32]);
    let (mut dave, dave_prekey) = GroupSession::new(group, &dave_identity).unwrap();
    alice.register_prekey(&dave_prekey).unwrap();
    charlie.register_prekey(&dave_prekey).unwrap();
    dave.register_prekey(&alice_prekey).unwrap();
    dave.register_prekey(&charlie_prekey).unwrap();

    let welcome = alice.add(dave.member()).unwrap();
    assert_eq!(welcome.direct.len(), 1);
    assert!(welcome.direct_for(dave.member()).is_some());
    charlie
        .process(alice.personae_root(), &welcome.control, None)
        .unwrap();
    let welcomed = dave
        .process(
            alice.personae_root(),
            &welcome.control,
            welcome.direct_for(dave.member()),
        )
        .unwrap();
    assert_eq!(welcomed.installed_epochs.len(), alice.epoch_count());
    assert_eq!(dave.current_epoch(), alice.current_epoch());
    assert_eq!(dave.members().unwrap(), alice.members().unwrap());
    assert_eq!(dave.open(&after_removal).unwrap(), b"after removal");
}

const LOOKUP_GROUP: GroupSessionId = GroupSessionId([0x49; 32]);

#[test]
fn a_founder_resolves_an_invited_root_to_its_recipient() {
    let alice_identity = InMemoryProvider::from_seed([0xa1; 32]);
    let bob_identity = InMemoryProvider::from_seed([0xb2; 32]);
    let (mut alice, alice_prekey) = GroupSession::new(LOOKUP_GROUP, &alice_identity).unwrap();
    let (mut bob, bob_prekey) = GroupSession::new(LOOKUP_GROUP, &bob_identity).unwrap();
    let bob_root = bob_prekey.personae_root().unwrap();
    assert_eq!(
        alice.recipient_for_root(bob_root),
        None,
        "not registered yet"
    );

    alice.register_prekey(&bob_prekey).unwrap();
    bob.register_prekey(&alice_prekey).unwrap();
    alice.create(&[]).unwrap();
    let welcome = alice.add(bob.member()).unwrap();
    bob.process(
        alice.personae_root(),
        &welcome.control,
        welcome.direct_for(bob.member()),
    )
    .unwrap();

    assert_eq!(alice.recipient_for_root(bob_root), Some(bob.member()));
    assert_eq!(
        alice.recipient_for_root(alice.personae_root()),
        Some(alice.member())
    );
    assert_eq!(
        bob.recipient_for_root(alice.personae_root()),
        Some(alice.member())
    );
    let stranger = InMemoryProvider::from_seed([0xee; 32]);
    assert_eq!(
        alice.recipient_for_root(stranger.master_public_key().to_bytes()),
        None
    );
}

#[test]
fn a_removed_member_still_resolves_but_is_not_a_member() {
    let alice_identity = InMemoryProvider::from_seed([0xa1; 32]);
    let bob_identity = InMemoryProvider::from_seed([0xb2; 32]);
    let (mut alice, _) = GroupSession::new(LOOKUP_GROUP, &alice_identity).unwrap();
    let (bob, bob_prekey) = GroupSession::new(LOOKUP_GROUP, &bob_identity).unwrap();
    alice.register_prekey(&bob_prekey).unwrap();
    alice.create(&[]).unwrap();
    alice.add(bob.member()).unwrap();
    alice.remove(bob.member()).unwrap();

    let bob_root = bob.personae_root();
    assert_eq!(alice.recipient_for_root(bob_root), Some(bob.member()));
    assert!(!alice.members().unwrap().contains(&bob.member()));
}

#[test]
fn a_root_with_two_recipients_resolves_to_the_current_member() {
    let alice_identity = InMemoryProvider::from_seed([0xa1; 32]);
    let bob_identity = InMemoryProvider::from_seed([0xb2; 32]);
    let (mut alice, _) = GroupSession::new(LOOKUP_GROUP, &alice_identity).unwrap();
    let (first, first_prekey) = GroupSession::new(LOOKUP_GROUP, &bob_identity).unwrap();
    let (second, second_prekey) = GroupSession::new(LOOKUP_GROUP, &bob_identity).unwrap();
    let bob_root = first.personae_root();
    alice.register_prekey(&first_prekey).unwrap();
    assert_eq!(
        alice.recipient_for_root(bob_root),
        Some(first.member()),
        "one recipient resolves before the group exists"
    );

    alice.register_prekey(&second_prekey).unwrap();
    assert_ne!(first.member(), second.member());
    assert_eq!(
        alice.recipient_for_root(bob_root),
        None,
        "two recipients, neither of them a member"
    );

    alice.create(&[]).unwrap();
    alice.add(second.member()).unwrap();
    assert_eq!(
        alice.recipient_for_root(bob_root),
        Some(second.member()),
        "the member among the two"
    );

    alice.add(first.member()).unwrap();
    assert_eq!(
        alice.recipient_for_root(bob_root),
        None,
        "two recipients, both of them members"
    );

    alice.remove(first.member()).unwrap();
    assert_eq!(
        alice.recipient_for_root(bob_root),
        Some(second.member()),
        "one member again"
    );
}

const FORGET_GROUP: GroupSessionId = GroupSessionId([0x48; 32]);

/// A founder alone through three epochs, with one ciphertext per epoch.
fn rotated_founder() -> (
    GroupSession,
    GroupPrekeyBundle,
    Vec<(GroupSecretId, GroupCiphertext)>,
) {
    let identity = InMemoryProvider::from_seed([0xa1; 32]);
    let (mut alice, prekey) = GroupSession::new(FORGET_GROUP, &identity).unwrap();
    alice.create(&[]).unwrap();
    let mut sealed = Vec::new();
    for turn in 0..3u8 {
        if turn > 0 {
            alice.update().unwrap();
        }
        let epoch = alice.current_epoch().unwrap();
        sealed.push((epoch, alice.seal_random(&[turn]).unwrap()));
    }
    (alice, prekey, sealed)
}

fn exported_epochs(session: &GroupSession) -> Vec<GroupSecretId> {
    DataKeyring::from_bytes(&session.data_keyring_state().unwrap())
        .unwrap()
        .epochs_oldest_first()
        .unwrap()
        .to_vec()
}

#[test]
fn forgetting_refuses_the_current_epoch() {
    let (mut alice, _, sealed) = rotated_founder();
    let current = alice.current_epoch().unwrap();
    assert!(matches!(
        alice.forget_epochs(&[current]),
        Err(GroupSessionError::CurrentEpochNotForgettable(epoch)) if epoch == current
    ));
    assert_eq!(
        exported_epochs(&alice),
        sealed.iter().map(|(epoch, _)| *epoch).collect::<Vec<_>>()
    );
}

#[test]
fn forgetting_refuses_an_epoch_the_session_does_not_hold() {
    let (mut alice, _, _) = rotated_founder();
    let unheld = [0xee; 32];
    assert!(matches!(
        alice.forget_epochs(&[unheld]),
        Err(GroupSessionError::EpochNotHeld(epoch)) if epoch == unheld
    ));
    assert_eq!(alice.epoch_count(), 3);
}

#[test]
fn a_mixed_forget_list_changes_nothing() {
    let (mut alice, _, sealed) = rotated_founder();
    let all: Vec<_> = sealed.iter().map(|(epoch, _)| *epoch).collect();
    let (first, current) = (all[0], all[2]);
    for (list, refused) in [
        (vec![first, [0xee; 32]], [0xee; 32]),
        (vec![first, current], current),
        (vec![first, first], first),
    ] {
        let error = alice.forget_epochs(&list).unwrap_err();
        assert!(
            matches!(
                error,
                GroupSessionError::EpochNotHeld(epoch)
                    | GroupSessionError::CurrentEpochNotForgettable(epoch)
                    if epoch == refused
            ),
            "{error}"
        );
        assert_eq!(
            exported_epochs(&alice),
            all,
            "the valid prefix did not apply"
        );
    }
    assert_eq!(alice.open(&sealed[0].1).unwrap(), [0]);
}

#[test]
fn a_forgotten_epoch_leaves_exported_state_and_later_welcomes() {
    let (mut alice, alice_prekey, sealed) = rotated_founder();
    let [
        (first, under_first),
        (second, under_second),
        (third, under_third),
    ] = <[_; 3]>::try_from(sealed).unwrap();
    alice.forget_epochs(&[first]).unwrap();

    assert_eq!(exported_epochs(&alice), [second, third]);
    let reopened = GroupSession::from_bytes(&alice.to_bytes().unwrap()).unwrap();
    assert_eq!(exported_epochs(&reopened), [second, third]);
    assert!(matches!(
        reopened.open(&under_first),
        Err(GroupSessionError::Crypto(GroupCryptoError::UnknownEpoch(epoch))) if epoch == first
    ));
    assert_eq!(reopened.open(&under_second).unwrap(), [1]);
    assert_eq!(reopened.open(&under_third).unwrap(), [2]);

    let dave_identity = InMemoryProvider::from_seed([0xd4; 32]);
    let (mut dave, dave_prekey) = GroupSession::new(FORGET_GROUP, &dave_identity).unwrap();
    alice.register_prekey(&dave_prekey).unwrap();
    dave.register_prekey(&alice_prekey).unwrap();
    let welcome = alice.add(dave.member()).unwrap();
    let welcomed = dave
        .process(
            alice.personae_root(),
            &welcome.control,
            welcome.direct_for(dave.member()),
        )
        .unwrap();
    assert_eq!(welcomed.installed_epochs, [second, third]);
    assert_eq!(exported_epochs(&dave), [second, third]);
    assert!(dave.open(&under_first).is_err());
    assert_eq!(dave.open(&under_second).unwrap(), [1]);
}
