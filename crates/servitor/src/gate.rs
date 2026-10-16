// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The gate: one authority pipeline for every petition.
//!
//! A participant proposes a **petition** (a batch of [`EditSpec`]s against its
//! nested graph, claiming to act under a [`ScopePath`]). A petition is always
//! scope-claimed, never power-claimed: powers name app abilities, scopes name
//! places in a graph, and only the second can contain a node id. The gate:
//!
//! 1. refuses any spec that would touch a **grant projection** (the reserved
//!    [`GRANT_PREFIX`] namespace) — a participant can read its grants but never
//!    edit them, so it cannot escalate itself;
//! 2. checks **authority**: the [`AuthorityProvider`] must cover the claimed
//!    path at [`Mode::Write`];
//! 3. checks **scope**: every node a spec touches must fall under the claimed
//!    path;
//! 4. checks **facet authority**: every facet write needs a covering
//!    [`Cap::Facet`] at [`Mode::Write`] as well as node scope;
//! 5. **commits** the batch attributed to the participant, revision-checked, atomic
//!    (chartulary's `commit_batch`).
//!
//! Authority is materialized elsewhere and *projected* here read-only:
//! [`Gate::project_grant`] renders a [`Grant`] into the nested graph as a
//! reserved-namespace node, committed by the gate's own author, so "what may
//! this participant do" is a browsable question answered from the graph itself.

use chartulary::{Author, CommitError, Committed, Container, EditSpec, GraphLog, Relation};

use crate::Subject;
use crate::cap::{Cap, ScopePath};
use crate::deadband::{Actuation, DeadbandRefusal, DeadbandTable};
use crate::grant::{AuthorityProvider, Grant, Mode};

/// The reserved node-id prefix for grant projections. The gate writes these and
/// refuses any petition that touches them, so a participant cannot rewrite its own
/// authority.
///
/// This one IS a string-prefix test, correctly: it reserves an id namespace
/// whose delimiter (`:`) is part of the prefix, so there is no segment
/// ambiguity to exploit (`grantx` does not start with `grant:`). The capability
/// checks below are the ones that had to become typed.
pub const GRANT_PREFIX: &str = "grant:";

/// The tag marking a node as a grant projection.
pub const PROJECTION_TAG: &str = "grant-projection";

/// The media type naming the projection record's schema, so a reader knows the
/// parse before it trusts the tags.
pub const PROJECTION_MEDIA_TYPE: &str = "application/vnd.mere.grant+json";

const CAP_TAG: &str = "cap:";
const MODE_TAG: &str = "mode:";
const SUBJECT_TAG: &str = "subject:";
const EXPIRES_TAG: &str = "expires:";
const EXPIRES_NEVER: &str = "never";

fn mode_wire(mode: Mode) -> &'static str {
    match mode {
        Mode::Read => "read",
        Mode::Write => "write",
        Mode::Delegate => "delegate",
    }
}

fn mode_from_wire(raw: &str) -> Option<Mode> {
    match raw {
        "read" => Some(Mode::Read),
        "write" => Some(Mode::Write),
        "delegate" => Some(Mode::Delegate),
        _ => None,
    }
}

fn tag_value<'a>(node: &'a Container, prefix: &str) -> Option<&'a str> {
    node.tags.iter().find_map(|t| t.strip_prefix(prefix))
}

/// The `cap`/`mode`/`subject`/`expires` tags a grant projects. Shared by the
/// grant and delegation records so their field encoding cannot drift.
fn grant_tags(grant: &Grant) -> Vec<String> {
    vec![
        format!("{CAP_TAG}{}", grant.cap.to_wire()),
        format!("{MODE_TAG}{}", mode_wire(grant.mode)),
        format!("{SUBJECT_TAG}{}", grant.subject.to_hex()),
        format!(
            "{EXPIRES_TAG}{}",
            match grant.expires_at_ms {
                Some(at) => at.to_string(),
                None => EXPIRES_NEVER.to_string(),
            }
        ),
    ]
}

/// Reconstruct the grant a projection node carries, or `None` if a field is
/// missing or unparseable. Shared reader for the grant and delegation records.
fn read_grant(node: &Container) -> Option<Grant> {
    let cap = Cap::parse(tag_value(node, CAP_TAG)?).ok()?;
    let mode = mode_from_wire(tag_value(node, MODE_TAG)?)?;
    let subject = Subject::from_hex(tag_value(node, SUBJECT_TAG)?)?;
    let grant = Grant::new(subject, cap, mode);
    // A record written before expiry existed carries no tag and is
    // open-ended, which is what it was.
    Some(match tag_value(node, EXPIRES_TAG) {
        None | Some(EXPIRES_NEVER) => grant,
        Some(raw) => grant.expiring_at(raw.parse().ok()?),
    })
}

