// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Delegation: the one mechanism behind install, expiry, and revocation —
//! **personae's signed certificates, viewed through the typed capability**.
//!
//! The capability-model round's D3 ruling (2026-07-24): the user is not an
//! implicit infinite authority that "writes some grants" — the user is a
//! **root subject**, and everything else is delegation from it. Install is an
//! attenuating delegation; expiry is a delegation with a bound; revocation is
//! severing one, cascading to its subtree.
//!
//! **This module owns none of that machinery.** `personae::delegation` already
//! provides the whole signed grammar — [`DelegationCertificate`] with
//! `attenuates` / `covers`, chain parents, expiry bounds, delegation depth,
//! content-addressed ids, `DelegationRevocation`, and the derived-key identity
//! proof beneath it — and gemot's moot store already consumes it. Servitor
//! contributes exactly one thing personae lacks: the **typed capability**
//! ([`Cap`], with its closed-set powers and hierarchical scopes) as a view over
//! personae's `(path_prefix, actions)` pair. One delegation system, two tiers.
//!
//! ## How a `Cap` becomes a personae scope
//!
//! personae's [`CapabilityScope`] is two-dimensional — *where*
//! (`path_prefix`, matched with a slash boundary) and *what* (`actions`, a set
//! attenuated by subset). A [`Cap`] encodes into the path, and [`Mode`] into
//! the actions:
//!
//! | servitor | personae `path_prefix` | personae `actions` |
//! | --- | --- | --- |
//! | `Cap::Power("navigate")` at `Write` | `power/navigate` | `{read, write}` |
//! | `Cap::Scope("scenario/a")` at `Read` | `scope/scenario/a` | `{read}` |
//! | `Cap::Facet("web")` at `Write` | `facet/web` | `{read, write}` |
//!
//! Both halves of the capability order survive the encoding:
//!
//! - **powers stay closed** — personae's `path_covers` requires a `/`
//!   boundary, so `power/nav` does not cover `power/navigate`, and no power
//!   has anything beneath it;
//! - **scopes stay hierarchical** — `scope/scenario` covers
//!   `scope/scenario/a`, which is what a scope is for;
//! - **modes stay ordered** — a `Write` grant carries `{read, write}`, so
//!   subset attenuation reproduces `Write` covering `Read` without personae
//!   knowing the ordering.

use std::collections::{BTreeSet, HashMap, HashSet};

use insigne::delegation::{
    CapabilityScope, DelegationCertificate, DelegationId, DelegationParent,
    SignedDelegationCertificate,
};

use insigne::CheckFault;

use crate::Subject;
use crate::cap::Cap;
use crate::grant::{AuthorityProvider, Mode};

/// The application family servitor's participant capabilities live under, in
/// personae's `domain` dimension. Keeps participant certificates from ever being
/// confused with a moot's or a mesh's.
/// Compatibility spelling retained after the 2026-09-20 participant rename:
/// changing this signed domain would invalidate existing delegation scopes.
pub const DENIZEN_DOMAIN: &str = "mere.denizen";

const POWER_PATH: &str = "power";
const SCOPE_PATH: &str = "scope";
const FACET_PATH: &str = "facet";

/// Escape one logical capability segment before placing it in personae's
/// slash-separated path. This keeps a slash inside a power or facet segment
/// from manufacturing hierarchy that the typed capability does not have.
fn encode_segment(segment: &str) -> String {
    segment.replace('%', "%25").replace('/', "%2F")
}

/// The `path_prefix` a capability encodes to.
///
/// A power becomes `power/<name>`; because personae matches paths on a slash
/// boundary, a power covers only itself (nothing sits beneath it), which is
/// the closed-set semantics [`Cap::Power`] exists for. A scope becomes
/// `scope/<path>`, keeping its hierarchy.
pub fn cap_path(cap: &Cap) -> String {
    match cap {
        Cap::Power(name) => format!("{POWER_PATH}/{}", encode_segment(name)),
        Cap::Scope(path) => {
            let path = path.to_string();
            if path.is_empty() {
                // The root scope: the `scope` segment itself, which covers
                // every `scope/...` beneath it.
                SCOPE_PATH.to_string()
            } else {
                format!("{SCOPE_PATH}/{path}")
            }
        }
        Cap::Facet(namespace) => {
            if namespace.segments().is_empty() {
                FACET_PATH.to_string()
            } else {
                let path = namespace
                    .segments()
                    .iter()
                    .map(|segment| encode_segment(segment))
                    .collect::<Vec<_>>()
                    .join("/");
                format!("{FACET_PATH}/{path}")
            }
        }
    }
}

