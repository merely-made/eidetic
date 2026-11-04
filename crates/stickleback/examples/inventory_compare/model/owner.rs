// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl Owner {
    pub(super) fn new() -> Self {
        Self {
            inventory: Inventory::new(NS, InventoryLimits::default()).unwrap(),
            peers: PeerInventories::new(NS, 8, 32).unwrap(),
            payloads: BTreeMap::new(),
            fifo: VecDeque::new(),
            history: VecDeque::new(),
            offline_until: 0,
        }
    }

    pub(super) fn receive(&mut self, item: &Item, now: u64) -> Result<bool> {
        if now >= item.expiry {
            return Ok(false);
        }
        if *blake3::hash(&item.bytes).as_bytes() != item.id {
            return Err("object digest mismatch".into());
        }
        let fresh = !self.history.contains(&item.id);
        if fresh {
            if self.history.len() == 128 {
                self.history.pop_front();
            }
            self.history.push_back(item.id);
        }
        if !self.payloads.contains_key(&item.id) {
            while self.payloads.len() >= 64
                || self.inventory.retained_bytes() + item.bytes.len() > 1048576
            {
                let old = self.fifo.pop_front().ok_or("owner eviction mismatch")?;
                self.payloads.remove(&old);
                self.inventory.remove(&old)?;
            }
            self.payloads.insert(item.id, item.bytes.clone());
            self.fifo.push_back(item.id);
        }
        self.inventory.retain(
            RetainedEntry {
                digest: item.id,
                bytes: item.bytes.len(),
                expires_at: item.expiry,
            },
            now,
        )?;
        Ok(fresh)
    }

    pub(super) fn expire(&mut self, now: u64) -> Result<()> {
        for id in self.inventory.expire(now)? {
            self.payloads.remove(&id);
            self.fifo.retain(|x| *x != id);
        }
        self.peers.expire(now)?;
        Ok(())
    }

    pub(super) fn check(&self) -> Result<()> {
        if self.payloads.len() != self.inventory.len()
            || self.fifo.len() != self.inventory.len()
            || self.inventory.len() > 64
            || self.peers.len() > 8
            || self.payloads.values().map(Vec::len).sum::<usize>()
                != self.inventory.retained_bytes()
        {
            return Err("owner/inventory resource invariant".into());
        }
        Ok(())
    }
}
