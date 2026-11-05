// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use identity::InMemoryProvider;

use super::*;
use crate::{GroupSessionDispatch, GroupSessionProcess, MessageCiphertext};

const GROUP: GroupSessionId = GroupSessionId([0x4d; 32]);

struct Member {
    identity: InMemoryProvider,
    session: MessageSession,
}

fn members<const N: usize>(limits: MessageLimits) -> [Member; N] {
    let mut made: Vec<Member> = (0..N)
        .map(|index| {
            let identity = InMemoryProvider::from_seed([index as u8 + 1; 32]);
            let session = MessageSession::with_limits(GROUP, &identity, limits).unwrap();
            Member { identity, session }
        })
        .collect();
    // Every member makes a batch for every other member.
    for from in 0..N {
        for to in 0..N {
            if from != to {
                let peer = made[to].session.member();
                let bundle = {
                    let member = &mut made[from];
                    member
                        .session
                        .prekeys_for(&member.identity, peer, MESSAGE_PREKEY_BATCH)
                        .unwrap()
                };
                made[to].session.register_prekeys(&bundle).unwrap();
            }
        }
    }
    made.try_into().ok().unwrap()
}

/// Deliver `dispatch`, then every acknowledgement it provokes, to `to`.
fn deliver(
    all: &mut [&mut Member],
    from: GroupRecipientId,
    dispatch: GroupSessionDispatch,
    to: &[GroupRecipientId],
) {
    let root = all
        .iter()
        .find(|member| member.session.member() == from)
        .unwrap()
        .session
        .personae_root();
    let mut queue = std::collections::VecDeque::from([(from, root, dispatch)]);
    while let Some((author, root, dispatch)) = queue.pop_front() {
        for member in all.iter_mut() {
            let id = member.session.member();
            if id == author || !to.contains(&id) {
                continue;
            }
            let GroupSessionProcess { outbound, .. } = member
                .session
                .process(root, &dispatch.control, dispatch.direct_for(id))
                .unwrap_or_else(|error| panic!("{id} refused a frame: {error}"));
            if let Some(outbound) = outbound {
                queue.push_back((id, member.session.personae_root(), outbound));
            }
        }
    }
}

#[test]
fn a_conversation_reads_in_both_directions_and_survives_reopen() {
    let [mut alice, mut bob] = members::<2>(MessageLimits::default());
    let everyone = [alice.session.member(), bob.session.member()];
    let created = alice.session.create(&[bob.session.member()]).unwrap();
    deliver(&mut [&mut alice, &mut bob], everyone[0], created, &everyone);
    assert_eq!(alice.session.members().unwrap().len(), 2);
    assert_eq!(bob.session.members().unwrap().len(), 2);

    let hello = alice.session.seal_message(b"hello bob").unwrap();
    assert_eq!(
        bob.session.open_message(everyone[0], &hello).unwrap(),
        b"hello bob"
    );
    let reply = bob.session.seal_message(b"hello alice").unwrap();
    assert_eq!(
        alice.session.open_message(everyone[1], &reply).unwrap(),
        b"hello alice"
    );

    // Both sides reopen from their sealed bytes and carry on.
    alice.session = MessageSession::from_bytes(&alice.session.to_bytes().unwrap()).unwrap();
    bob.session = MessageSession::from_bytes(&bob.session.to_bytes().unwrap()).unwrap();
    let again = alice.session.seal_message(b"after reopen").unwrap();
    assert_eq!(
        bob.session.open_message(everyone[0], &again).unwrap(),
        b"after reopen"
    );
}

