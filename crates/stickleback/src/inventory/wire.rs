// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::filter::{CELLS, CuckooFilter};
use super::{
    InventoryError, MAX_CANDIDATES, MAX_CONTROL_BYTES, MAX_ENTRIES, Namespace, ObjectDigest,
};

/// A page of one immutable exact snapshot; all pages share its digest/revision.
#[derive(Clone, Debug)]
pub struct ExactPage {
    pub(super) namespace: Namespace,
    pub(super) revision: u64,
    pub(super) snapshot: ObjectDigest,
    pub(super) total: usize,
    pub(super) start: usize,
    pub(super) ids: Vec<ObjectDigest>,
}

impl ExactPage {
    pub fn start(&self) -> usize {
        self.start
    }
    pub fn next_cursor(&self) -> Option<usize> {
        let next = self.start + self.ids.len();
        (next < self.total).then_some(next)
    }
}

#[derive(Clone, Debug)]
pub struct CuckooSummary {
    pub(super) namespace: Namespace,
    pub(super) revision: u64,
    pub(super) snapshot: ObjectDigest,
    pub(super) total: usize,
    pub(super) filter: CuckooFilter,
}

/// Authored v1 profile, not MemoryLAN wire compatibility. Fixed-size identifiers,
/// explicit little-endian numbers, a 1 KiB receive ceiling and exact length checks.
#[derive(Clone, Debug)]
pub enum Summary {
    Exact(ExactPage),
    Cuckoo(Box<CuckooSummary>),
}

impl Summary {
    pub fn namespace(&self) -> Namespace {
        match self {
            Self::Exact(p) => p.namespace,
            Self::Cuckoo(p) => p.namespace,
        }
    }

    pub fn encode(&self) -> Vec<u8> {
        let (kind, namespace, revision, snapshot, total) = match self {
            Self::Exact(p) => (0, p.namespace, p.revision, p.snapshot, p.total),
            Self::Cuckoo(p) => (1, p.namespace, p.revision, p.snapshot, p.total),
        };
        let mut bytes = Vec::with_capacity(512);
        bytes.extend_from_slice(b"MIV1");
        bytes.push(kind);
        bytes.extend_from_slice(&namespace);
        bytes.extend_from_slice(&revision.to_le_bytes());
        bytes.extend_from_slice(&snapshot);
        bytes.extend_from_slice(&(total as u16).to_le_bytes());
        match self {
            Self::Exact(p) => {
                bytes.extend_from_slice(&(p.start as u16).to_le_bytes());
                bytes.push(p.ids.len() as u8);
                for id in &p.ids {
                    bytes.extend_from_slice(id);
                }
            },
            Self::Cuckoo(p) => {
                for fp in p.filter.cells {
                    bytes.extend_from_slice(&fp.to_le_bytes());
                }
            },
        }
        bytes
    }

    /// Structural validation only. Authenticate the peer and authorize inventory
    /// access to this namespace before allocating/storing its decoded summary.
    pub fn decode(bytes: &[u8]) -> Result<Self, InventoryError> {
        if bytes.len() > MAX_CONTROL_BYTES || bytes.len() < 79 || &bytes[..4] != b"MIV1" {
            return Err(InventoryError::Wire);
        }
        let namespace = bytes[5..37].try_into().map_err(|_| InventoryError::Wire)?;
        let revision =
            u64::from_le_bytes(bytes[37..45].try_into().map_err(|_| InventoryError::Wire)?);
        let snapshot = bytes[45..77].try_into().map_err(|_| InventoryError::Wire)?;
        let total = usize::from(u16::from_le_bytes([bytes[77], bytes[78]]));
        if total > MAX_ENTRIES {
            return Err(InventoryError::Wire);
        }
        match bytes[4] {
            0 => {
                if bytes.len() < 82 {
                    return Err(InventoryError::Wire);
                }
                let start = usize::from(u16::from_le_bytes([bytes[79], bytes[80]]));
                let count = usize::from(bytes[81]);
                if count > MAX_CANDIDATES
                    || start + count > total
                    || (count == 0 && total != 0)
                    || bytes.len() != 82 + count * 32
                {
                    return Err(InventoryError::Wire);
                }
                let ids = bytes[82..].as_chunks::<32>().0.to_vec();
                if ids.windows(2).any(|w| w[0] >= w[1]) {
                    return Err(InventoryError::Wire);
                }
                Ok(Self::Exact(ExactPage {
                    namespace,
                    revision,
                    snapshot,
                    total,
                    start,
                    ids,
                }))
            },
            1 => {
                if bytes.len() != 79 + CELLS * 2 {
                    return Err(InventoryError::Wire);
                }
                let mut cells = [0; CELLS];
                for (slot, fp) in cells.iter_mut().zip(bytes[79..].as_chunks::<2>().0) {
                    *slot = u16::from_le_bytes([fp[0], fp[1]]);
                }
                if cells.iter().filter(|s| **s != 0).count() > total {
                    return Err(InventoryError::Wire);
                }
                Ok(Self::Cuckoo(Box::new(CuckooSummary {
                    namespace,
                    revision,
                    snapshot,
                    total,
                    filter: CuckooFilter { cells },
                })))
            },
            _ => Err(InventoryError::Wire),
        }
    }
}