/// Read a grant back out of a projection node, or `None` if the node is not a
/// well-formed projection (not tagged, or missing/unparseable fields).
///
/// Fails closed and loudly-by-absence: a projection this build cannot fully
/// understand yields no grant at all, rather than a partially-reconstructed one
/// that might be wider than what was written.
pub fn read_projection(node: &Container) -> Option<Grant> {
    if !node.tags.iter().any(|t| t == PROJECTION_TAG) {
        return None;
    }
    read_grant(node)
}

/// Why the gate refused a petition. Nothing was applied.
#[derive(Clone, Debug, PartialEq)]
pub enum GateError {
    /// A declared behavior deadband refused this actuation before commit.
    Deadband(DeadbandRefusal),
    /// The subject holds no capability covering the claimed path at write mode.
    Unauthorized {
        /// The scope the petition claimed.
        path: String,
    },
    /// A spec targets a node outside the claimed scope, or one whose id is not
    /// a well-formed scope at all (which fails closed).
    OutOfScope {
        /// The offending node id.
        node: String,
        /// The scope the petition claimed.
        path: String,
    },
    /// A facet write is inside the node scope but the subject holds no
    /// capability covering that facet namespace.
    UnauthorizedFacet {
        /// The facet id the petition tried to set or remove.
        facet: String,
    },
    /// A spec would touch a grant projection (the reserved namespace).
    TouchesProjection {
        /// The offending node id.
        node: String,
    },
    /// The underlying attributed commit refused (revision conflict, unknown
    /// node, and so on). Carries chartulary's error, including the current
    /// revision on a conflict so the participant can rebase.
    Commit(CommitError<String>),
}

/// The node ids a spec touches (for scope and projection checks). Exact-ID
/// edge operations are checked against the retained assertion record so they
/// cannot bypass endpoint policy merely by naming an edge id.
fn touched_nodes<'a>(
    nested: &'a GraphLog<Container, Relation>,
    spec: &'a EditSpec<Container, Relation>,
) -> Vec<&'a str> {
    match spec {
        EditSpec::InsertNode(node) => vec![node.id.as_str()],
        EditSpec::RemoveNode(id) => vec![id.as_str()],
        EditSpec::Connect { from, to, .. } | EditSpec::Assert { from, to, .. } => {
            vec![from.as_str(), to.as_str()]
        },
        EditSpec::Disconnect(id)
        | EditSpec::ConfirmAssertion { id, .. }
        | EditSpec::WithdrawAssertion { id, .. } => nested
            .assertion(*id)
            .map(|record| vec![record.from.as_str(), record.to.as_str()])
            .unwrap_or_default(),
        EditSpec::Derive { node, .. } => vec![node.as_str()],
        EditSpec::SetFacet { node, .. } | EditSpec::RemoveFacet { node, .. } => {
            vec![node.as_str()]
        },
    }
}

/// The facet id a spec writes, if any. Facet authority is an independent axis
/// from the node scope returned by [`touched_nodes`].
fn touched_facet(spec: &EditSpec<Container, Relation>) -> Option<&str> {
    match spec {
        EditSpec::SetFacet { facet, .. } | EditSpec::RemoveFacet { facet, .. } => {
            Some(facet.as_str())
        },
        _ => None,
    }
}

/// The behavior-specific half of a petition: its ordinary graph batch plus
/// the scalar actuation sample that a declared deadband evaluates.
pub struct BehaviorPetition<'a> {
    /// The node scope this batch claims.
    pub claimed: &'a ScopePath,
    /// The graph revision the batch expects.
    pub expected: u64,
    /// The behavior output and host-supplied instant.
    pub actuation: Actuation,
    /// The proposed atomic graph edits.
    pub specs: Vec<EditSpec<Container, Relation>>,
}

impl<'a> BehaviorPetition<'a> {
    /// Assemble one behavior petition.
    pub fn new(
        claimed: &'a ScopePath,
        expected: u64,
        actuation: Actuation,
        specs: Vec<EditSpec<Container, Relation>>,
    ) -> Self {
        Self {
            claimed,
            expected,
            actuation,
            specs,
        }
    }
}

/// The authority gate. Holds the author it commits **grant projections** under
/// (distinct from any participant), so projections are attributable to the gate,
/// not to the helper they describe.
#[derive(Clone, Debug)]
pub struct Gate {
    author: Author,
}

impl Default for Gate {
    fn default() -> Self {
        Self::new()
    }
}

impl Gate {
    /// A gate whose projections are authored `gate`.
    pub fn new() -> Self {
        Self {
            author: Author::new("gate"),
        }
    }

    /// A gate whose projections carry a specific author.
    pub fn with_author(author: Author) -> Self {
        Self { author }
    }

    /// The projection node id for a grant over `cap`, in the capability's wire
    /// form so the record round-trips (`grant:power:navigate`).
    pub fn projection_id(cap: &Cap) -> String {
        format!("{GRANT_PREFIX}{}", cap.to_wire())
    }

