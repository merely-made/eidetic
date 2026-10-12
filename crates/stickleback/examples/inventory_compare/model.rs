// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use stickleback::inventory::{
    CandidateBudget, Inventory, InventoryLimits, ObjectDigest, PeerInventories, RetainedEntry,
    Snapshot, Summary, SummaryMode,
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const NS: [u8; 32] = [0x49; 32];
const NODES: usize = 8;
const ROUND_BYTES: usize = 4096;
const QUEUE_LIMIT: usize = 64;
pub const MODES: [&str; 3] = ["eager_push", "eager_exact", "eager_cuckoo_fallback"];
pub const SCENARIOS: [&str; 7] = [
    "finite_fit",
    "clean_churn",
    "combined_churn",
    "small_recovery",
    "cold_restart_source",
    "cold_restart_empty",
    "retention_expiry",
];

struct Owner {
    inventory: Inventory,
    peers: PeerInventories,
    payloads: BTreeMap<ObjectDigest, Vec<u8>>,
    fifo: VecDeque<ObjectDigest>,
    history: VecDeque<ObjectDigest>,
    offline_until: u64,
}

impl Owner {
    fn new() -> Self {
        Self {
            inventory: Inventory::new(NS, InventoryLimits::default()).unwrap(),
            peers: PeerInventories::new(NS, 8, 32).unwrap(),
            payloads: BTreeMap::new(),
            fifo: VecDeque::new(),
            history: VecDeque::new(),
            offline_until: 0,
        }
    }