/// The action one mode is checked as.
pub fn mode_action(mode: Mode) -> &'static str {
    match mode {
        Mode::Read => "read",
        Mode::Write => "write",
        Mode::Delegate => "delegate",
    }
}

/// The action SET a mode confers: every mode it implies, so personae's subset
/// attenuation reproduces servitor's `Read < Write < Delegate` ordering
/// without personae having to know it.
pub fn mode_actions(mode: Mode) -> BTreeSet<String> {
    let mut actions = BTreeSet::new();
    actions.insert("read".to_string());
    if mode >= Mode::Write {
        actions.insert("write".to_string());
    }
    if mode >= Mode::Delegate {
        actions.insert("delegate".to_string());
    }
    actions
}

/// Build the personae scope for `cap` at `mode` over `resource` (the opaque
/// id of the governed space — a session graph, a participant's world).
pub fn scope_for(cap: &Cap, mode: Mode, resource: Vec<u8>) -> CapabilityScope {
    CapabilityScope {
        domain: DENIZEN_DOMAIN.to_string(),
        resource,
        path_prefix: cap_path(cap),
        actions: mode_actions(mode),
    }
}

/// Why a chain did not verify. Reported rather than silently denied, so a
/// broken delegation is attributable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChainError {
    /// The certificate did not check; the fault says which step failed.
    BadSignature(DelegationId, CheckFault),
    /// A parent certificate the chain names is not held.
    MissingParent(DelegationId),
    /// A link does not attenuate its parent (widening, depth, or expiry).
    NotAttenuating(DelegationId),
    /// A root link names an authority other than this table's root, or was
    /// issued by a key other than the root it names.
    WrongRoot(DelegationId),
    /// The certificate, or one of its ancestors, is revoked.
    Revoked(DelegationId),
    /// The chain loops.
    Cycle(DelegationId),
}

/// A set of signed delegation certificates rooted at one authority, answering
/// coverage by verified chain validity. Implements [`AuthorityProvider`], so it
/// drops into the gate exactly where [`GrantTable`](crate::GrantTable) does.
///
/// Validity is evaluated on read, so a revocation cascades to a whole subtree
/// with no marking pass (D4's lazy cascade): a descendant whose ancestor is
/// revoked or missing simply stops verifying.
#[derive(Clone, Debug, Default)]
pub struct DelegationTable {
    /// The root authority id: a root certificate is valid only if it names
    /// this. In turnstone this is the user's personae master public key.
    root: [u8; 32],
    /// The host-set clock (servitor reads no clock of its own).
    now_ms: u64,
    certificates: HashMap<DelegationId, SignedDelegationCertificate>,
    revoked: HashSet<DelegationId>,
}

impl DelegationTable {
    /// A table whose root certificates must name `root` — turnstone passes the
    /// active personae identity's master public key (OQ2).
    pub fn new(root: [u8; 32]) -> Self {
        Self {
            root,
            now_ms: 0,
            certificates: HashMap::new(),
            revoked: HashSet::new(),
        }
    }

    /// The root authority this table trusts.
    pub fn root(&self) -> [u8; 32] {
        self.root
    }

    /// Set the clock expiry is judged against. personae's `covers` takes the
    /// evaluation time explicitly, so this is simply what servitor passes it.
    pub fn set_now(&mut self, now_ms: u64) {
        self.now_ms = now_ms;
    }

    /// The clock in force.
    pub fn now(&self) -> u64 {
        self.now_ms
    }

    /// Adopt a certificate (a fresh issue, or one replayed from a cold store).
    /// Validity is re-derived on read, never trusted from storage.
    pub fn adopt(&mut self, signed: SignedDelegationCertificate) {
        self.certificates.insert(signed.certificate.id(), signed);
    }

    /// The certificates held.
    pub fn certificates(&self) -> impl Iterator<Item = &SignedDelegationCertificate> {
        self.certificates.values()
    }

    /// Look one up by id.
    pub fn get(&self, id: &DelegationId) -> Option<&SignedDelegationCertificate> {
        self.certificates.get(id)
    }

    /// Record a revocation. The certificate stays in the table (the record of
    /// what once was), but neither it nor anything beneath it verifies again.
    pub fn revoke(&mut self, id: DelegationId) -> bool {
        self.revoked.insert(id)
    }

