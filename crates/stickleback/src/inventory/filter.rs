// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::{Namespace, ObjectDigest};

pub(super) const BUCKETS: usize = 32;
pub(super) const SLOTS: usize = 4;
pub(super) const CELLS: usize = BUCKETS * SLOTS;

/// Version one: 32 four-slot buckets, nonzero little-endian 16-bit fingerprints,
/// stable domain-separated BLAKE3 indices, and at most 128 deterministic kicks.
/// Rebuilt from an immutable snapshot, so eviction cannot leave stale slots.
#[derive(Clone, Debug)]
pub(super) struct CuckooFilter {
    pub(super) cells: [u16; CELLS],
}

impl CuckooFilter {
    pub(super) fn build(namespace: &Namespace, ids: &[ObjectDigest]) -> Option<Self> {
        let mut result = Self { cells: [0; CELLS] };
        for id in ids {
            if !result.insert(namespace, id) {
                return None;
            }
        }
        Some(result)
    }

    pub(super) fn contains(&self, namespace: &Namespace, id: &ObjectDigest) -> bool {
        let (fp, a) = fingerprint(namespace, id);
        let b = alternate(a, fp);
        self.bucket(a).contains(&fp) || self.bucket(b).contains(&fp)
    }

    fn bucket(&self, index: usize) -> &[u16] {
        &self.cells[index * SLOTS..(index + 1) * SLOTS]
    }

    fn place(&mut self, index: usize, fp: u16) -> bool {
        let slots = &mut self.cells[index * SLOTS..(index + 1) * SLOTS];
        if let Some(slot) = slots.iter_mut().find(|s| **s == 0) {
            *slot = fp;
            true
        } else {
            false
        }
    }

    fn insert(&mut self, namespace: &Namespace, id: &ObjectDigest) -> bool {
        if self.contains(namespace, id) {
            return true;
        }
        let (mut fp, mut a) = fingerprint(namespace, id);
        if self.place(a, fp) || self.place(alternate(a, fp), fp) {
            return true;
        }
        for kick in 0..128 {
            let slot = a * SLOTS + (usize::from(fp) + kick) % SLOTS;
            std::mem::swap(&mut self.cells[slot], &mut fp);
            a = alternate(a, fp);
            if self.place(a, fp) {
                return true;
            }
        }
        // The mutated tentative filter is discarded in its entirety on failure.
        false
    }
}

pub(super) fn fingerprint(namespace: &Namespace, id: &ObjectDigest) -> (u16, usize) {
    let mut h = blake3::Hasher::new_derive_key("mere.inventory.cuckoo.key.v1");
    h.update(namespace);
    h.update(id);
    let hash = h.finalize();
    let bytes = hash.as_bytes();
    let fp = u16::from_le_bytes([bytes[0], bytes[1]]).max(1);
    let index = usize::from(bytes[2]) & (BUCKETS - 1);
    (fp, index)
}

fn alternate(index: usize, fp: u16) -> usize {
    let hash = blake3::derive_key("mere.inventory.cuckoo.alternate.v1", &fp.to_le_bytes());
    // Odd XOR offsets are nonzero and preserve the involution i2(i2(i)) = i.
    index ^ ((usize::from(hash[0]) & (BUCKETS - 1)) | 1)
}
