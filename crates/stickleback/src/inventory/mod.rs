// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Bounded advisory inventory for retained opaque objects.
//!
//! The caller owns payloads, peer/namespace authorization, time, retention and
//! delivery admission. This module performs no I/O and makes no custody promise.
//! A positive probabilistic membership result may hide a useful offer; bounded
//! exact reconciliation is therefore a separate, explicit operation. Object
//! digests identify entire original byte records, not p2panda operation IDs.

mod filter;
mod peers;
#[cfg(test)]
mod tests;
mod wire;

use std::collections::BTreeMap;

pub use peers::{CandidateBudget, Observation, Offer, PeerInventories};
pub use wire::{CuckooSummary, ExactPage, Summary};

pub type ObjectDigest = [u8; 32];
pub type Namespace = [u8; 32];
pub type PeerId = [u8; 32];

pub const MAX_ENTRIES: usize = 64;
pub const MAX_RETAINED_BYTES: usize = 1024 * 1024;
pub const MAX_OBJECT_BYTES: usize = 16 * 1024;
pub const MAX_CONTROL_BYTES: usize = 1024;
pub const MAX_CANDIDATES: usize = 8;
pub const MAX_PEERS: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InventoryError {
    #[error("invalid inventory limits")]
    Limits,
    #[error("retained inventory capacity exceeded")]
    Capacity,
    #[error("invalid object length or expiry")]
    Object,
    #[error("inventory revision exhausted")]
    Revision,
    #[error("invalid or oversized inventory control message")]
    Wire,
    #[error("inventory namespace mismatch")]
    Namespace,
    #[error("stale or inconsistent inventory page")]
    Stale,
    #[error("peer inventory incomplete or absent")]
    Incomplete,
    #[error("caller clock moved backwards")]
    Clock,
}

#[derive(Clone, Copy, Debug)]
pub struct InventoryLimits {
    pub entries: usize,
    pub retained_bytes: usize,
    pub object_bytes: usize,
}

impl Default for InventoryLimits {
    fn default() -> Self {
        Self {
            entries: MAX_ENTRIES,
            retained_bytes: MAX_RETAINED_BYTES,
            object_bytes: MAX_OBJECT_BYTES,
        }
    }
}

