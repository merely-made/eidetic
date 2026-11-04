// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;
use identity::delegation::Issue;
use identity::{IdentityProvider, InMemoryProvider};

fn provider(seed: u8) -> InMemoryProvider {
    InMemoryProvider::from_seed([seed; 32])
}

fn root_key(provider: &InMemoryProvider) -> [u8; 32] {
    provider.master_public_key().to_bytes()
}

fn scope(raw: &str) -> Cap {
    Cap::scope(raw).unwrap()
}

fn subject_of(provider: &InMemoryProvider) -> Subject {
    Subject::new(provider.master_public_key().to_bytes())
}

/// The user delegates `trail` at `mode` to the helper, with `depth`
/// further steps allowed.
fn rooted(mode: Mode, depth: u16) -> (DelegationTable, InMemoryProvider, InMemoryProvider) {
    let user = provider(0);
    let helper = provider(1);
    let mut table = DelegationTable::new(root_key(&user));
    let cert = root_certificate(
        root_key(&user),
        subject_of(&helper),
        &scope("trail"),
        mode,
        b"session-1".to_vec(),
        1_000,
        None,
        depth,
        [7; 32],
    );
    table.adopt(SignedDelegationCertificate::issue(&user, cert).unwrap());
    table.set_now(2_000);
    (table, user, helper)
}

#[test]
fn a_root_delegation_covers_within_its_scope() {
    let (table, _user, helper) = rooted(Mode::Write, 0);
    let subject = subject_of(&helper);
    assert!(table.covers(subject, &scope("trail/step"), Mode::Write));
    assert!(
        table.covers(subject, &scope("trail/step"), Mode::Read),
        "write implies read"
    );
    assert!(
        !table.covers(subject, &scope("notes"), Mode::Write),
        "outside the scope"
    );
    assert!(
        !table.covers(subject, &scope("trail/step"), Mode::Delegate),
        "a Write grant confers no delegate action"
    );
}

#[test]
fn powers_stay_closed_through_the_personae_encoding() {
    // The F1 hazard, checked at the ENCODED layer: personae's path match
    // requires a slash boundary, so `power/nav` cannot cover
    // `power/navigate`.
    let user = provider(0);
    let helper = provider(1);
    let mut table = DelegationTable::new(root_key(&user));
    let cert = root_certificate(
        root_key(&user),
        subject_of(&helper),
        &Cap::power("nav").unwrap(),
        Mode::Write,
        b"app".to_vec(),
        1_000,
        None,
        0,
        [9; 32],
    );
    table.adopt(SignedDelegationCertificate::issue(&user, cert).unwrap());
    table.set_now(2_000);

    let subject = subject_of(&helper);
    assert!(table.covers(subject, &Cap::power("nav").unwrap(), Mode::Write));
    assert!(
        !table.covers(subject, &Cap::power("navigate").unwrap(), Mode::Write),
        "a longer power name is a different power"
    );
    assert!(
        !table.covers(subject, &scope("nav"), Mode::Write),
        "a scope spelled like the power is not the power"
    );
    assert_eq!(
        cap_path(&Cap::power("nav/admin").unwrap()),
        "power/nav%2Fadmin",
        "a slash inside a power name cannot manufacture hierarchy"
    );
}

#[test]
fn facet_namespaces_keep_their_dot_hierarchy_through_personae_encoding() {
    let user = provider(0);
    let helper = provider(1);
    let mut table = DelegationTable::new(root_key(&user));
    let cert = root_certificate(
        root_key(&user),
        subject_of(&helper),
        &Cap::facet("web.").unwrap(),
        Mode::Write,
        b"app".to_vec(),
        1_000,
        None,
        0,
        [10; 32],
    );
    table.adopt(SignedDelegationCertificate::issue(&user, cert).unwrap());
    table.set_now(2_000);

    let subject = subject_of(&helper);
    assert!(table.covers(subject, &Cap::facet("web.viewer").unwrap(), Mode::Write));
    assert!(!table.covers(subject, &Cap::facet("website.viewer").unwrap(), Mode::Write));
    assert!(!table.covers(
        subject,
        &Cap::facet("denizen.binding").unwrap(),
        Mode::Write
    ));
    assert!(
        !table.covers(subject, &scope("web/viewer"), Mode::Write),
        "a facet namespace never crosses into the node-scope hierarchy"
    );
    assert_eq!(
        cap_path(&Cap::facet("example.portable/v1").unwrap()),
        "facet/example/portable%2Fv1",
        "a slash inside one dot segment stays inside that segment"
    );
}