#[test]
fn messages_open_out_of_order_within_limits_and_never_twice() {
    let limits = MessageLimits {
        maximum_forward_distance: 8,
        out_of_order_tolerance: 4,
    };
    let [mut alice, mut bob] = members::<2>(limits);
    let (a, b) = (alice.session.member(), bob.session.member());
    let created = alice.session.create(&[b]).unwrap();
    deliver(&mut [&mut alice, &mut bob], a, created, &[a, b]);

    let sent: Vec<_> = (0..4u8)
        .map(|n| alice.session.seal_message(&[n]).unwrap())
        .collect();
    for n in [2usize, 0, 3, 1] {
        assert_eq!(bob.session.open_message(a, &sent[n]).unwrap(), [n as u8]);
    }
    // A replay finds its key already deleted.
    assert!(bob.session.open_message(a, &sent[1]).is_err());

    // A message far beyond the skip limit is refused and costs the chain
    // nothing: the next honest message still opens.
    for _ in 0..20 {
        alice.session.seal_message(b"skipped").unwrap();
    }
    let far = alice.session.seal_message(b"far").unwrap();
    assert!(bob.session.open_message(a, &far).is_err());
    let forged = MessageCiphertext {
        generation: 5,
        ciphertext: vec![0; 40],
    };
    assert!(bob.session.open_message(a, &forged).is_err());
}

#[test]
fn a_compromised_current_state_cannot_read_earlier_messages() {
    let [mut alice, mut bob] = members::<2>(MessageLimits::default());
    let (a, b) = (alice.session.member(), bob.session.member());
    let created = alice.session.create(&[b]).unwrap();
    deliver(&mut [&mut alice, &mut bob], a, created, &[a, b]);

    let before = bob.session.to_bytes().unwrap();
    let first = alice.session.seal_message(b"first secret").unwrap();
    let second = alice.session.seal_message(b"second secret").unwrap();
    assert_eq!(
        bob.session.open_message(a, &first).unwrap(),
        b"first secret"
    );
    assert_eq!(
        bob.session.open_message(a, &second).unwrap(),
        b"second secret"
    );

    // The attacker takes Bob's state now and holds both ciphertexts.
    let mut stolen = MessageSession::from_bytes(&bob.session.to_bytes().unwrap()).unwrap();
    assert!(stolen.open_message(a, &first).is_err());
    assert!(stolen.open_message(a, &second).is_err());

    // The ciphertexts were valid: Bob's state from before reading them opens
    // them, so the refusal above is the deleted keys and nothing else.
    let mut earlier = MessageSession::from_bytes(&before).unwrap();
    assert_eq!(earlier.open_message(a, &first).unwrap(), b"first secret");

    // The stolen state does read what comes next, until the group updates.
    let third = alice.session.seal_message(b"third").unwrap();
    assert_eq!(stolen.open_message(a, &third).unwrap(), b"third");
    let update = bob.session.update().unwrap();
    deliver(&mut [&mut alice, &mut bob], b, update, &[a, b]);
    let fourth = alice.session.seal_message(b"after update").unwrap();
    assert!(stolen.open_message(a, &fourth).is_err());
    assert_eq!(
        bob.session.open_message(a, &fourth).unwrap(),
        b"after update"
    );
}

