// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;
use crate::grant::GrantTable;
use chartulary::{AssertionBasis, Container, FacetId, RelationClass};

fn subject(tag: u8) -> Subject {
    Subject::new([tag; 32])
}

fn authority(subject: Subject) -> GrantTable {
    GrantTable::new().with_grant(Grant::new(subject, trail(), Mode::Write))
}

fn authority_with_facet(subject: Subject, namespace: &str) -> GrantTable {
    authority(subject).with_grant(Grant::new(
        subject,
        Cap::facet(namespace).unwrap(),
        Mode::Write,
    ))
}

/// The capability every test petitions under.
fn trail() -> Cap {
    Cap::scope("trail").unwrap()
}

fn trail_scope() -> ScopePath {
    ScopePath::parse("trail").unwrap()
}

fn insert(id: &str) -> EditSpec<Container, Relation> {
    EditSpec::InsertNode(Container::new(id))
}

fn assert_relation(from: &str, to: &str) -> EditSpec<Container, Relation> {
    EditSpec::Assert {
        from: from.to_string(),
        to: to.to_string(),
        edge: Relation::new(RelationClass::app("servitor-test", 0)),
        asserter: Some(Author::new("source-owner")),
        at_ms: Some(7),
        basis: AssertionBasis::default(),
    }
}

/// Seed a source-owned assertion directly so the gate tests admission of
/// later exact-ID operations against a record already in the graph.
fn seed_assertion(
    nested: &mut GraphLog<Container, Relation>,
    from: &str,
    to: &str,
) -> chartulary::EdgeId {
    let mut specs: Vec<EditSpec<Container, Relation>> = Vec::new();
    for id in [from, to] {
        if nested.graph().key_of(&id.to_string()).is_none()
            && !specs
                .iter()
                .any(|spec| matches!(spec, EditSpec::InsertNode(node) if node.id == id))
        {
            specs.push(insert(id));
        }
    }
    specs.push(assert_relation(from, to));
    nested
        .commit_batch(Author::new("source-owner"), nested.revision(), specs)
        .expect("seed assertion commits")
        .edges[0]
}

fn exact_id_operations(id: chartulary::EdgeId) -> [EditSpec<Container, Relation>; 3] {
    [
        EditSpec::Disconnect(id),
        EditSpec::ConfirmAssertion { id, at_ms: Some(8) },
        EditSpec::WithdrawAssertion { id, at_ms: Some(9) },
    ]
}

#[test]
fn a_grant_projects_read_only_and_attributed_to_the_gate() {
    let gate = Gate::new();
    let mut nested = GraphLog::<Container, Relation>::new();
    let grant = Grant::new(subject(1), trail(), Mode::Write);

    let committed = gate.project_grant(&mut nested, &grant).unwrap();
    let entry = &nested.log().entries()[committed.batch.0 as usize];
    assert_eq!(
        entry.author,
        Author::new("gate"),
        "projection is the gate's, not the denizen's"
    );
    assert!(
        nested
            .graph()
            .key_of(&Gate::projection_id(&trail()))
            .is_some(),
        "the projection node exists in the nested graph"
    );
}

#[test]
fn a_projection_round_trips_every_field_including_a_read_mode() {
    // F3, the bug this encoding exists to kill: the mode used to live only
    // in the display title, and readers hardcoded Write, so a Read grant
    // came back as Write after a restart.
    let gate = Gate::new();
    for (cap, mode) in [
        (trail(), Mode::Read),
        (trail(), Mode::Write),
        (Cap::power("navigate").unwrap(), Mode::Delegate),
        (Cap::facet("web.").unwrap(), Mode::Write),
        (Cap::root_scope(), Mode::Write),
    ] {
        let mut nested = GraphLog::<Container, Relation>::new();
        // Bounded on the Delegate case, so expiry rides the round trip too.
        let grant = match mode {
            Mode::Delegate => Grant::new(subject(7), cap.clone(), mode).expiring_at(1_234),
            _ => Grant::new(subject(7), cap.clone(), mode),
        };
        gate.project_grant(&mut nested, &grant).unwrap();

        let key = nested.graph().key_of(&Gate::projection_id(&cap)).unwrap();
        let node = nested.graph().node(key).unwrap();
        assert_eq!(
            read_projection(node),
            Some(grant),
            "{cap:?} at {mode:?} must survive the round trip exactly"
        );
    }
}

#[test]
fn a_node_that_is_not_a_projection_reads_as_none() {
    // Fails closed: an ordinary node, and a mistagged one missing fields,
    // both yield no grant rather than a guessed one.
    assert_eq!(read_projection(&Container::new("trail/step1")), None);
    let partial = Container::new("grant:scope:trail").with_tag(PROJECTION_TAG);
    assert_eq!(read_projection(&partial), None, "no cap/mode/subject tags");
}