    /// Whether `id` is revoked directly (not counting an ancestor).
    pub fn is_revoked(&self, id: &DelegationId) -> bool {
        self.revoked.contains(id)
    }

    /// Revoke every certificate the ROOT issued directly to `subject`:
    /// uninstall, and the "replace" half of replace-and-cascade (OQ1).
    /// Everything `subject` delegated onward cascades. Returns how many root
    /// certificates were revoked.
    pub fn revoke_root_grants(&mut self, subject: Subject) -> usize {
        let ids: Vec<DelegationId> = self
            .certificates
            .values()
            .filter(|signed| {
                signed.certificate.subject == subject.0
                    && matches!(signed.certificate.parent, DelegationParent::Root(_))
            })
            .map(|signed| signed.certificate.id())
            .collect();
        let mut revoked = 0;
        for id in ids {
            if self.revoke(id) {
                revoked += 1;
            }
        }
        revoked
    }

    /// Verify one certificate's whole chain: its own signature and identity
    /// proof, that each link attenuates its parent, that nothing on the path
    /// is revoked, and that it terminates at this table's root.
    pub fn verify_chain(&self, signed: &SignedDelegationCertificate) -> Result<(), ChainError> {
        let mut seen = HashSet::new();
        self.verify_inner(signed, &mut seen)
    }

    fn verify_inner(
        &self,
        signed: &SignedDelegationCertificate,
        seen: &mut HashSet<DelegationId>,
    ) -> Result<(), ChainError> {
        let id = signed.certificate.id();
        if !seen.insert(id) {
            return Err(ChainError::Cycle(id));
        }
        if self.revoked.contains(&id) {
            return Err(ChainError::Revoked(id));
        }
        // insigne owns the cryptography: signature, derived-key attestation,
        // and issuer binding all check there, not in servitor.
        signed
            .check()
            .map_err(|fault| ChainError::BadSignature(id, fault))?;
        match signed.certificate.parent {
            DelegationParent::Root(root) => {
                // `check` binds the signer to `issuer`; this binds `issuer`
                // to the root, so only the root can issue a root link.
                if root == self.root && signed.certificate.issuer == root {
                    Ok(())
                } else {
                    Err(ChainError::WrongRoot(id))
                }
            }
            DelegationParent::Certificate(parent_id) => {
                let parent = self
                    .certificates
                    .get(&parent_id)
                    .ok_or(ChainError::MissingParent(parent_id))?;
                // personae owns attenuation too: scope narrowing, action
                // subset, expiry containment, and delegation depth.
                if !signed.certificate.attenuates(&parent.certificate) {
                    return Err(ChainError::NotAttenuating(id));
                }
                self.verify_inner(parent, seen)
            }
        }
    }

    /// Whether `subject` holds `needed` at `mode` right now, by a verified,
    /// unrevoked chain to the root.
    fn covers_inner(&self, subject: Subject, needed: &Cap, mode: Mode) -> bool {
        let path = cap_path(needed);
        let action = mode_action(mode);
        self.certificates.values().any(|signed| {
            signed.certificate.subject == subject.0
                && signed.certificate.covers(&path, action, self.now_ms)
                && self.verify_chain(signed).is_ok()
        })
    }
}

impl AuthorityProvider for DelegationTable {
    fn covers(&self, subject: Subject, needed: &Cap, mode: Mode) -> bool {
        self.covers_inner(subject, needed, mode)
    }
}

/// Build a root delegation certificate granting `cap` at `mode` over
/// `resource` from the user's `issuer` key to `subject`. Sign it with the
/// `SignedDelegationCertificate::issue` extension from
/// [`identity::delegation::Issue`].
///
/// `depth` is how many further delegation steps the subject may take — `0`
/// forbids sub-delegation entirely, which is the right default for an
/// installed helper.
#[allow(clippy::too_many_arguments)]
pub fn root_certificate(
    issuer: [u8; 32],
    subject: Subject,
    cap: &Cap,
    mode: Mode,
    resource: Vec<u8>,
    issued_at_ms: u64,
    expires_at_ms: Option<u64>,
    depth: u16,
    nonce: [u8; 32],
) -> DelegationCertificate {
    DelegationCertificate::new(
        DelegationParent::Root(issuer),
        issuer,
        subject.0,
        scope_for(cap, mode, resource),
        issued_at_ms,
        issued_at_ms,
        expires_at_ms,
        depth,
        nonce,
    )
}

#[cfg(test)]
mod tests;
