// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Source-scoped browser encounters. Stemma owns admitted occurrences; source
//! children awaiting a parent remain intake, never guessed chronological edges.

use chartulary::stemma::{EntryPrivacy, Stemma, StemmaSnapshot, TransitionKind};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrowserSourceScope {
    pub product: String,
    pub profile: String,
    pub generation: String,
}

impl BrowserSourceScope {
    pub fn is_valid(&self) -> bool {
        [&self.product, &self.profile, &self.generation]
            .iter()
            .all(|value| !value.trim().is_empty() && value.len() <= 256)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum BrowserParent {
    #[default]
    Unknown,
    Root,
    Visit {
        visit_id: String,
    },
    OutsideRange {
        visit_id: String,
    },
    Withheld,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BrowserEncounterRef {
    pub scope: BrowserSourceScope,
    pub visit_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrowserEncounter {
    pub identity: BrowserEncounterRef,
    pub parent: BrowserParent,
    pub url: String,
    pub title: Option<String>,
    pub raw_transition: String,
    pub at_ms: u64,
    pub original_unix_micros: Option<i64>,
    pub source_is_local: Option<bool>,
    /// Host-selected intake attribution, not the unknown original browser user.
    pub persona: String,
    pub acquisition_device: String,
    #[serde(default)]
    pub intake_at_ms: u64,
}

impl BrowserEncounter {
    fn same_source_facts(&self, other: &Self) -> bool {
        let parent_key = |parent: &BrowserParent| match parent {
            BrowserParent::Visit { visit_id } | BrowserParent::OutsideRange { visit_id } => {
                Some(visit_id.clone())
            },
            _ => None,
        };
        let same_parent = self.parent == other.parent
            || (parent_key(&self.parent).is_some()
                && parent_key(&self.parent) == parent_key(&other.parent));
        self.identity == other.identity
            && same_parent
            && self.url == other.url
            && self.raw_transition == other.raw_transition
            && self.at_ms == other.at_ms
            && self.original_unix_micros == other.original_unix_micros
            && self.source_is_local == other.source_is_local
        // Page-level titles and acquisition attribution may differ on replay.
    }
}

pub type BrowserEncounterSnapshot =
    StemmaSnapshot<String, String, BrowserSourceScope, BrowserEncounter>;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrowserEncounterLedger {
    pub snapshot: BrowserEncounterSnapshot,
    pub pending: Vec<BrowserEncounter>,
}

impl Default for BrowserEncounterLedger {
    fn default() -> Self {
        Self {
            snapshot: Stemma::new().to_snapshot(),
            pending: Vec::new(),
        }
    }
}

impl BrowserEncounterLedger {
    pub fn validate(&self) -> Result<(), &'static str> {
        let entries = self.snapshot.entries.len();
        let visits = self.snapshot.visits.len();
        let owners = self.snapshot.owners.len();
        let mut identities = std::collections::HashSet::new();
        for (index, visit) in self.snapshot.visits.iter().enumerate() {
            if visit.entry >= entries
                || visit.parent.is_some_and(|parent| parent >= index)
                || visit.children.iter().any(|child| {
                    *child >= visits || self.snapshot.visits[*child].parent != Some(index)
                })
                || visit
                    .bindings
                    .iter()
                    .any(|binding| binding.owner >= owners || binding.forward_child.is_some())
                || !identities.insert(visit.context.identity.clone())
            {
                return Err("invalid stored browser encounter tree");
            }
        }
        for owner in &self.snapshot.owners {
            if owner.current.is_some()
                || owner.origin.is_some()
                || owner.creator.is_some()
                || owner.pending_origin_parent.is_some()
                || owner.owned_visits.iter().any(|visit| *visit >= visits)
            {
                return Err("invalid stored browser encounter owner");
            }
        }
        for visit in &self.pending {
            if !identities.insert(visit.identity.clone()) {
                return Err("duplicate stored browser encounter");
            }
        }
        Ok(())
    }
    pub fn encounter(&self, identity: &BrowserEncounterRef) -> Option<&BrowserEncounter> {
        self.snapshot
            .visits
            .iter()
            .map(|visit| &visit.context)
            .chain(self.pending.iter())
            .find(|visit| &visit.identity == identity)
    }

    /// True for a new source observation, false for identical redelivery.
    pub fn receive(&mut self, visit: BrowserEncounter, limit: usize) -> Result<bool, &'static str> {
        if !visit.identity.scope.is_valid()
            || visit.identity.visit_id.trim().is_empty()
            || visit.identity.visit_id.len() > 256
        {
            return Err("invalid browser encounter identity");
        }
        if let Some(existing) = self.encounter(&visit.identity) {
            return if existing.same_source_facts(&visit) {
                Ok(false)
            } else {
                Err("conflicting browser encounter source facts")
            };
        }
        if self.snapshot.visits.len() + self.pending.len() >= limit {
            return Err("browser encounter capacity reached");
        }
        self.pending.push(visit);
        Ok(true)
    }

    /// Admit resolvable source visits with explicit parentage and untouched
    /// owner cursors. Cycles and missing parents stay inspectable in pending.
    pub fn resolve(&mut self) -> Vec<BrowserEncounter> {
        let mut memory = Stemma::from_snapshot(self.snapshot.clone());
        let mut known: std::collections::HashMap<_, _> = memory
            .visits()
            .map(|(id, visit)| (visit.context.identity.clone(), id))
            .collect();
        let mut admitted = Vec::new();
        loop {
            let mut progress = false;
            let mut unresolved = Vec::new();
            for visit in self.pending.drain(..) {
                let parent = match &visit.parent {
                    BrowserParent::Visit { visit_id }
                    | BrowserParent::OutsideRange { visit_id } => {
                        let reference = BrowserEncounterRef {
                            scope: visit.identity.scope.clone(),
                            visit_id: visit_id.clone(),
                        };
                        let found = known.get(&reference).copied();
                        let Some(found) = found else {
                            unresolved.push(visit);
                            continue;
                        };
                        Some(found)
                    },
                    _ => None,
                };
                let owner = memory.ensure_owner(visit.identity.scope.clone(), None);
                let entry = memory.resolve_or_create_entry(
                    visit.url.clone(),
                    visit.url.clone(),
                    visit.at_ms,
                    EntryPrivacy::LocalOnly,
                );
                let transition = browser_transition(&visit.raw_transition);
                let id = memory
                    .admit_visit(owner, entry, parent, visit.clone(), transition, visit.at_ms)
                    .expect("resolved browser owner, entry and parent");
                known.insert(visit.identity.clone(), id);
                admitted.push(visit);
                progress = true;
            }
            self.pending = unresolved;
            if !progress {
                break;
            }
        }
        self.snapshot = memory.to_snapshot();
        admitted
    }

    /// Rebuild after explicit forgetting; descendants keep unresolved source
    /// references instead of silently being reparented to a different visit.
    pub fn forget_url(&mut self, url: &str) {
        let kept: Vec<_> = self
            .snapshot
            .visits
            .iter()
            .map(|visit| visit.context.clone())
            .chain(self.pending.iter().cloned())
            .filter(|visit| visit.url != url)
            .collect();
        *self = Self::default();
        self.pending = kept;
        self.resolve();
    }
}

pub fn browser_transition(raw: &str) -> TransitionKind {
    match raw.trim().to_ascii_lowercase().as_str() {
        "link" | "link_click" => TransitionKind::LinkClick,
        "typed" | "url_typed" | "generated" | "keyword" | "keyword_generated" => {
            TransitionKind::UrlTyped
        },
        "back" => TransitionKind::Back,
        "forward" => TransitionKind::Forward,
        "reload" => TransitionKind::Reload,
        "redirect" => TransitionKind::Redirect,
        "tab_spawn" => TransitionKind::TabSpawn,
        "restore" | "reopen" => TransitionKind::Restore,
        "imported" | "auto_bookmark" => TransitionKind::Imported,
        _ => TransitionKind::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn visit(id: &str, parent: BrowserParent, profile: &str) -> BrowserEncounter {
        BrowserEncounter {
            identity: BrowserEncounterRef {
                scope: BrowserSourceScope {
                    product: "firefox".into(),
                    profile: profile.into(),
                    generation: "history-1".into(),
                },
                visit_id: id.into(),
            },
            parent,
            url: format!("https://example.org/{id}"),
            title: None,
            raw_transition: "link".into(),
            at_ms: 1000,
            original_unix_micros: Some(1_000_123),
            source_is_local: None,
            persona: "persona:a".into(),
            acquisition_device: "device:a".into(),
            intake_at_ms: 2000,
        }
    }

    #[test]
    fn late_parent_branches_replay_and_profile_collisions_survive_serialization() {
        let mut ledger = BrowserEncounterLedger::default();
        let child = visit(
            "2",
            BrowserParent::Visit {
                visit_id: "1".into(),
            },
            "a",
        );
        ledger.receive(child.clone(), 100).unwrap();
        assert!(ledger.resolve().is_empty());
        assert_eq!(ledger.pending.len(), 1);
        ledger
            .receive(visit("1", BrowserParent::Root, "a"), 100)
            .unwrap();
        ledger
            .receive(
                visit(
                    "3",
                    BrowserParent::Visit {
                        visit_id: "1".into(),
                    },
                    "a",
                ),
                100,
            )
            .unwrap();
        ledger
            .receive(visit("1", BrowserParent::Root, "b"), 100)
            .unwrap();
        assert_eq!(ledger.resolve().len(), 4);
        assert!(ledger.pending.is_empty());
        let mut reopened: BrowserEncounterLedger =
            serde_json::from_slice(&serde_json::to_vec(&ledger).unwrap()).unwrap();
        reopened.validate().unwrap();
        assert!(!reopened.receive(child, 100).unwrap());
        assert_eq!(reopened.snapshot.visits.len(), 4);
        assert!(
            reopened
                .snapshot
                .owners
                .iter()
                .all(|owner| owner.current.is_none())
        );
        assert_eq!(
            reopened
                .snapshot
                .visits
                .iter()
                .filter(|visit| visit.parent.is_some())
                .count(),
            2
        );
    }

    #[test]
    fn missing_parent_cycle_and_conflict_are_retained_or_refused() {
        let mut ledger = BrowserEncounterLedger::default();
        let a = visit(
            "a",
            BrowserParent::Visit {
                visit_id: "b".into(),
            },
            "a",
        );
        ledger.receive(a.clone(), 2).unwrap();
        ledger
            .receive(
                visit(
                    "b",
                    BrowserParent::Visit {
                        visit_id: "a".into(),
                    },
                    "a",
                ),
                2,
            )
            .unwrap();
        assert!(ledger.resolve().is_empty());
        let mut conflicting = a;
        conflicting.url = "https://other.example/".into();
        assert!(ledger.receive(conflicting, 2).is_err());
        assert!(
            ledger
                .receive(visit("extra", BrowserParent::Root, "a"), 2)
                .is_err()
        );
        let mut corrupt = BrowserEncounterLedger::default();
        corrupt
            .receive(visit("root", BrowserParent::Root, "a"), 2)
            .unwrap();
        corrupt.resolve();
        corrupt.snapshot.visits[0].parent = Some(0);
        assert!(corrupt.validate().is_err());
    }
}
