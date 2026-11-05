// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use identity::InMemoryProvider;
use muniment::MemoryBackend;
use pollster::block_on;

use super::*;

const GROUP: GroupSessionId = GroupSessionId([0x4c; 32]);

struct Peer {
    lane: GroupKeyLane<MemoryBackend>,
    session: MessageSession,
}

fn drain(peer: &mut Peer) -> GroupKeyDrain {
    block_on(
        peer.lane
            .drain_ready(&mut peer.session, |session| session.to_bytes().map(drop)),
    )
    .unwrap()
}

#[test]
fn a_forward_secure_conversation_runs_over_the_group_key_lane() {
    let make = |seed: u8| {
        let identity = InMemoryProvider::from_seed([seed; 32]);
        let lane = GroupKeyLane::for_identity(MemoryBackend::new(), GROUP, &identity).unwrap();
        let session = MessageSession::new(GROUP, &identity).unwrap();
        (Peer { lane, session }, identity)
    };
    let (mut alice, alice_id) = make(0xa1);
    let (mut bob, bob_id) = make(0xb2);
    let (a, b) = (alice.session.member(), bob.session.member());
    let for_alice = bob.session.prekeys_for(&bob_id, a, 2).unwrap();
    let for_bob = alice.session.prekeys_for(&alice_id, b, 2).unwrap();
    alice.session.register_prekeys(&for_alice).unwrap();
    bob.session.register_prekeys(&for_bob).unwrap();

    let created = alice.session.create(&[b]).unwrap();
    let created_op = block_on(alice.lane.author_dispatch(&created)).unwrap();
    block_on(bob.lane.accept(&created_op)).unwrap();

    // Bob applies the create and owes the group an acknowledgement.
    let drained = drain(&mut bob);
    assert_eq!(drained.applied.len(), 1);
    let ack = drained.applied[0].process.outbound.clone().unwrap();
    let ack_op = block_on(bob.lane.author_dispatch(&ack)).unwrap();
    block_on(alice.lane.accept(&ack_op)).unwrap();
    let drained = drain(&mut alice);
    assert_eq!(drained.applied.len(), 1);
    assert_eq!(drained.superseded.len(), 1);

    let hello = alice.session.seal_message(b"over the lane").unwrap();
    assert_eq!(
        bob.session.open_message(a, &hello).unwrap(),
        b"over the lane"
    );
    let reply = bob.session.seal_message(b"and back").unwrap();
    assert_eq!(alice.session.open_message(b, &reply).unwrap(), b"and back");

    // A second drain finds everything settled, including the local frames.
    assert!(drain(&mut alice).applied.is_empty());
    assert!(drain(&mut bob).applied.is_empty());
}