#[test]
fn a_forged_root_is_refused() {
    // A certificate signed by someone who is not this table's root.
    let user = provider(0);
    let intruder = provider(5);
    let helper = provider(1);
    let mut table = DelegationTable::new(root_key(&user));
    let cert = root_certificate(
        root_key(&intruder),
        subject_of(&helper),
        &scope("trail"),
        Mode::Write,
        b"session-1".to_vec(),
        1_000,
        None,
        0,
        [3; 32],
    );
    let signed = SignedDelegationCertificate::issue(&intruder, cert).unwrap();
    signed.check().expect("it is a validly SIGNED certificate");
    table.adopt(signed);
    table.set_now(2_000);
    assert!(
        !table.covers(subject_of(&helper), &scope("trail/x"), Mode::Write),
        "signed by the wrong authority: not this table's root"
    );
}

#[test]
fn a_root_link_issued_by_another_key_is_refused() {
    // The intruder names THIS table's root as parent but issues and signs
    // as itself: well signed, but it holds no root authority.
    let user = provider(0);
    let intruder = provider(5);
    let helper = provider(1);
    let mut table = DelegationTable::new(root_key(&user));
    let cert = DelegationCertificate::new(
        DelegationParent::Root(root_key(&user)),
        root_key(&intruder),
        subject_of(&helper).0,
        scope_for(&scope("trail"), Mode::Write, b"session-1".to_vec()),
        1_000,
        1_000,
        None,
        0,
        [4; 32],
    );
    let signed = SignedDelegationCertificate::issue(&intruder, cert).unwrap();
    signed.check().expect("it is a validly SIGNED certificate");
    table.adopt(signed.clone());
    table.set_now(2_000);
    assert!(matches!(
        table.verify_chain(&signed).unwrap_err(),
        ChainError::WrongRoot(_)
    ));
    assert!(!table.covers(subject_of(&helper), &scope("trail/x"), Mode::Write));
}

#[test]
fn a_child_narrows_and_a_widening_child_never_verifies() {
    // The helper holds `trail` at Delegate with one step available.
    let (mut table, _user, helper) = rooted(Mode::Delegate, 1);
    let sub = provider(2);
    let parent_id = table.certificates().next().unwrap().certificate.id();

    // A strict narrowing verifies.
    let narrow = DelegationCertificate::new(
        DelegationParent::Certificate(parent_id),
        helper.master_public_key().to_bytes(),
        subject_of(&sub).0,
        scope_for(&scope("trail/step"), Mode::Write, b"session-1".to_vec()),
        1_000,
        1_000,
        None,
        0,
        [11; 32],
    );
    table.adopt(SignedDelegationCertificate::issue(&helper, narrow).unwrap());
    assert!(table.covers(subject_of(&sub), &scope("trail/step/a"), Mode::Write));
    assert!(!table.covers(subject_of(&sub), &scope("trail/other"), Mode::Write));

    // A widening child (the root scope) does not verify, however well signed.
    let wide = DelegationCertificate::new(
        DelegationParent::Certificate(parent_id),
        helper.master_public_key().to_bytes(),
        subject_of(&sub).0,
        scope_for(&Cap::root_scope(), Mode::Write, b"session-1".to_vec()),
        1_000,
        1_000,
        None,
        0,
        [12; 32],
    );
    let wide_signed = SignedDelegationCertificate::issue(&helper, wide).unwrap();
    wide_signed
        .check()
        .expect("signed correctly, but still not authorized");
    let err = table.verify_chain(&wide_signed).unwrap_err();
    assert!(matches!(err, ChainError::NotAttenuating(_)), "{err:?}");
}

#[test]
fn a_holder_without_delegation_depth_cannot_delegate() {
    // depth 0: the helper may act, never grant.
    let (mut table, _user, helper) = rooted(Mode::Delegate, 0);
    let sub = provider(2);
    let parent_id = table.certificates().next().unwrap().certificate.id();
    let child = DelegationCertificate::new(
        DelegationParent::Certificate(parent_id),
        helper.master_public_key().to_bytes(),
        subject_of(&sub).0,
        scope_for(&scope("trail/step"), Mode::Read, b"session-1".to_vec()),
        1_000,
        1_000,
        None,
        0,
        [13; 32],
    );
    let signed = SignedDelegationCertificate::issue(&helper, child).unwrap();
    table.adopt(signed.clone());
    assert!(matches!(
        table.verify_chain(&signed).unwrap_err(),
        ChainError::NotAttenuating(_)
    ));
    assert!(!table.covers(subject_of(&sub), &scope("trail/step"), Mode::Read));
}

