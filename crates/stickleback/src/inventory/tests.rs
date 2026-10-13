// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;

fn id(n: u64) -> ObjectDigest {
    *blake3::hash(&n.to_le_bytes()).as_bytes()
}
fn inventory() -> Inventory {
    Inventory::new([7; 32], InventoryLimits::default()).unwrap()
}
fn entry(n: u64) -> RetainedEntry {
    RetainedEntry {
        digest: id(n),
        bytes: 128,
        expires_at: 1000,
    }
}
fn roundtrip(summary: &Summary) -> Summary {
    let encoded = summary.encode();
    assert!(encoded.len() <= MAX_CONTROL_BYTES);
    let decoded = Summary::decode(&encoded).unwrap();
    assert_eq!(decoded.encode(), encoded);
    decoded
}

#[test]
fn eviction_churn_rebuilds_without_losing_any_retained_membership() {
    let mut inv = inventory();
    for n in 0..1024 {
        if n >= 64 {
            inv.remove(&id(n - 64)).unwrap();
        }
        inv.retain(entry(n), 0).unwrap();
        let build = inv.snapshot(0).summary(SummaryMode::Cuckoo);
        let summary = roundtrip(&build.summary);
        if let Summary::Cuckoo(s) = summary {
            for retained in inv.entries() {
                assert!(s.filter.contains(&[7; 32], &retained.digest));
            }
        }
        assert_eq!(inv.len(), (n as usize + 1).min(64));
        assert_eq!(inv.retained_bytes(), inv.len() * 128);
    }
}

#[test]
fn capacity_refusal_is_transactional_and_expiry_removes_offers() {
    let mut inv = Inventory::new(
        [7; 32],
        InventoryLimits {
            entries: 2,
            retained_bytes: 256,
            object_bytes: 256,
        },
    )
    .unwrap();
    inv.retain(entry(1), 0).unwrap();
    let before = inv.snapshot(0).summary(SummaryMode::Exact).summary.encode();
    let mut oversized = entry(2);
    oversized.bytes = 256;
    assert_eq!(inv.retain(oversized, 0), Err(InventoryError::Capacity));
    assert_eq!(
        inv.snapshot(0).summary(SummaryMode::Exact).summary.encode(),
        before
    );
    let removed = inv.expire(1000).unwrap();
    assert_eq!(removed, vec![id(1)]);
    assert!(inv.is_empty());
    assert_eq!(inv.retained_bytes(), 0);
    assert_eq!(inv.retain(entry(2), 1000), Err(InventoryError::Object));
}

#[test]
fn pagination_is_atomic_idempotent_and_binds_one_snapshot() {
    let mut remote = inventory();
    for n in 0..19 {
        remote.retain(entry(n), 0).unwrap();
    }
    let snapshot = remote.snapshot(0);
    let mut peers = PeerInventories::new([7; 32], 2, 10).unwrap();
    let peer = [1; 32];
    let first = roundtrip(&Summary::Exact(snapshot.exact_page(0).unwrap()));
    let seen = peers.observe(peer, &first, 0).unwrap();
    assert!(!seen.complete);
    assert_eq!(seen.next_cursor, Some(8));
    assert_eq!(peers.observe(peer, &first, 0).unwrap().next_cursor, Some(8));
    assert!(matches!(
        peers.offers(
            &inventory(),
            &peer,
            0,
            CandidateBudget::new(8, 1024).unwrap()
        ),
        Err(InventoryError::Incomplete)
    ));
    remote.remove(&id(0)).unwrap();
    let changed = Summary::Exact(remote.snapshot(0).exact_page(8).unwrap());
    assert!(matches!(
        peers.observe(peer, &changed, 0),
        Err(InventoryError::Stale)
    ));
    for start in [8, 16] {
        peers
            .observe(
                peer,
                &roundtrip(&Summary::Exact(snapshot.exact_page(start).unwrap())),
                0,
            )
            .unwrap();
    }
    let mut local = inventory();
    local.retain(entry(0), 0).unwrap();
    local.retain(entry(100), 0).unwrap();
    let offers = peers
        .offers(&local, &peer, 0, CandidateBudget::new(8, 1024).unwrap())
        .unwrap();
    assert_eq!(offers.len(), 1);
    assert_eq!(offers[0].digest, id(100));
}

#[test]
fn false_positive_is_repaired_by_exact_fallback() {
    let remote = inventory();
    let mut fake = remote.snapshot(0).summary(SummaryMode::Cuckoo).summary;
    let Summary::Cuckoo(ref mut s) = fake else {
        panic!("empty filter");
    };
    let (fp, bucket) = filter::fingerprint(&[7; 32], &id(9));
    s.filter.cells[bucket * filter::SLOTS] = fp;
    s.total = 1;
    let mut peers = PeerInventories::new([7; 32], 1, 10).unwrap();
    let peer = [1; 32];
    let mut local = inventory();
    local.retain(entry(9), 0).unwrap();
    peers.observe(peer, &roundtrip(&fake), 0).unwrap();
    let budget = CandidateBudget::new(8, 1024).unwrap();
    assert!(peers.offers(&local, &peer, 0, budget).unwrap().is_empty());
    peers
        .observe(
            peer,
            &roundtrip(&Summary::Exact(remote.snapshot(0).exact_page(0).unwrap())),
            1,
        )
        .unwrap();
    assert_eq!(
        peers.offers(&local, &peer, 1, budget).unwrap()[0].digest,
        id(9)
    );
}