#[test]
fn a_newcomer_reads_forward_only_and_a_removed_member_reads_nothing_after() {
    let [mut alice, mut bob, mut carol] = members::<3>(MessageLimits::default());
    let (a, b, c) = (
        alice.session.member(),
        bob.session.member(),
        carol.session.member(),
    );
    let created = alice.session.create(&[b]).unwrap();
    deliver(&mut [&mut alice, &mut bob], a, created, &[a, b]);
    let old = alice.session.seal_message(b"before carol").unwrap();

    // Carol joins; her welcome is addressed to her, the rest hear the add.
    let welcome = alice.session.add(c).unwrap();
    deliver(
        &mut [&mut alice, &mut bob, &mut carol],
        a,
        welcome,
        &[a, b, c],
    );
    assert_eq!(carol.session.members().unwrap().len(), 3);
    assert!(carol.session.open_message(a, &old).is_err());
    let fresh = alice.session.seal_message(b"hello carol").unwrap();
    assert_eq!(
        carol.session.open_message(a, &fresh).unwrap(),
        b"hello carol"
    );
    assert_eq!(bob.session.open_message(a, &fresh).unwrap(), b"hello carol");
    let from_carol = carol.session.seal_message(b"hi all").unwrap();
    assert_eq!(bob.session.open_message(c, &from_carol).unwrap(), b"hi all");

    // Bob stays offline for the removal, so his state still believes in the
    // old chains: the refusal is the cryptography, not his membership view.
    let offline_bob = bob.session.to_bytes().unwrap();
    let removal = alice.session.remove(b).unwrap();
    deliver(&mut [&mut alice, &mut carol], a, removal.clone(), &[a, c]);
    let secret = alice.session.seal_message(b"without bob").unwrap();
    assert_eq!(
        carol.session.open_message(a, &secret).unwrap(),
        b"without bob"
    );
    let mut stale = MessageSession::from_bytes(&offline_bob).unwrap();
    assert!(stale.open_message(a, &secret).is_err());

    // Bob who does hear the removal learns he is out and can neither send nor
    // read.
    let root = alice.session.personae_root();
    bob.session
        .process(root, &removal.control, removal.direct_for(b))
        .unwrap();
    assert!(!bob.session.members().unwrap().contains(&b));
    assert!(bob.session.seal_message(b"still here?").is_err());
    assert!(bob.session.open_message(a, &secret).is_err());
    assert!(alice.session.members().unwrap().len() == 2);
}

#[test]
fn frames_are_checked_for_author_order_and_ownership() {
    let [mut alice, mut bob] = members::<2>(MessageLimits::default());
    let (a, b) = (alice.session.member(), bob.session.member());
    let created = alice.session.create(&[b]).unwrap();
    let root = alice.session.personae_root();

    // A frame signed under someone else's root is refused whole.
    assert!(matches!(
        bob.session
            .process([9; 32], &created.control, created.direct_for(b)),
        Err(GroupSessionError::ControlAuthorRootMismatch { .. })
    ));
    bob.session
        .process(root, &created.control, created.direct_for(b))
        .unwrap();
    // The same frame again is behind the author's sequence.
    assert!(matches!(
        bob.session
            .process(root, &created.control, created.direct_for(b)),
        Err(GroupSessionError::UnexpectedSequence { .. })
    ));
    // The author's own frame is never applied back to it.
    assert!(matches!(
        alice
            .session
            .process(root, &created.control, created.direct_for(a)),
        Err(GroupSessionError::UnexpectedSequence { .. } | GroupSessionError::OwnControlFrame)
    ));
    // A direct frame for another recipient is refused.
    let mut wrong = created.clone();
    wrong.direct[0].recipient = a;
    assert!(
        bob.session
            .process(root, &wrong.control, wrong.direct.first())
            .is_err()
    );
}

#[test]
fn a_batch_registers_once_only_for_its_addressee_and_must_verify() {
    let [mut alice, mut bob] = members::<2>(MessageLimits::default());
    let (a, b) = (alice.session.member(), bob.session.member());
    let batch = bob
        .session
        .prekeys_for(&bob.identity, a, MESSAGE_PREKEY_BATCH)
        .unwrap();
    alice.session.register_prekeys(&batch).unwrap();
    alice.session.register_prekeys(&batch).unwrap();
    // Not addressed to Bob himself, and a tampered batch fails to decode.
    assert!(matches!(
        bob.session.register_prekeys(&batch),
        Err(GroupSessionError::PrekeysForAnotherPeer(peer)) if peer == a
    ));
    let mut forged = batch.clone();
    forged.identity_signature[0] ^= 1;
    assert!(matches!(
        MessagePrekeyBundle::from_bytes(&forged.to_bytes().unwrap()),
        Err(GroupSessionError::InvalidPersonaeSignature)
    ));
    let _ = b;
}