impl InventoryLimits {
    fn validate(self) -> Result<Self, InventoryError> {
        if self.entries == 0
            || self.entries > MAX_ENTRIES
            || self.retained_bytes == 0
            || self.retained_bytes > MAX_RETAINED_BYTES
            || self.object_bytes == 0
            || self.object_bytes > MAX_OBJECT_BYTES
            || self.object_bytes > self.retained_bytes
        {
            return Err(InventoryError::Limits);
        }
        Ok(self)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RetainedEntry {
    pub digest: ObjectDigest,
    pub bytes: usize,
    /// Caller-owned monotonic time; expired entries are never offered.
    pub expires_at: u64,
}

#[derive(Clone, Debug)]
pub struct Inventory {
    namespace: Namespace,
    limits: InventoryLimits,
    entries: BTreeMap<ObjectDigest, RetainedEntry>,
    retained_bytes: usize,
    revision: u64,
}

impl Inventory {
    pub fn new(namespace: Namespace, limits: InventoryLimits) -> Result<Self, InventoryError> {
        Ok(Self {
            namespace,
            limits: limits.validate()?,
            entries: BTreeMap::new(),
            retained_bytes: 0,
            revision: 0,
        })
    }

    /// Announce an owner's retained object. Failure leaves the inventory intact.
    /// The owner selects evictions and calls `remove`; this leaf never owns bytes.
    pub fn retain(&mut self, entry: RetainedEntry, now: u64) -> Result<(), InventoryError> {
        if entry.bytes == 0 || entry.bytes > self.limits.object_bytes || entry.expires_at <= now {
            return Err(InventoryError::Object);
        }
        let old = self.entries.get(&entry.digest);
        if old == Some(&entry) {
            return Ok(());
        }
        let bytes = self.retained_bytes - old.map_or(0, |e| e.bytes) + entry.bytes;
        if bytes > self.limits.retained_bytes
            || (old.is_none() && self.entries.len() == self.limits.entries)
        {
            return Err(InventoryError::Capacity);
        }
        let revision = self
            .revision
            .checked_add(1)
            .ok_or(InventoryError::Revision)?;
        self.entries.insert(entry.digest, entry);
        self.retained_bytes = bytes;
        self.revision = revision;
        Ok(())
    }

    pub fn remove(&mut self, id: &ObjectDigest) -> Result<Option<RetainedEntry>, InventoryError> {
        if let Some(entry) = self.entries.get(id).copied() {
            let revision = self
                .revision
                .checked_add(1)
                .ok_or(InventoryError::Revision)?;
            self.entries.remove(id);
            self.retained_bytes -= entry.bytes;
            self.revision = revision;
            Ok(Some(entry))
        } else {
            Ok(None)
        }
    }

    /// Forget expired metadata and return the IDs for the owner's own cleanup.
    pub fn expire(&mut self, now: u64) -> Result<Vec<ObjectDigest>, InventoryError> {
        let ids: Vec<_> = self
            .entries
            .values()
            .filter(|e| e.expires_at <= now)
            .map(|e| e.digest)
            .collect();
        // Reserve the revision before mutation, including the multi-entry case.
        if !ids.is_empty() {
            self.revision = self
                .revision
                .checked_add(1)
                .ok_or(InventoryError::Revision)?;
            for id in &ids {
                let entry = self.entries.remove(id).expect("selected live metadata");
                self.retained_bytes -= entry.bytes;
            }
        }
        Ok(ids)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    pub fn retained_bytes(&self) -> usize {
        self.retained_bytes
    }
    pub fn entries(&self) -> impl Iterator<Item = RetainedEntry> + '_ {
        self.entries.values().copied()
    }

    /// A bounded immutable view. Pagination remains consistent even if the live
    /// owner adds/removes entries while these pages are exchanged.
    pub fn snapshot(&self, now: u64) -> Snapshot {
        let ids = self
            .entries
            .values()
            .filter(|e| e.expires_at > now)
            .map(|e| e.digest)
            .collect::<Vec<_>>();
        let digest = snapshot_digest(&self.namespace, self.revision, &ids);
        Snapshot {
            namespace: self.namespace,
            revision: self.revision,
            digest,
            ids,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Snapshot {
    namespace: Namespace,
    revision: u64,
    digest: ObjectDigest,
    ids: Vec<ObjectDigest>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SummaryMode {
    Exact,
    Cuckoo,
}

#[derive(Clone, Debug)]
pub struct SummaryBuild {
    pub summary: Summary,
    /// True if deterministic filter construction saturated and selected exact.
    pub saturated: bool,
}

impl Snapshot {
    pub fn len(&self) -> usize {
        self.ids.len()
    }
    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    pub fn exact_page(&self, start: usize) -> Result<ExactPage, InventoryError> {
        if start > self.ids.len() || (start == self.ids.len() && !self.ids.is_empty()) {
            return Err(InventoryError::Stale);
        }
        let end = (start + MAX_CANDIDATES).min(self.ids.len());
        Ok(ExactPage {
            namespace: self.namespace,
            revision: self.revision,
            snapshot: self.digest,
            total: self.ids.len(),
            start,
            ids: self.ids[start..end].to_vec(),
        })
    }

    pub fn summary(&self, mode: SummaryMode) -> SummaryBuild {
        if mode == SummaryMode::Cuckoo
            && let Some(filter) = filter::CuckooFilter::build(&self.namespace, &self.ids)
        {
            return SummaryBuild {
                summary: Summary::Cuckoo(Box::new(CuckooSummary {
                    namespace: self.namespace,
                    revision: self.revision,
                    snapshot: self.digest,
                    total: self.ids.len(),
                    filter,
                })),
                saturated: false,
            };
        }
        SummaryBuild {
            summary: Summary::Exact(self.exact_page(0).expect("bounded snapshot")),
            saturated: mode == SummaryMode::Cuckoo,
        }
    }
}

fn snapshot_digest(namespace: &Namespace, revision: u64, ids: &[ObjectDigest]) -> ObjectDigest {
    let mut h = blake3::Hasher::new_derive_key("mere.inventory.snapshot.v1");
    h.update(namespace);
    h.update(&revision.to_le_bytes());
    h.update(&(ids.len() as u16).to_le_bytes());
    for id in ids {
        h.update(id);
    }
    *h.finalize().as_bytes()
}