#[test]
fn peer_idle_capacity_namespace_and_payload_budgets_are_enforced() {
    let empty = inventory().snapshot(0).summary(SummaryMode::Exact).summary;
    let mut peers = PeerInventories::new([7; 32], 1, 5).unwrap();
    peers.observe([1; 32], &empty, 0).unwrap();
    assert!(matches!(
        peers.observe([2; 32], &empty, 1),
        Err(InventoryError::Capacity)
    ));
    assert!(peers.observe([2; 32], &empty, 5).is_ok());
    assert_eq!(peers.len(), 1);
    assert!(matches!(peers.expire(4), Err(InventoryError::Clock)));
    let wrong = Inventory::new([8; 32], InventoryLimits::default())
        .unwrap()
        .snapshot(0)
        .summary(SummaryMode::Exact)
        .summary;
    assert!(matches!(
        peers.observe([2; 32], &wrong, 5),
        Err(InventoryError::Namespace)
    ));
    let mut local = inventory();
    for n in 0..10 {
        local.retain(entry(n), 5).unwrap();
    }
    let offers = peers
        .offers(&local, &[2; 32], 5, CandidateBudget::new(8, 256).unwrap())
        .unwrap();
    assert_eq!(offers.len(), 2);
    assert_eq!(offers.iter().map(|o| o.bytes).sum::<usize>(), 256);
}

#[test]
fn malformed_wire_and_incomplete_snapshot_never_become_inventory() {
    let mut inv = inventory();
    inv.retain(entry(1), 0).unwrap();
    for mode in [SummaryMode::Exact, SummaryMode::Cuckoo] {
        let wire = inv.snapshot(0).summary(mode).summary.encode();
        for cut in 0..wire.len() {
            assert!(Summary::decode(&wire[..cut]).is_err());
        }
        let mut trailing = wire.clone();
        trailing.push(0);
        assert!(Summary::decode(&trailing).is_err());
        let mut version = wire;
        version[3] = b'2';
        assert!(Summary::decode(&version).is_err());
    }
    assert!(Summary::decode(&vec![0; MAX_CONTROL_BYTES + 1]).is_err());
    let mut corrupt = inv.snapshot(0).summary(SummaryMode::Exact).summary.encode();
    corrupt[45] ^= 1;
    let summary = Summary::decode(&corrupt).unwrap();
    let mut peers = PeerInventories::new([7; 32], 1, 5).unwrap();
    assert!(matches!(
        peers.observe([1; 32], &summary, 0),
        Err(InventoryError::Stale)
    ));
    assert!(peers.is_empty());
}

#[test]
fn pathological_bucket_pair_saturates_into_exact_without_dropping_ids() {
    let mut inv = inventory();
    let mut selected = 0;
    for n in 0..100_000 {
        let (fp, index) = filter::fingerprint(&[7; 32], &id(n));
        let hash = blake3::derive_key("mere.inventory.cuckoo.alternate.v1", &fp.to_le_bytes());
        if index == 0 && (usize::from(hash[0]) & 31) | 1 == 1 {
            inv.retain(entry(n), 0).unwrap();
            selected += 1;
            if selected == 9 {
                break;
            }
        }
    }
    assert_eq!(selected, 9);
    let snapshot = inv.snapshot(0);
    let build = snapshot.summary(SummaryMode::Cuckoo);
    assert!(build.saturated);
    assert!(matches!(build.summary, Summary::Exact(_)));
    let mut peers = PeerInventories::new([7; 32], 1, 5).unwrap();
    peers
        .observe([1; 32], &roundtrip(&build.summary), 0)
        .unwrap();
    peers
        .observe([1; 32], &Summary::Exact(snapshot.exact_page(8).unwrap()), 0)
        .unwrap();
    assert!(
        peers
            .offers(&inv, &[1; 32], 0, CandidateBudget::new(8, 1024).unwrap())
            .unwrap()
            .is_empty()
    );
}

#[test]
fn bounded_offers_rotate_when_replies_are_lost() {
    let mut local = inventory();
    for n in 0..10 {
        local.retain(entry(n), 0).unwrap();
    }
    let empty = inventory().snapshot(0).summary(SummaryMode::Exact).summary;
    let mut peers = PeerInventories::new([7; 32], 1, 10).unwrap();
    let peer = [1; 32];
    let mut offered = std::collections::BTreeSet::new();
    for now in 0..5 {
        peers.observe(peer, &empty, now).unwrap();
        for offer in peers
            .offers(&local, &peer, now, CandidateBudget::new(2, 256).unwrap())
            .unwrap()
        {
            offered.insert(offer.digest);
        }
    }
    assert_eq!(offered.len(), 10);
}
