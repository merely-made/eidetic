// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[test]
fn an_unauthorized_subject_is_refused_and_nothing_applies() {
    let gate = Gate::new();
    let granted = subject(1);
    let intruder = subject(2);
    let auth = authority(granted);
    let mut nested = GraphLog::<Container, Relation>::new();

    let rev = nested.revision();
    let err = gate
        .petition(
            &auth,
            &mut nested,
            intruder,
            &trail_scope(),
            rev,
            vec![insert("trail/x")],
        )
        .unwrap_err();
    assert_eq!(
        err,
        GateError::Unauthorized {
            path: "trail".into()
        }
    );
    assert_eq!(nested.graph().node_count(), 0, "nothing applied");
}

#[test]
fn a_petition_outside_the_claimed_path_is_refused() {
    let gate = Gate::new();
    let sub = subject(1);
    let auth = authority(sub);
    let mut nested = GraphLog::<Container, Relation>::new();

    let rev = nested.revision();
    let err = gate
        .petition(
            &auth,
            &mut nested,
            sub,
            &trail_scope(),
            rev,
            vec![insert("notes/sneaky")],
        )
        .unwrap_err();
    assert_eq!(
        err,
        GateError::OutOfScope {
            node: "notes/sneaky".into(),
            path: "trail".into()
        }
    );
    assert_eq!(nested.graph().node_count(), 0);
}

#[test]
fn a_facet_write_needs_authority_on_both_node_scope_and_facet_namespace() {
    let gate = Gate::new();
    let sub = subject(1);
    let auth = authority(sub);
    let mut nested = GraphLog::<Container, Relation>::new();
    gate.petition(
        &auth,
        &mut nested,
        sub,
        &trail_scope(),
        0,
        vec![insert("trail/step1")],
    )
    .unwrap();

    let rev = nested.revision();
    let err = gate
        .petition(
            &auth,
            &mut nested,
            sub,
            &trail_scope(),
            rev,
            vec![EditSpec::SetFacet {
                node: "trail/step1".into(),
                facet: FacetId::new("web.viewer"),
                value: "genet".into(),
            }],
        )
        .unwrap_err();
    assert_eq!(
        err,
        GateError::UnauthorizedFacet {
            facet: "web.viewer".into()
        }
    );
    assert!(
        nested
            .facets()
            .get(&"trail/step1".to_string(), &FacetId::new("web.viewer"))
            .is_none(),
        "node scope alone never confers a facet namespace"
    );
}

#[test]
fn a_web_namespace_grant_permits_web_and_refuses_denizen_on_the_same_node() {
    let gate = Gate::new();
    let sub = subject(1);
    let auth = authority_with_facet(sub, "web.");
    let mut nested = GraphLog::<Container, Relation>::new();
    gate.petition(
        &auth,
        &mut nested,
        sub,
        &trail_scope(),
        0,
        vec![insert("trail/step1")],
    )
    .unwrap();

    let rev = nested.revision();
    gate.petition(
        &auth,
        &mut nested,
        sub,
        &trail_scope(),
        rev,
        vec![EditSpec::SetFacet {
            node: "trail/step1".into(),
            facet: FacetId::new("web.viewer"),
            value: "genet".into(),
        }],
    )
    .unwrap();

    let rev = nested.revision();
    let err = gate
        .petition(
            &auth,
            &mut nested,
            sub,
            &trail_scope(),
            rev,
            vec![EditSpec::SetFacet {
                node: "trail/step1".into(),
                facet: FacetId::new("denizen.binding"),
                value: "forged".into(),
            }],
        )
        .unwrap_err();
    assert_eq!(
        err,
        GateError::UnauthorizedFacet {
            facet: "denizen.binding".into()
        }
    );

    let rev = nested.revision();
    let err = gate
        .petition(
            &auth,
            &mut nested,
            sub,
            &trail_scope(),
            rev,
            vec![EditSpec::RemoveFacet {
                node: "trail/step1".into(),
                facet: FacetId::new("denizen.binding"),
            }],
        )
        .unwrap_err();
    assert_eq!(
        err,
        GateError::UnauthorizedFacet {
            facet: "denizen.binding".into()
        },
        "removal is a facet write too"
    );
}

#[test]
fn a_denizen_cannot_touch_its_own_grant_projection() {
    let gate = Gate::new();
    let sub = subject(1);
    // Grant the participant the reserved namespace itself — the guard still bites.
    // Grant the participant the reserved namespace as a scope; the guard still bites.
    let auth =
        GrantTable::new().with_grant(Grant::new(sub, Cap::scope("grant:").unwrap(), Mode::Write));
    let mut nested = GraphLog::<Container, Relation>::new();
    gate.project_grant(&mut nested, &Grant::new(sub, trail(), Mode::Write))
        .unwrap();

    let rev = nested.revision();
    let err = gate
        .petition(
            &auth,
            &mut nested,
            sub,
            &ScopePath::parse("grant:").unwrap(),
            rev,
            vec![EditSpec::RemoveNode(Gate::projection_id(&trail()))],
        )
        .unwrap_err();
    assert_eq!(
        err,
        GateError::TouchesProjection {
            node: Gate::projection_id(&trail())
        }
    );
    assert!(
        nested
            .graph()
            .key_of(&Gate::projection_id(&trail()))
            .is_some(),
        "the projection survived the attempt"
    );
}
