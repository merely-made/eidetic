// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;
use serde::{Deserialize, Serialize};

#[test]
fn explicit_parent_admission_preserves_a_live_cursor_and_forward_choice() {
    let mut memory = Stemma::<String, String, String, ()>::new();
    let owner = memory.ensure_owner("live".into(), None);
    let a = memory.resolve_or_create_entry("a".into(), "a".into(), 100, EntryPrivacy::LocalOnly);
    let b = memory.resolve_or_create_entry("b".into(), "b".into(), 200, EntryPrivacy::LocalOnly);
    let root = memory
        .visit_entry(owner, a, (), TransitionKind::UrlTyped, 100)
        .unwrap();
    let forward = memory
        .visit_entry(owner, b, (), TransitionKind::LinkClick, 200)
        .unwrap();
    memory.back(owner, 1, 300).unwrap();
    let admitted = memory
        .admit_visit(owner, b, Some(root), (), TransitionKind::Imported, 150)
        .unwrap();
    assert_eq!(memory.current_visit_of_owner(owner), Some(root));
    assert_eq!(memory.owner(owner).unwrap().origin, Some(root));
    assert_eq!(
        memory.visit(root).unwrap().bindings[&owner].forward_child,
        Some(forward)
    );
    assert_eq!(memory.visit(admitted).unwrap().parent, Some(root));
    assert_eq!(memory.visit(root).unwrap().children.len(), 2);
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct EntryData {
    url: String,
    title: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
struct OwnerName(String);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct VisitContext {
    label: String,
}

type Memory = Stemma<(String, Option<String>), EntryData, OwnerName, VisitContext>;

fn entry(url: &str, title: &str) -> EntryData {
    EntryData {
        url: url.to_string(),
        title: title.to_string(),
    }
}

fn owner(name: &str) -> OwnerName {
    OwnerName(name.to_string())
}

fn ctx(label: &str) -> VisitContext {
    VisitContext {
        label: label.to_string(),
    }
}

#[test]
fn entry_deduplicates_by_key_but_allows_contextual_duplicates() {
    let mut memory = Memory::new();

    let a1 = memory.resolve_or_create_entry(
        (
            "https://example.com".to_string(),
            Some("workspace-a".to_string()),
        ),
        entry("https://example.com", "First"),
        1,
        EntryPrivacy::LocalOnly,
    );
    let a2 = memory.resolve_or_create_entry(
        (
            "https://example.com".to_string(),
            Some("workspace-a".to_string()),
        ),
        entry("https://example.com", "Updated"),
        2,
        EntryPrivacy::ShareCandidate,
    );
    let b = memory.resolve_or_create_entry(
        (
            "https://example.com".to_string(),
            Some("workspace-b".to_string()),
        ),
        entry("https://example.com", "Parallel"),
        3,
        EntryPrivacy::LocalOnly,
    );

    assert_eq!(a1, a2);
    assert_ne!(a1, b);
    assert_eq!(memory.entry_count(), 2);
    assert_eq!(memory.entry(a1).unwrap().payload.title, "Updated");
    assert_eq!(
        memory.entry(a1).unwrap().privacy,
        EntryPrivacy::ShareCandidate
    );
}

#[test]
fn spawned_owner_origin_attaches_under_creator_current_visit() {
    let mut memory = Memory::new();
    let root_entry = memory.resolve_or_create_entry(
        ("https://a.example".to_string(), None),
        entry("https://a.example", "A"),
        1,
        EntryPrivacy::LocalOnly,
    );
    let child_entry = memory.resolve_or_create_entry(
        ("https://b.example".to_string(), None),
        entry("https://b.example", "B"),
        2,
        EntryPrivacy::LocalOnly,
    );
    let spawned_entry = memory.resolve_or_create_entry(
        ("https://c.example".to_string(), None),
        entry("https://c.example", "C"),
        3,
        EntryPrivacy::LocalOnly,
    );

    let x = memory.ensure_owner(owner("x"), None);
    let _root = memory
        .visit_entry(x, root_entry, ctx("root"), TransitionKind::UrlTyped, 10)
        .unwrap();
    let current = memory
        .visit_entry(x, child_entry, ctx("child"), TransitionKind::LinkClick, 20)
        .unwrap();

    let y = memory.ensure_owner(owner("y"), Some(x));
    let spawned = memory
        .visit_entry(y, spawned_entry, ctx("spawn"), TransitionKind::TabSpawn, 30)
        .unwrap();

    assert_eq!(memory.owner(y).unwrap().creator, Some(x));
    assert_eq!(memory.owner(y).unwrap().origin, Some(spawned));
    assert_eq!(memory.visit(spawned).unwrap().parent, Some(current));
}

#[test]
fn deleting_a_spawned_owner_leaves_a_serializable_snapshot() {
    // Regression (2026-06-06): deleting an owner that was spawned under a creator's
    // current visit left a dangling forward-child binding on the creator's visit,
    // which `to_snapshot` then panicked indexing.
    let mut memory = Memory::new();
    let a_entry = memory.resolve_or_create_entry(
        ("a".to_string(), None),
        entry("a", "A"),
        1,
        EntryPrivacy::LocalOnly,
    );
    let c_entry = memory.resolve_or_create_entry(
        ("c".to_string(), None),
        entry("c", "C"),
        2,
        EntryPrivacy::LocalOnly,
    );
    let a = memory.ensure_owner(owner("a"), None);
    memory
        .visit_entry(a, a_entry, ctx("a"), TransitionKind::UrlTyped, 10)
        .unwrap();
    // Spawn a child under a's current visit, give it a visit, then delete it.
    let child = memory.ensure_owner(owner("c"), Some(a));
    memory
        .visit_entry(child, c_entry, ctx("c"), TransitionKind::TabSpawn, 20)
        .unwrap();
    memory.delete_owner(child).unwrap();
    // The creator survives and the snapshot round-trips without panicking.
    let restored = Memory::from_snapshot(memory.to_snapshot());
    assert!(
        restored.owner_id_by_identity(&owner("c")).is_none(),
        "spawned owner gone"
    );
    assert!(
        restored.owner_id_by_identity(&owner("a")).is_some(),
        "creator survives"
    );
}

#[test]
fn forward_child_is_owner_scoped_on_shared_visit() {
    let mut memory = Memory::new();

    let a = memory.resolve_or_create_entry(
        ("https://a.example".to_string(), None),
        entry("https://a.example", "A"),
        1,
        EntryPrivacy::LocalOnly,
    );
    let b = memory.resolve_or_create_entry(
        ("https://b.example".to_string(), None),
        entry("https://b.example", "B"),
        2,
        EntryPrivacy::LocalOnly,
    );
    let c = memory.resolve_or_create_entry(
        ("https://c.example".to_string(), None),
        entry("https://c.example", "C"),
        3,
        EntryPrivacy::LocalOnly,
    );

    let x = memory.ensure_owner(owner("x"), None);
    let a_visit = memory
        .visit_entry(x, a, ctx("a"), TransitionKind::UrlTyped, 10)
        .unwrap();
    let b_visit = memory
        .visit_entry(x, b, ctx("b"), TransitionKind::LinkClick, 20)
        .unwrap();

    let y = memory.ensure_owner(owner("y"), None);
    memory.adopt_visit(y, a_visit, 25).unwrap();
    let c_visit = memory
        .visit_entry(y, c, ctx("c"), TransitionKind::LinkClick, 30)
        .unwrap();

    memory.back(x, 1, 40).unwrap();
    memory.back(y, 1, 41).unwrap();

    assert_eq!(
        memory
            .visit(a_visit)
            .unwrap()
            .bindings
            .get(&x)
            .unwrap()
            .forward_child,
        Some(b_visit)
    );
    assert_eq!(
        memory
            .visit(a_visit)
            .unwrap()
            .bindings
            .get(&y)
            .unwrap()
            .forward_child,
        Some(c_visit)
    );

    assert_eq!(memory.forward(x, 1, 50).unwrap(), Some(b_visit));
    assert_eq!(memory.forward(y, 1, 51).unwrap(), Some(c_visit));
}

#[test]
fn repeated_navigation_creates_distinct_visits_and_aggregates_edges() {
    let mut memory = Memory::new();

    let a = memory.resolve_or_create_entry(
        ("https://a.example".to_string(), None),
        entry("https://a.example", "A"),
        1,
        EntryPrivacy::LocalOnly,
    );
    let b = memory.resolve_or_create_entry(
        ("https://b.example".to_string(), None),
        entry("https://b.example", "B"),
        2,
        EntryPrivacy::LocalOnly,
    );

    let owner = memory.ensure_owner(owner("x"), None);
    let a_visit = memory
        .visit_entry(owner, a, ctx("a"), TransitionKind::UrlTyped, 10)
        .unwrap();
    let first_b = memory
        .visit_entry(owner, b, ctx("b1"), TransitionKind::LinkClick, 20)
        .unwrap();
    memory.back(owner, 1, 30).unwrap();
    let second_b = memory
        .visit_entry(owner, b, ctx("b2"), TransitionKind::Reload, 40)
        .unwrap();

    assert_ne!(first_b, second_b);
    assert_eq!(memory.visit(second_b).unwrap().parent, Some(a_visit));

    let aggregated = memory.aggregated_entry_edges();
    assert_eq!(aggregated.len(), 1);
    let edge = &aggregated[0];
    assert_eq!(edge.from_entry, a);
    assert_eq!(edge.to_entry, b);
    assert_eq!(edge.traversal_count, 2);
    assert_eq!(
        edge.transition_counts.get(&TransitionKind::LinkClick),
        Some(&1)
    );
    assert_eq!(
        edge.transition_counts.get(&TransitionKind::Reload),
        Some(&1)
    );
}

#[test]
fn owner_branch_projection_reports_alternate_children() {
    let mut memory = Memory::new();

    let a = memory.resolve_or_create_entry(
        ("https://a.example".to_string(), None),
        entry("https://a.example", "A"),
        1,
        EntryPrivacy::LocalOnly,
    );
    let b = memory.resolve_or_create_entry(
        ("https://b.example".to_string(), None),
        entry("https://b.example", "B"),
        2,
        EntryPrivacy::LocalOnly,
    );
    let c = memory.resolve_or_create_entry(
        ("https://c.example".to_string(), None),
        entry("https://c.example", "C"),
        3,
        EntryPrivacy::LocalOnly,
    );

    let owner_id = memory.ensure_owner(owner("x"), None);
    let a_visit = memory
        .visit_entry(owner_id, a, ctx("a"), TransitionKind::UrlTyped, 10)
        .unwrap();
    let b_visit = memory
        .visit_entry(owner_id, b, ctx("b"), TransitionKind::LinkClick, 20)
        .unwrap();

    memory.back(owner_id, 1, 30).unwrap();
    let c_visit = memory
        .visit_entry(owner_id, c, ctx("c"), TransitionKind::Reload, 40)
        .unwrap();

    let projection = memory.owner_branch_projection(owner_id).unwrap();
    assert_eq!(projection.current_index, Some(1));
    assert_eq!(projection.visits.len(), 2);
    assert_eq!(projection.visits[0].visit_id, a_visit);
    assert_eq!(projection.visits[1].visit_id, c_visit);
    assert!(projection.visits[1].is_current);
    assert_eq!(projection.visits[0].alternate_children.len(), 1);
    assert_eq!(projection.visits[0].alternate_children[0].visit_id, b_visit);
    assert_eq!(
        projection.visits[0].alternate_children[0].payload.url,
        "https://b.example"
    );
}

#[test]
fn deleting_last_owner_collects_ownerless_branch() {
    let mut memory = Memory::new();

    let d = memory.resolve_or_create_entry(
        ("https://d.example".to_string(), None),
        entry("https://d.example", "D"),
        1,
        EntryPrivacy::LocalOnly,
    );
    let e = memory.resolve_or_create_entry(
        ("https://e.example".to_string(), None),
        entry("https://e.example", "E"),
        2,
        EntryPrivacy::LocalOnly,
    );

    let owner = memory.ensure_owner(owner("solo"), None);
    let d_visit = memory
        .visit_entry(owner, d, ctx("d"), TransitionKind::UrlTyped, 10)
        .unwrap();
    let e_visit = memory
        .visit_entry(owner, e, ctx("e"), TransitionKind::LinkClick, 20)
        .unwrap();

    let report = memory.delete_owner(owner).unwrap();
    assert!(report.deleted_visits.contains(&d_visit));
    assert!(report.deleted_visits.contains(&e_visit));
    assert_eq!(memory.owner_count(), 0);
    assert_eq!(memory.visit_count(), 0);
}

fn page_entry(memory: &mut Memory) -> EntryId {
    let key = ("page".to_string(), None);
    memory.resolve_or_create_entry(key, entry("page", "Page"), 1, EntryPrivacy::LocalOnly)
}

#[test]
fn deleting_a_creator_reparents_its_surviving_owners_to_the_creators_creator() {
    let mut memory = Memory::new();
    let page = page_entry(&mut memory);
    let grandparent = memory.ensure_owner(owner("grandparent"), None);
    memory
        .visit_entry(grandparent, page, ctx("g"), TransitionKind::UrlTyped, 10)
        .unwrap();
    let creator = memory.ensure_owner(owner("creator"), Some(grandparent));
    memory
        .visit_entry(creator, page, ctx("c"), TransitionKind::TabSpawn, 20)
        .unwrap();
    let survivor = memory.ensure_owner(owner("survivor"), Some(creator));
    memory
        .visit_entry(survivor, page, ctx("s"), TransitionKind::TabSpawn, 30)
        .unwrap();

    memory.delete_owner(creator).unwrap();

    assert_eq!(memory.owner(survivor).unwrap().creator, Some(grandparent));
    let restored = Memory::from_snapshot(memory.to_snapshot());
    let survivor = restored.owner_id_by_identity(&owner("survivor")).unwrap();
    let grandparent = restored.owner_id_by_identity(&owner("grandparent")).unwrap();
    assert_eq!(restored.owner(survivor).unwrap().creator, Some(grandparent));
}

#[test]
fn deleting_a_creator_with_no_creator_clears_its_surviving_owners_creator() {
    let mut memory = Memory::new();
    let page = page_entry(&mut memory);
    let creator = memory.ensure_owner(owner("creator"), None);
    memory
        .visit_entry(creator, page, ctx("c"), TransitionKind::UrlTyped, 10)
        .unwrap();
    let survivor = memory.ensure_owner(owner("survivor"), Some(creator));
    memory
        .visit_entry(survivor, page, ctx("s"), TransitionKind::TabSpawn, 20)
        .unwrap();

    memory.delete_owner(creator).unwrap();

    assert_eq!(memory.owner(survivor).unwrap().creator, None);
    let restored = Memory::from_snapshot(memory.to_snapshot());
    let survivor = restored.owner_id_by_identity(&owner("survivor")).unwrap();
    assert_eq!(restored.owner(survivor).unwrap().creator, None);
}

#[test]
fn deleting_a_creator_leaves_no_pending_origin_on_a_collected_visit() {
    let mut memory = Memory::new();
    let page = page_entry(&mut memory);
    let creator = memory.ensure_owner(owner("creator"), None);
    memory
        .visit_entry(creator, page, ctx("c"), TransitionKind::UrlTyped, 10)
        .unwrap();
    // Spawned beneath the creator's visit but not yet navigated, so it holds
    // only a pending origin parent, which the creator's deletion collects.
    let survivor = memory.ensure_owner(owner("survivor"), Some(creator));

    memory.delete_owner(creator).unwrap();

    assert_eq!(memory.owner(survivor).unwrap().pending_origin_parent, None);
    Memory::from_snapshot(memory.to_snapshot());
}

/// Twelve owners sharing one visit and one owner holding twelve visits: enough
/// map and set entries that two independently built hash tables almost surely
/// iterate in different orders.
fn crowded() -> Memory {
    let mut memory = Memory::new();
    let page = page_entry(&mut memory);
    let first = memory.ensure_owner(owner("o0"), None);
    let shared = memory
        .visit_entry(first, page, ctx("shared"), TransitionKind::UrlTyped, 10)
        .unwrap();
    for n in 1..12 {
        let other = memory.ensure_owner(owner(&format!("o{n}")), None);
        memory.adopt_visit(other, shared, 10 + n).unwrap();
    }
    for n in 0..12 {
        memory
            .visit_entry(first, page, ctx(&format!("v{n}")), TransitionKind::LinkClick, 100 + n)
            .unwrap();
    }
    memory
}

#[test]
fn snapshots_of_equal_navigation_state_are_equal_whatever_the_hash_order() {
    assert_eq!(crowded().to_snapshot(), crowded().to_snapshot());
}

#[test]
fn hydrating_and_rewriting_a_snapshot_leaves_it_unchanged() {
    let snapshot = crowded().to_snapshot();
    let mut rewritten = snapshot.clone();
    for _ in 0..4 {
        rewritten = Memory::from_snapshot(rewritten).to_snapshot();
        assert_eq!(rewritten, snapshot);
    }
}

#[test]
fn snapshot_lists_bindings_and_owned_visits_in_ascending_index_order() {
    let snapshot = crowded().to_snapshot();
    assert!(snapshot.visits.iter().any(|visit| visit.bindings.len() == 12));
    for visit in &snapshot.visits {
        assert!(visit.bindings.is_sorted_by_key(|binding| binding.owner));
    }
    assert!(snapshot.owners.iter().any(|owner| owner.owned_visits.len() == 13));
    for owner in &snapshot.owners {
        assert!(owner.owned_visits.is_sorted());
    }
}