#[test]
fn an_in_scope_petition_commits_attributed_to_the_denizen() {
    let gate = Gate::new();
    let sub = subject(1);
    let auth = authority(sub);
    let mut nested = GraphLog::<Container, Relation>::new();

    let rev = nested.revision();
    let committed = gate
        .petition(
            &auth,
            &mut nested,
            sub,
            &trail_scope(),
            rev,
            vec![insert("trail/step1"), insert("trail/step2")],
        )
        .unwrap();

    let entry = &nested.log().entries()[committed.batch.0 as usize];
    assert_eq!(
        entry.author,
        sub.to_author(),
        "the journal attributes the change to the denizen"
    );
    assert_eq!(nested.graph().node_count(), 2);
}

#[test]
fn an_assertion_must_keep_both_endpoints_inside_the_claimed_scope() {
    let gate = Gate::new();
    let sub = subject(1);
    let auth = authority(sub);
    let mut nested = GraphLog::<Container, Relation>::new();
    let revision = nested.revision();

    let error = gate
        .petition(
            &auth,
            &mut nested,
            sub,
            &trail_scope(),
            revision,
            vec![assert_relation("trail/inside", "notes/outside")],
        )
        .unwrap_err();

    assert_eq!(
        error,
        GateError::OutOfScope {
            node: "notes/outside".into(),
            path: "trail".into(),
        }
    );
    assert_eq!(nested.revision(), revision);
    assert_eq!(nested.graph().node_count(), 0);
    assert_eq!(nested.assertions().count(), 0);
}

#[test]
fn an_in_scope_denizen_can_assert_with_separate_source_attribution() {
    let gate = Gate::new();
    let sub = subject(1);
    let auth = authority(sub);
    let mut nested = GraphLog::<Container, Relation>::new();
    let mut assertion = assert_relation("trail/one", "trail/two");
    if let EditSpec::Assert {
        asserter,
        at_ms,
        basis,
        ..
    } = &mut assertion
    {
        *asserter = Some(Author::new("independent-source"));
        *at_ms = Some(12);
        basis.note = Some("because the source says so".into());
    }

    let committed = gate
        .petition(
            &auth,
            &mut nested,
            sub,
            &trail_scope(),
            0,
            vec![insert("trail/one"), insert("trail/two"), assertion],
        )
        .unwrap();
    let id = committed.edges[0];
    let record = nested.assertion(id).expect("assertion is retained");
    assert_eq!(record.asserter, Some(Author::new("independent-source")));
    assert_eq!(record.span.first_asserted_ms, Some(12));
    assert_eq!(
        record.basis.note.as_deref(),
        Some("because the source says so")
    );
    assert_eq!(
        nested.log().entries()[committed.batch.0 as usize].author,
        sub.to_author(),
        "the gate still attributes admission to the acting denizen"
    );
}

#[test]
fn exact_id_edits_check_both_recorded_endpoints_for_scope() {
    let gate = Gate::new();
    let sub = subject(1);
    let auth = authority(sub);

    let mut nested = GraphLog::<Container, Relation>::new();
    let id = seed_assertion(&mut nested, "trail/inside", "notes/outside");
    let revision = nested.revision();
    for operation in exact_id_operations(id) {
        let error = gate
            .petition(
                &auth,
                &mut nested,
                sub,
                &trail_scope(),
                revision,
                vec![operation],
            )
            .unwrap_err();

        assert_eq!(
            error,
            GateError::OutOfScope {
                node: "notes/outside".into(),
                path: "trail".into(),
            }
        );
        assert_eq!(nested.revision(), revision, "refusal is atomic");
        assert!(nested.assertion(id).unwrap().is_held());
    }
}

#[test]
fn exact_id_edits_cannot_touch_grant_projection_endpoints() {
    let gate = Gate::new();
    let sub = subject(1);
    let auth = authority(sub);
    let projection = Gate::projection_id(&trail());
    let mut nested = GraphLog::<Container, Relation>::new();
    gate.project_grant(&mut nested, &Grant::new(subject(2), trail(), Mode::Write))
        .unwrap();
    let id = seed_assertion(&mut nested, "trail/inside", &projection);
    let revision = nested.revision();
    for operation in exact_id_operations(id) {
        let error = gate
            .petition(
                &auth,
                &mut nested,
                sub,
                &trail_scope(),
                revision,
                vec![operation],
            )
            .unwrap_err();

        assert_eq!(
            error,
            GateError::TouchesProjection {
                node: projection.clone()
            }
        );
        assert_eq!(nested.revision(), revision, "refusal is atomic");
        assert!(nested.assertion(id).unwrap().is_held());
    }
}

