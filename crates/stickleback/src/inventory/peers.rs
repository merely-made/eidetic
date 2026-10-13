// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use std::collections::BTreeMap;

use super::{
    Inventory, InventoryError, MAX_CANDIDATES, MAX_OBJECT_BYTES, MAX_PEERS, Namespace,
    ObjectDigest, PeerId, Summary, snapshot_digest,
};

#[derive(Clone, Copy, Debug)]
pub struct CandidateBudget {
    candidates: usize,
    payload_bytes: usize,
}

impl CandidateBudget {
    pub fn new(candidates: usize, payload_bytes: usize) -> Result<Self, InventoryError> {
        if candidates == 0
            || candidates > MAX_CANDIDATES
            || payload_bytes == 0
            || payload_bytes > MAX_OBJECT_BYTES * MAX_CANDIDATES
        {
            return Err(InventoryError::Limits);
        }
        Ok(Self {
            candidates,
            payload_bytes,
        })
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Offer {
    pub digest: ObjectDigest,
    pub bytes: usize,
    pub expires_at: u64,
}

#[derive(Clone, Copy, Debug)]
pub struct Observation {
    pub complete: bool,
    pub next_cursor: Option<usize>,
}

#[derive(Clone, Debug)]
struct ExactAssembly {
    namespace: Namespace,
    revision: u64,
    snapshot: ObjectDigest,
    total: usize,
    ids: Vec<ObjectDigest>,
}

#[derive(Clone, Debug)]
enum Remote {
    Filter(Box<super::CuckooSummary>),
    Exact(ExactAssembly),
}

#[derive(Clone, Debug)]
struct Peer {
    last_seen: u64,
    remote: Remote,
    cursor: Option<ObjectDigest>,
}

/// Bounded peer metadata. IDs must already be authenticated and separately
/// authorized by the caller for inventory access. The caller supplies monotonic
/// time and may forget a peer on session/incarnation change. No discovery or
/// membership authority is inferred here.
pub struct PeerInventories {
    namespace: Namespace,
    max_peers: usize,
    idle_ticks: u64,
    clock: u64,
    peers: BTreeMap<PeerId, Peer>,
}

impl PeerInventories {
    pub fn new(
        namespace: Namespace,
        max_peers: usize,
        idle_ticks: u64,
    ) -> Result<Self, InventoryError> {
        if max_peers == 0 || max_peers > MAX_PEERS || idle_ticks == 0 {
            return Err(InventoryError::Limits);
        }
        Ok(Self {
            namespace,
            max_peers,
            idle_ticks,
            clock: 0,
            peers: BTreeMap::new(),
        })
    }

    pub fn len(&self) -> usize {
        self.peers.len()
    }
    pub fn is_empty(&self) -> bool {
        self.peers.is_empty()
    }
    pub fn forget(&mut self, peer: &PeerId) {
        self.peers.remove(peer);
    }

    pub fn expire(&mut self, now: u64) -> Result<(), InventoryError> {
        if now < self.clock {
            return Err(InventoryError::Clock);
        }
        self.clock = now;
        self.peers
            .retain(|_, p| now - p.last_seen < self.idle_ticks);
        Ok(())
    }

    /// Accept one bounded decoded message. A failed page leaves prior peer state
    /// intact. Duplicated pages are idempotent; mixed snapshots cannot complete.
    pub fn observe(
        &mut self,
        peer: PeerId,
        summary: &Summary,
        now: u64,
    ) -> Result<Observation, InventoryError> {
        if summary.namespace() != self.namespace {
            return Err(InventoryError::Namespace);
        }
        self.expire(now)?;
        if !self.peers.contains_key(&peer) && self.peers.len() == self.max_peers {
            return Err(InventoryError::Capacity);
        }
        let (remote, observation) = match summary {
            Summary::Cuckoo(s) => (
                Remote::Filter(s.clone()),
                Observation {
                    complete: true,
                    next_cursor: None,
                },
            ),
            Summary::Exact(p) => {
                let existing = self.peers.get(&peer).and_then(|peer| match &peer.remote {
                    Remote::Exact(e)
                        if e.snapshot == p.snapshot
                            && e.revision == p.revision
                            && e.total == p.total =>
                    {
                        Some(e.clone())
                    },
                    _ => None,
                });
                let mut exact = match existing {
                    Some(e) => e,
                    None if p.start == 0 => ExactAssembly {
                        namespace: p.namespace,
                        revision: p.revision,
                        snapshot: p.snapshot,
                        total: p.total,
                        ids: Vec::new(),
                    },
                    None => return Err(InventoryError::Stale),
                };
                if p.start == exact.ids.len() {
                    if exact
                        .ids
                        .last()
                        .zip(p.ids.first())
                        .is_some_and(|(a, b)| a >= b)
                    {
                        return Err(InventoryError::Stale);
                    }
                    exact.ids.extend_from_slice(&p.ids);
                } else if p.start > exact.ids.len()
                    || exact.ids.get(p.start..p.start + p.ids.len()) != Some(p.ids.as_slice())
                {
                    return Err(InventoryError::Stale);
                }
                let complete = exact.ids.len() == exact.total;
                if complete
                    && snapshot_digest(&exact.namespace, exact.revision, &exact.ids)
                        != exact.snapshot
                {
                    return Err(InventoryError::Stale);
                }
                let next_cursor = (!complete).then_some(exact.ids.len());
                (
                    Remote::Exact(exact),
                    Observation {
                        complete,
                        next_cursor,
                    },
                )
            },
        };
        let cursor = self.peers.get(&peer).and_then(|p| p.cursor);
        self.peers.insert(
            peer,
            Peer {
                last_seen: now,
                remote,
                cursor,
            },
        );
        Ok(observation)
    }

    /// Advisory offers only. The caller rechecks retained availability/expiry,
    /// reserves ingress/session capacity, retrieves the exact bytes, and verifies
    /// their digest at completion before invoking its existing admission owner.
    pub fn offers(
        &mut self,
        inventory: &Inventory,
        peer: &PeerId,
        now: u64,
        budget: CandidateBudget,
    ) -> Result<Vec<Offer>, InventoryError> {
        if inventory.namespace != self.namespace {
            return Err(InventoryError::Namespace);
        }
        self.expire(now)?;
        let state = self.peers.get_mut(peer).ok_or(InventoryError::Incomplete)?;
        let remote = &state.remote;
        if matches!(remote, Remote::Exact(e) if e.ids.len() != e.total) {
            return Err(InventoryError::Incomplete);
        }
        let mut offers = Vec::new();
        let mut remaining = budget.payload_bytes;
        let entries = inventory
            .entries()
            .filter(|e| e.expires_at > now)
            .collect::<Vec<_>>();
        let start = state
            .cursor
            .map_or(0, |cursor| entries.partition_point(|e| e.digest <= cursor));
        for entry in entries[start..].iter().chain(entries[..start].iter()) {
            let present = match remote {
                Remote::Filter(s) => s.filter.contains(&s.namespace, &entry.digest),
                Remote::Exact(e) => e.ids.binary_search(&entry.digest).is_ok(),
            };
            if !present && entry.bytes <= remaining {
                offers.push(Offer {
                    digest: entry.digest,
                    bytes: entry.bytes,
                    expires_at: entry.expires_at,
                });
                remaining -= entry.bytes;
                if offers.len() == budget.candidates {
                    break;
                }
            }
        }
        if let Some(offer) = offers.last() {
            state.cursor = Some(offer.digest);
        }
        Ok(offers)
    }
}