    fn receive(&mut self, item: &Item, now: u64) -> Result<bool> {
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

    fn expire(&mut self, now: u64) -> Result<()> {
        for id in self.inventory.expire(now)? {
            self.payloads.remove(&id);
            self.fifo.retain(|x| *x != id);
        }
        self.peers.expire(now)?;
        Ok(())
    }

    fn check(&self) -> Result<()> {
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

#[derive(Clone)]
struct Item {
    id: ObjectDigest,
    bytes: Vec<u8>,
    publisher: usize,
    expiry: u64,
}
struct Pending {
    source: usize,
    target: usize,
    id: ObjectDigest,
    arrives: u64,
    nonce: u64,
}
struct Advert {
    snapshot: Snapshot,
    cursor: usize,
    started: u64,
    filter: bool,
}

#[derive(Default, Serialize)]
struct Counters {
    control_bytes: usize,
    attempted_payload_frame_bytes: usize,
    useful_received_payload_bytes: usize,
    reserved_result_frame_bytes: usize,
    attempted_result_frame_bytes: usize,
    duplicate_payload_bytes: usize,
    lost_frames: usize,
    backpressured_rounds: usize,
    queue_overflow: usize,
    unavailable: usize,
    saturated_summaries: usize,
    exact_fallback_rounds: usize,
    expired_unreceived_pairs: usize,
    peak_inventory_entries: usize,
    peak_retained_bytes: usize,
    peak_queue_entries: usize,
    peak_peer_slots: usize,
    peak_receiver_reservations: usize,
}

#[derive(Serialize)]
pub struct Run {
    scenario: String,
    mode: String,
    seed: u64,
    ticks: u64,
    published_objects: usize,
    remote_pairs: usize,
    remote_received_pairs: usize,
    remote_received_percent: f64,
    unreceived_pairs: usize,
    final_common_objects: usize,
    final_union_objects: usize,
    final_retained_copies: usize,
    final_pending_payloads: usize,
    retained_bytes_by_node: Vec<usize>,
    state_trace_blake3: String,
    counters: Counters,
}

fn peer(node: usize) -> [u8; 32] {
    [node as u8; 32]
}
fn neighbors(node: usize) -> [usize; 2] {
    [(node + 7) % 8, (node + 1) % 8]
}

fn connected(scenario: &str, tick: u64, source: usize, target: usize, owners: &[Owner]) -> bool {
    if tick < owners[source].offline_until || tick < owners[target].offline_until {
        return false;
    }
    let partition = match scenario {
        "combined_churn" => (80..224).contains(&tick),
        "small_recovery" | "retention_expiry" => (16..160).contains(&tick),
        _ => false,
    };
    !(partition && source / 4 != target / 4)
}

fn loss(
    scenario: &str,
    seed: u64,
    tick: u64,
    source: usize,
    target: usize,
    kind: u8,
    ordinal: u64,
) -> bool {
    if matches!(
        scenario,
        "clean_churn" | "cold_restart_source" | "cold_restart_empty"
    ) {
        return false;
    }
    let mut hash = blake3::Hasher::new_derive_key("mere.inventory.experiment.loss.v1");
    for value in [
        seed,
        tick,
        source as u64,
        target as u64,
        u64::from(kind),
        ordinal,
    ] {
        hash.update(&value.to_le_bytes());
    }
    hash.finalize().as_bytes()[0] < 64
}

fn enqueue(
    queues: &mut BTreeMap<(usize, usize), VecDeque<ObjectDigest>>,
    source: usize,
    id: ObjectDigest,
    counters: &mut Counters,
) {
    for target in neighbors(source) {
        let queue = queues.entry((source, target)).or_default();
        if !queue.contains(&id) {
            if queue.len() == QUEUE_LIMIT {
                queue.pop_front();
                counters.queue_overflow += 1;
            }
            queue.push_back(id);
            counters.peak_queue_entries = counters.peak_queue_entries.max(queue.len());
        }
    }
}

pub fn run(scenario: &str, mode: &str, seed: u64) -> Result<Run> {
    let small = matches!(scenario, "small_recovery" | "retention_expiry");
    let cold = scenario.starts_with("cold_restart");
    let count = if scenario == "finite_fit" {
        32
    } else if small {
        8
    } else if cold {
        32
    } else {
        256
    };
    let ticks = if small || cold { 384 } else { 768 };
    let mut owners: Vec<_> = (0..NODES).map(|_| Owner::new()).collect();
    let mut observed: Vec<BTreeSet<ObjectDigest>> = (0..NODES).map(|_| BTreeSet::new()).collect();
    let mut items = BTreeMap::new();
    let mut queues: BTreeMap<(usize, usize), VecDeque<ObjectDigest>> = BTreeMap::new();
    let mut adverts: BTreeMap<(usize, usize), Advert> = BTreeMap::new();
    let mut pending = Vec::new();
    let mut stats = Counters::default();
    let mut trace = blake3::Hasher::new();
    let mut produced = 0;
    for tick in 0..ticks {
        for owner in &mut owners {
            owner.expire(tick)?;
        }
        let restarts = if cold && tick == 128 {
            (0..8).collect::<Vec<_>>()
        } else if scenario == "combined_churn" && tick == 128 {
            vec![2]
        } else if scenario == "combined_churn" && tick == 288 {
            vec![6]
        } else if small && tick == 112 {
            vec![2]
        } else {
            Vec::new()
        };
        for node in restarts {
            let durable = if scenario == "cold_restart_source" && node == 0 {
                owners[node].payloads.keys().copied().collect::<Vec<_>>()
            } else {
                Vec::new()
            };
            owners[node] = Owner::new();
            owners[node].offline_until = tick + 8;
            for id in durable {
                owners[node].receive(&items[&id], tick)?;
            }
            for owner in &mut owners {
                owner.peers.forget(&peer(node));
            }
            queues.retain(|(s, t), _| *s != node && *t != node);
            adverts.retain(|(s, t), _| *s != node && *t != node);
            pending.retain(|p: &Pending| p.source != node && p.target != node);
        }
        let drained = std::mem::take(&mut pending);
        for p in drained {
            if p.arrives > tick {
                pending.push(p);
                continue;
            }
            if !connected(scenario, tick, p.source, p.target, &owners) {
                stats.lost_frames += 1;
                continue;
            }
            let item = &items[&p.id];
            if tick >= item.expiry {
                stats.unavailable += 1;
                continue;
            }
            let fresh = owners[p.target].receive(item, tick)?;
            stats.attempted_result_frame_bytes += 52;
            if loss(scenario, seed, tick, p.target, p.source, 5, p.nonce) {
                stats.lost_frames += 1;
            }
            if observed[p.target].insert(p.id) {
                stats.useful_received_payload_bytes += item.bytes.len();
            } else {
                stats.duplicate_payload_bytes += item.bytes.len();
            }
            if fresh {
                enqueue(&mut queues, p.target, p.id, &mut stats);
            }
        }
        let publish = if scenario == "finite_fit" {
            tick % 8 == 0 && produced < count
        } else if small {
            tick >= 32 && (tick - 32) % 4 == 0 && produced < count
        } else {
            produced < count
        };
        if publish {
            let publisher = if cold || produced % 2 == 0 { 0 } else { 4 };
            let bytes = (0..(256 + produced % 4 * 128))
                .map(|i| (i as u64 * 17 + produced as u64 * 31) as u8)
                .collect::<Vec<_>>();
            // Include a full counter so the finite workload has distinct content IDs.
            let mut bytes = bytes;
            bytes[..8].copy_from_slice(&(produced as u64).to_le_bytes());
            let id = *blake3::hash(&bytes).as_bytes();
            let expiry = if scenario == "retention_expiry" {
                128
            } else {
                2048
            };
            let item = Item {
                id,
                bytes,
                publisher,
                expiry,
            };
            owners[publisher].receive(&item, tick)?;
            observed[publisher].insert(id);
            enqueue(&mut queues, publisher, id, &mut stats);
            items.insert(id, item);
            produced += 1;
        }
        if tick % 4 == 0 {
            let mut slots = [2usize; NODES];
            let mut fresh_slots = [1usize; NODES];
            let mut round_bytes = [[ROUND_BYTES; NODES]; NODES];
            for phase in 0..3 {
                if mode == "eager_push" && phase != 0 {
                    break;
                }
                for shift in 0..NODES {
                    let source = (shift + (tick / 4) as usize) % NODES;
                    for target in neighbors(source) {
                        if !connected(scenario, tick, source, target, &owners) {
                            continue;
                        }
                        let available = if phase == 0 && mode != "eager_push" {
                            slots[target].min(fresh_slots[target])
                        } else {
                            slots[target]
                        };
                        if available == 0 {
                            stats.backpressured_rounds += 1;
                            continue;
                        }
                        let ids = if phase != 1 {
                            let queue = queues.entry((source, target)).or_default();
                            let mut ids = Vec::new();
                            while ids.len() < available {
                                let Some(id) = queue.pop_front() else {
                                    break;
                                };
                                let Some(payload) = owners[source].payloads.get(&id) else {
                                    stats.unavailable += 1;
                                    continue;
                                };
                                if payload.len() + 56 > round_bytes[source][target] {
                                    queue.push_front(id);
                                    break;
                                }
                                round_bytes[source][target] -= payload.len() + 56;
                                ids.push(id);
                            }
                            ids
                        } else {
                            let key = (source, target);
                            if adverts.get(&key).is_some_and(|a| tick - a.started >= 32) {
                                adverts.remove(&key);
                                owners[source].peers.forget(&peer(target));
                            }
                            if let std::collections::btree_map::Entry::Vacant(slot) =
                                adverts.entry(key)
                            {
                                let fallback =
                                    mode == "eager_cuckoo_fallback" && (tick / 4) % 4 == 0;
                                if fallback {
                                    stats.exact_fallback_rounds += 1;
                                }
                                slot.insert(Advert {
                                    snapshot: owners[target].inventory.snapshot(tick),
                                    cursor: 0,
                                    started: tick,
                                    filter: mode == "eager_cuckoo_fallback" && !fallback,
                                });
                            }
                            let mut complete = false;
                            loop {
                                let advert = &adverts[&key];
                                let summary = if advert.filter {
                                    let build = advert.snapshot.summary(SummaryMode::Cuckoo);
                                    if build.saturated {
                                        stats.saturated_summaries += 1;
                                    }
                                    build.summary
                                } else {
                                    Summary::Exact(advert.snapshot.exact_page(advert.cursor)?)
                                };
                                let wire = summary.encode();
                                let request = if advert.cursor == 0 { 0 } else { 52 };
                                let cost = wire.len() + 4 + request;
                                if cost > round_bytes[source][target] {
                                    break;
                                }
                                let cursor = advert.cursor;
                                round_bytes[source][target] -= request;
                                stats.control_bytes += request;
                                if request != 0
                                    && loss(scenario, seed, tick, source, target, 3, cursor as u64)
                                {
                                    stats.lost_frames += 1;
                                    break;
                                }
                                round_bytes[source][target] -= wire.len() + 4;
                                stats.control_bytes += wire.len() + 4;
                                if loss(scenario, seed, tick, target, source, 0, cursor as u64) {
                                    stats.lost_frames += 1;
                                    break;
                                }
                                let observation = owners[source].peers.observe(
                                    peer(target),
                                    &Summary::decode(&wire)?,
                                    tick,
                                )?;
                                if observation.complete {
                                    complete = true;
                                    break;
                                }
                                let advert = adverts.get_mut(&key).unwrap();
                                advert.cursor =
                                    observation.next_cursor.ok_or("incomplete cursor")?;
                                advert.filter = false;
                            }
                            if !complete {
                                continue;
                            }
                            adverts.remove(&key);
                            let reserve = 2 * (52 + 32 * available) + 56 * available;
                            if round_bytes[source][target] <= reserve {
                                continue;
                            }
                            let budget = CandidateBudget::new(
                                available,
                                round_bytes[source][target] - reserve,
                            )?;
                            let Owner {
                                inventory, peers, ..
                            } = &mut owners[source];
                            let offers = peers.offers(inventory, &peer(target), tick, budget)?;
                            if offers.is_empty() {
                                continue;
                            }
                            let control = 52 + 32 * offers.len();
                            round_bytes[source][target] -= control;
                            stats.control_bytes += control;
                            if loss(scenario, seed, tick, source, target, 1, 0) {
                                stats.lost_frames += 1;
                                continue;
                            }
                            round_bytes[source][target] -= control;
                            stats.control_bytes += control;
                            if loss(scenario, seed, tick, target, source, 2, 0) {
                                stats.lost_frames += 1;
                                continue;
                            }
                            let ids = offers.iter().map(|o| o.digest).collect::<Vec<_>>();
                            for id in &ids {
                                round_bytes[source][target] -=
                                    owners[source].payloads[id].len() + 56;
                            }
                            ids
                        };
                        if round_bytes[source][target] > ROUND_BYTES {
                            return Err("round byte budget overflow".into());
                        }
                        for (ordinal, id) in ids.into_iter().enumerate() {
                            slots[target] -= 1;
                            if phase == 0 && mode != "eager_push" {
                                fresh_slots[target] -= 1;
                            }
                            stats.peak_receiver_reservations =
                                stats.peak_receiver_reservations.max(2 - slots[target]);
                            let Some(payload) = owners[source].payloads.get(&id) else {
                                stats.unavailable += 1;
                                continue;
                            };
                            stats.attempted_payload_frame_bytes += payload.len() + 4;
                            stats.reserved_result_frame_bytes += 52;
                            let nonce = ordinal as u64 + phase * 8;
                            if loss(scenario, seed, tick, source, target, 4, nonce) {
                                stats.lost_frames += 1;
                            } else {
                                pending.push(Pending {
                                    source,
                                    target,
                                    id,
                                    arrives: tick + 1,
                                    nonce,
                                });
                            }
                        }
                    }
                }
            }
        }
        for (node, owner) in owners.iter().enumerate() {
            owner.check()?;
            stats.peak_inventory_entries = stats.peak_inventory_entries.max(owner.inventory.len());
            stats.peak_retained_bytes = stats
                .peak_retained_bytes
                .max(owner.inventory.retained_bytes());
            stats.peak_peer_slots = stats.peak_peer_slots.max(owner.peers.len());
            trace.update(&tick.to_le_bytes());
            trace.update(&(node as u64).to_le_bytes());
            for entry in owner.inventory.entries() {
                trace.update(&entry.digest);
            }
            trace.update(&(observed[node].len() as u64).to_le_bytes());
        }
    }
    let remote_pairs = items.len() * 7;
    let remote_received_pairs = items
        .values()
        .map(|item| {
            (0..NODES)
                .filter(|n| *n != item.publisher && observed[*n].contains(&item.id))
                .count()
        })
        .sum::<usize>();
    stats.expired_unreceived_pairs = items
        .values()
        .filter(|i| i.expiry <= ticks)
        .map(|item| {
            (0..NODES)
                .filter(|n| *n != item.publisher && !observed[*n].contains(&item.id))
                .count()
        })
        .sum();
    let union = owners
        .iter()
        .flat_map(|o| o.payloads.keys().copied())
        .collect::<BTreeSet<_>>();
    let common = union
        .iter()
        .filter(|id| owners.iter().all(|o| o.payloads.contains_key(*id)))
        .count();
    if scenario == "small_recovery" && mode != "eager_push" && common != count {
        return Err(
            format!("small-set recovery failed: {mode} seed {seed}, common {common}").into(),
        );
    }
    if scenario == "finite_fit" && mode != "eager_push" && common != count {
        return Err("finite fitting workload did not recover".into());
    }
    if scenario == "cold_restart_empty" && !union.is_empty() {
        return Err("cold caches not empty".into());
    }
    if scenario == "cold_restart_source" && mode != "eager_push" && common != count {
        return Err("retained source failed restart recovery".into());
    }
    Ok(Run {
        scenario: scenario.into(),
        mode: mode.into(),
        seed,
        ticks,
        published_objects: items.len(),
        remote_pairs,
        remote_received_pairs,
        remote_received_percent: 100.0 * remote_received_pairs as f64 / remote_pairs as f64,
        unreceived_pairs: remote_pairs - remote_received_pairs,
        final_common_objects: common,
        final_union_objects: union.len(),
        final_retained_copies: owners.iter().map(|o| o.payloads.len()).sum(),
        final_pending_payloads: pending.len(),
        retained_bytes_by_node: owners
            .iter()
            .map(|o| o.inventory.retained_bytes())
            .collect(),
        state_trace_blake3: trace.finalize().to_hex().to_string(),
        counters: stats,
    })
}