    fn commit_projection(
        &self,
        nested: &mut GraphLog<Container, Relation>,
        mut node: Container,
    ) -> Result<Committed, GateError> {
        node.media_type = Some(PROJECTION_MEDIA_TYPE.to_string());
        let expected = nested.revision();
        nested
            .commit_batch(
                self.author.clone(),
                expected,
                vec![EditSpec::InsertNode(node)],
            )
            .map_err(GateError::Commit)
    }

    /// Render `grant` into `nested` as a read-only projection node, committed by
    /// the gate's own author. A browsable record of what the participant may do.
    ///
    /// The record is **lossless**: every field rides an explicit `key:value`
    /// tag, so [`read_projection`] reconstructs the grant exactly. (Before the
    /// capability-model round the mode lived only in the display title and
    /// readers hardcoded [`Mode::Write`], so a `Read` grant came back as
    /// `Write` after a restart. A field that does not survive replay is not a
    /// field.)
    pub fn project_grant(
        &self,
        nested: &mut GraphLog<Container, Relation>,
        grant: &Grant,
    ) -> Result<Committed, GateError> {
        let mut node = Container::new(Self::projection_id(&grant.cap)).with_tag(PROJECTION_TAG);
        for tag in grant_tags(grant) {
            node = node.with_tag(tag);
        }
        node = node.with_title(format!(
            "{:?} {} {}",
            grant.mode,
            grant.cap,
            grant.subject.to_hex()
        ));
        self.commit_projection(nested, node)
    }

    /// Run a petition through the gate: projection guard, authority, scope, then
    /// an attributed revision-checked commit. Returns the commit receipt or the
    /// reason nothing applied.
    pub fn petition(
        &self,
        provider: &impl AuthorityProvider,
        nested: &mut GraphLog<Container, Relation>,
        subject: Subject,
        claimed: &ScopePath,
        expected: u64,
        specs: Vec<EditSpec<Container, Relation>>,
    ) -> Result<Committed, GateError> {
        self.validate_petition(provider, nested, subject, claimed, &specs)?;
        nested
            .commit_batch(subject.to_author(), expected, specs)
            .map_err(GateError::Commit)
    }

    /// Run a behavior petition through the ordinary authority checks and its
    /// declared actuation deadband, then commit. The accepted output and time
    /// are recorded only after the revision-checked commit lands.
    pub fn petition_behavior(
        &self,
        provider: &impl AuthorityProvider,
        deadbands: &mut DeadbandTable,
        nested: &mut GraphLog<Container, Relation>,
        subject: Subject,
        petition: BehaviorPetition<'_>,
    ) -> Result<Committed, GateError> {
        self.validate_petition(provider, nested, subject, petition.claimed, &petition.specs)?;
        let admission = deadbands
            .check(subject, petition.actuation)
            .map_err(GateError::Deadband)?;
        let committed = nested
            .commit_batch(subject.to_author(), petition.expected, petition.specs)
            .map_err(GateError::Commit)?;
        deadbands.record(admission);
        Ok(committed)
    }

    fn validate_petition(
        &self,
        provider: &impl AuthorityProvider,
        nested: &GraphLog<Container, Relation>,
        subject: Subject,
        claimed: &ScopePath,
        specs: &[EditSpec<Container, Relation>],
    ) -> Result<(), GateError> {
        // 1. Projection guard: a participant may never touch its own grants.
        for spec in specs {
            for node in touched_nodes(nested, spec) {
                if node.starts_with(GRANT_PREFIX) {
                    return Err(GateError::TouchesProjection {
                        node: node.to_string(),
                    });
                }
            }
        }
        // 2. Authority: the subject must hold a write cap covering the scope.
        let claimed_cap = Cap::Scope(claimed.clone());
        if !provider.covers(subject, &claimed_cap, Mode::Write) {
            return Err(GateError::Unauthorized {
                path: claimed.to_string(),
            });
        }
        // 3. Scope: every touched node must fall under the claimed scope, by
        //    SEGMENT (`trail` does not contain `trailer/x`). A node id that is
        //    not a well-formed scope fails closed rather than being compared as
        //    a raw string.
        for spec in specs {
            for node in touched_nodes(nested, spec) {
                let inside = ScopePath::parse(node)
                    .is_ok_and(|node_scope| claimed.covers_scope(&node_scope));
                if !inside {
                    return Err(GateError::OutOfScope {
                        node: node.to_string(),
                        path: claimed.to_string(),
                    });
                }
            }
        }
        // 4. Facet namespace: touching a permitted node does not imply
        //    permission to write every facet family on it. Parse at the gate
        //    boundary; malformed ids fail closed because no typed capability
        //    can cover them.
        for spec in specs {
            let Some(facet) = touched_facet(spec) else {
                continue;
            };
            let covered = Cap::facet(facet)
                .is_ok_and(|needed| provider.covers(subject, &needed, Mode::Write));
            if !covered {
                return Err(GateError::UnauthorizedFacet {
                    facet: facet.to_string(),
                });
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
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
        let auth = GrantTable::new().with_grant(Grant::new(
            sub,
            Cap::scope("grant:").unwrap(),
            Mode::Write,
        ));
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
            },
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
}