#[test]
fn an_unknown_assertion_id_reaches_the_atomic_commit_error() {
    let gate = Gate::new();
    let sub = subject(1);
    let auth = authority(sub);
    let mut nested = GraphLog::<Container, Relation>::new();
    let unknown = chartulary::EdgeId::local(99);
    let revision = nested.revision();

    let error = gate
        .petition(
            &auth,
            &mut nested,
            sub,
            &trail_scope(),
            revision,
            vec![EditSpec::WithdrawAssertion {
                id: unknown,
                at_ms: Some(3),
            }],
        )
        .unwrap_err();

    assert!(matches!(
        error,
        GateError::Commit(CommitError::UnknownAssertion(id)) if id == unknown
    ));
    assert_eq!(nested.revision(), revision);
    assert_eq!(nested.assertions().count(), 0);
}

#[test]
fn a_stale_petition_surfaces_the_revision_conflict() {
    let gate = Gate::new();
    let sub = subject(1);
    let auth = authority(sub);
    let mut nested = GraphLog::<Container, Relation>::new();
    let stale = nested.revision();
    // A concurrent commit moves the revision past `stale`.
    gate.petition(
        &auth,
        &mut nested,
        sub,
        &trail_scope(),
        stale,
        vec![insert("trail/a")],
    )
    .unwrap();

    let err = gate
        .petition(
            &auth,
            &mut nested,
            sub,
            &trail_scope(),
            stale,
            vec![insert("trail/b")],
        )
        .unwrap_err();
    match err {
        GateError::Commit(CommitError::RevisionConflict { current }) => {
            assert_eq!(
                current,
                stale + 1,
                "the denizen learns the revision to rebase onto"
            );
        }
        other => panic!("expected a revision conflict, got {other:?}"),
    }
}

#[test]
fn a_slow_limit_cycle_is_named_before_it_adds_history() {
    let gate = Gate::new();
    let sub = subject(1);
    let auth = authority(sub);
    let mut nested = GraphLog::<Container, Relation>::new();
    let mut deadbands = DeadbandTable::new();
    deadbands.register(sub, crate::Deadband::new(2, 1_000).unwrap());

    gate.petition_behavior(
        &auth,
        &mut deadbands,
        &mut nested,
        sub,
        BehaviorPetition::new(
            &trail_scope(),
            0,
            Actuation::new(0, 0),
            vec![insert("trail/first")],
        ),
    )
    .unwrap();
    let landed = nested.revision();

    // Each pass is a separate, short cascade and arrives long after the
    // interval. A depth budget cannot see the 0/1 oscillation; the output
    // deadband can.
    for (at_ms, output) in [(2_000, 1), (4_000, 0), (6_000, 1)] {
        let error = gate
            .petition_behavior(
                &auth,
                &mut deadbands,
                &mut nested,
                sub,
                BehaviorPetition::new(
                    &trail_scope(),
                    landed,
                    Actuation::new(output, at_ms),
                    vec![insert(&format!("trail/{at_ms}"))],
                ),
            )
            .unwrap_err();
        let GateError::Deadband(refusal) = error else {
            panic!("expected a named deadband refusal, got {error:?}");
        };
        assert_eq!(refusal.subject, sub);
        assert_eq!(refusal.change.unwrap().actual, output.unsigned_abs());
        assert_eq!(refusal.interval, None, "this is the slow-loop case");
        assert_eq!(nested.revision(), landed, "no refusal wrote history");
    }
}

#[test]
fn a_failed_commit_does_not_consume_the_deadband_interval() {
    let gate = Gate::new();
    let sub = subject(1);
    let auth = authority(sub);
    let mut nested = GraphLog::<Container, Relation>::new();
    let mut deadbands = DeadbandTable::new();
    deadbands.register(sub, crate::Deadband::new(5, 1_000).unwrap());
    gate.petition_behavior(
        &auth,
        &mut deadbands,
        &mut nested,
        sub,
        BehaviorPetition::new(
            &trail_scope(),
            0,
            Actuation::new(0, 0),
            vec![insert("trail/first")],
        ),
    )
    .unwrap();

    let stale = nested.revision();
    gate.petition(
        &auth,
        &mut nested,
        sub,
        &trail_scope(),
        stale,
        vec![insert("trail/concurrent")],
    )
    .unwrap();
    let error = gate
        .petition_behavior(
            &auth,
            &mut deadbands,
            &mut nested,
            sub,
            BehaviorPetition::new(
                &trail_scope(),
                stale,
                Actuation::new(10, 1_000),
                vec![insert("trail/retry")],
            ),
        )
        .unwrap_err();
    assert!(matches!(
        error,
        GateError::Commit(CommitError::RevisionConflict { .. })
    ));

    let current = nested.revision();
    gate.petition_behavior(
        &auth,
        &mut deadbands,
        &mut nested,
        sub,
        BehaviorPetition::new(
            &trail_scope(),
            current,
            Actuation::new(10, 1_000),
            vec![insert("trail/retry")],
        ),
    )
    .unwrap();
}

#[path = "tests/authority.rs"]
mod authority;