#[test]
fn revoking_a_link_cascades_to_the_whole_subtree() {
    let (mut table, _user, helper) = rooted(Mode::Delegate, 1);
    let sub = provider(2);
    let parent_id = table.certificates().next().unwrap().certificate.id();
    let child = DelegationCertificate::new(
        DelegationParent::Certificate(parent_id),
        helper.master_public_key().to_bytes(),
        subject_of(&sub).0,
        scope_for(&scope("trail/step"), Mode::Write, b"session-1".to_vec()),
        1_000,
        1_000,
        None,
        0,
        [14; 32],
    );
    table.adopt(SignedDelegationCertificate::issue(&helper, child).unwrap());
    assert!(table.covers(subject_of(&helper), &scope("trail/x"), Mode::Write));
    assert!(table.covers(subject_of(&sub), &scope("trail/step/y"), Mode::Write));

    // Revoke the ROOT link only.
    assert!(table.revoke(parent_id));
    assert!(!table.covers(subject_of(&helper), &scope("trail/x"), Mode::Write));
    assert!(
        !table.covers(subject_of(&sub), &scope("trail/step/y"), Mode::Write),
        "the subtree cascades from one revocation, with no marking pass"
    );
}

#[test]
fn revoke_root_grants_is_uninstall() {
    let (mut table, _user, helper) = rooted(Mode::Write, 0);
    assert_eq!(table.revoke_root_grants(subject_of(&helper)), 1);
    assert!(!table.covers(subject_of(&helper), &scope("trail/x"), Mode::Write));
}

#[test]
fn a_bounded_grant_stops_covering_when_the_clock_passes_it() {
    let user = provider(0);
    let helper = provider(1);
    let mut table = DelegationTable::new(root_key(&user));
    let cert = root_certificate(
        root_key(&user),
        subject_of(&helper),
        &scope("trail"),
        Mode::Write,
        b"session-1".to_vec(),
        1_000,
        Some(5_000),
        0,
        [15; 32],
    );
    table.adopt(SignedDelegationCertificate::issue(&user, cert).unwrap());

    table.set_now(4_999);
    assert!(table.covers(subject_of(&helper), &scope("trail/x"), Mode::Write));
    table.set_now(5_001);
    assert!(
        !table.covers(subject_of(&helper), &scope("trail/x"), Mode::Write),
        "expiry needs no mutation of the store"
    );
}

#[test]
fn a_chain_verifies_from_a_cold_store_in_any_order() {
    // Rebuild: certificates arrive out of order and validity is re-derived
    // from the signatures, not trusted because they were stored.
    let (table, user, helper) = rooted(Mode::Delegate, 1);
    let sub = provider(2);
    let parent_id = table.certificates().next().unwrap().certificate.id();
    let child = DelegationCertificate::new(
        DelegationParent::Certificate(parent_id),
        helper.master_public_key().to_bytes(),
        subject_of(&sub).0,
        scope_for(&scope("trail/step"), Mode::Write, b"session-1".to_vec()),
        1_000,
        1_000,
        None,
        0,
        [16; 32],
    );
    let child_signed = SignedDelegationCertificate::issue(&helper, child).unwrap();
    let root_signed = table.certificates().next().unwrap().clone();

    let mut cold = DelegationTable::new(root_key(&user));
    cold.adopt(child_signed); // child first
    cold.adopt(root_signed);
    cold.set_now(2_000);
    assert!(cold.covers(subject_of(&sub), &scope("trail/step/y"), Mode::Write));
}

#[test]
fn a_child_whose_parent_is_absent_does_not_verify() {
    let (table, _user, helper) = rooted(Mode::Delegate, 1);
    let sub = provider(2);
    let orphan = DelegationCertificate::new(
        DelegationParent::Certificate(DelegationId([0xAB; 32])),
        helper.master_public_key().to_bytes(),
        subject_of(&sub).0,
        scope_for(&scope("trail/step"), Mode::Write, b"session-1".to_vec()),
        1_000,
        1_000,
        None,
        0,
        [17; 32],
    );
    let signed = SignedDelegationCertificate::issue(&helper, orphan).unwrap();
    assert!(matches!(
        table.verify_chain(&signed).unwrap_err(),
        ChainError::MissingParent(_)
    ));
}
