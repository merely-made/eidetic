// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

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

fn hashes<'a>(operations: impl IntoIterator<Item = &'a Operation<GroupKeyExt>>) -> Vec<[u8; 32]> {
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
fn remove_and_rotate(founded: &mut Founded) -> (Operation<GroupKeyExt>, Operation<GroupKeyExt>) {
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
    let derived = blake3::hash(&[b"stickleback/group-key-topic/v1/".as_slice(), &GROUP.0].concat());
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
        block_on(unmarked.drain_ready(&mut session, |_| Ok::<_, GroupSessionError>(()))).unwrap();
    assert_eq!(judged.superseded.len(), 4);
}

#[test]
fn failed_persist_can_retry_same_live_session_before_settling() {
    let mut f = caught_up();
    let (removal, rotation) = remove_and_rotate(&mut f);
    f.b.receive([&removal, &rotation]);
    let before = f.b.session.to_bytes().unwrap();

    let failed = block_on(
        f.b.lane
            .drain_ready(&mut f.b.session, |_| Err::<(), _>("injected save failure")),
    );
    assert!(matches!(failed, Err(GroupKeyLaneError::Persist(_))));
    assert_eq!(f.b.session.to_bytes().unwrap(), before);

    // The same live session must apply and save the transition on retry.
    let mut saved = None;
    let retried = block_on(f.b.lane.drain_ready(&mut f.b.session, |session| {
        saved = Some(session.to_bytes()?);
        Ok::<_, GroupSessionError>(())
    }))
    .unwrap();
    assert_eq!(retried.applied.len(), 2);
    assert!(retried.superseded.is_empty());
    let saved = saved.expect("retry must persist before settling");

    // A fresh lane/session sees the settled marks and can decrypt data in
    // the epoch installed by the retried transition.
    let after = f.a.session.seal_random(b"after retry").unwrap();
    let Member {
        identity, backend, ..
    } = f.b;
    let lane = GroupKeyLane::for_identity(backend, GROUP, &identity).unwrap();
    let mut reopened = GroupSession::from_bytes(&saved).unwrap();
    let drain =
        block_on(lane.drain_ready(&mut reopened, |_| Ok::<_, GroupSessionError>(()))).unwrap();
    assert_eq!(drain, GroupKeyDrain::default());
    assert_eq!(reopened.open(&after).unwrap(), b"after retry");
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
