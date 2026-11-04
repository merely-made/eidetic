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
        }
        EditSpec::Disconnect(id)
        | EditSpec::ConfirmAssertion { id, .. }
        | EditSpec::WithdrawAssertion { id, .. } => nested
            .assertion(*id)
            .map(|record| vec![record.from.as_str(), record.to.as_str()])
            .unwrap_or_default(),
        EditSpec::Derive { node, .. } => vec![node.as_str()],
        EditSpec::SetFacet { node, .. } | EditSpec::RemoveFacet { node, .. } => {
            vec![node.as_str()]
        }
    }
}

/// The facet id a spec writes, if any. Facet authority is an independent axis
/// from the node scope returned by [`touched_nodes`].
fn touched_facet(spec: &EditSpec<Container, Relation>) -> Option<&str> {
    match spec {
        EditSpec::SetFacet { facet, .. } | EditSpec::RemoveFacet { facet, .. } => {
            Some(facet.as_str())
        }
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
mod tests;
