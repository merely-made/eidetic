// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// M0/M1 of the commons multi-writer convergence plan (mere design_docs,
/// 2026-07-26). Through 0.1.x `EdgeId` was a bare per-log counter, so two
/// partitioned replicas both minted `EdgeId(0)` for unrelated edges. The
/// identity is now `(writer, counter)`, and this asserts the ids differ.
#[test]
fn two_offline_writers_mint_distinct_edge_ids() {
    let alice = Author::new("alice");
    let bob = Author::new("bob");

    // Two replicas of one container, edited while partitioned.
    let mut a = GraphLog::<Container, Relation>::new().for_writer(writer(0xa1));
    a.insert_node(&alice, Container::new("a"));
    a.insert_node(&alice, Container::new("b"));
    let a_edge = a
        .connect(&alice, &"a".to_string(), &"b".to_string(), cites())
        .expect("alice connects her own pair");

    let mut b = GraphLog::<Container, Relation>::new().for_writer(writer(0xb2));
    b.insert_node(&bob, Container::new("c"));
    b.insert_node(&bob, Container::new("d"));
    let b_edge = b
        .connect(&bob, &"c".to_string(), &"d".to_string(), cites())
        .expect("bob connects his own pair");

    assert_ne!(
        a_edge, b_edge,
        "distinct replicas mint distinct ids for distinct edges"
    );
    assert_eq!(
        a_edge.counter, b_edge.counter,
        "the counters still collide; the writer half is what separates them"
    );
}

/// The payoff: merging both journals leaves every edge addressable, so a
/// retraction names exactly one edge.
#[test]
fn a_merged_journal_addresses_both_writers_edges() {
    let alice = Author::new("alice");
    let bob = Author::new("bob");

    let mut a = GraphLog::<Container, Relation>::new().for_writer(writer(0xa1));
    a.insert_node(&alice, Container::new("a"));
    a.insert_node(&alice, Container::new("b"));
    let a_edge = a
        .connect(&alice, &"a".to_string(), &"b".to_string(), cites())
        .expect("alice connects");

    let mut b = GraphLog::<Container, Relation>::new().for_writer(writer(0xb2));
    b.insert_node(&bob, Container::new("c"));
    b.insert_node(&bob, Container::new("d"));
    let b_edge = b
        .connect(&bob, &"c".to_string(), &"d".to_string(), cites())
        .expect("bob connects");

    // Merge: one journal carrying both writers' batches, in the deterministic
    // order the replication layer would impose. Ids are carried in the
    // `Connect` edits, so replay reuses them rather than re-minting.
    let mut merged = Journal::new();
    for batch in a.log().entries().iter().chain(b.log().entries()) {
        merged.append(batch.clone());
    }
    let merged = GraphLog::<Container, Relation>::replay(merged);

    assert_eq!(
        merged.graph().edge_count(),
        2,
        "both edges survive the merge"
    );
    let a_key = merged
        .edge_key(a_edge)
        .expect("alice's edge is addressable");
    let b_key = merged.edge_key(b_edge).expect("bob's edge is addressable");
    assert_ne!(a_key, b_key, "each id names a different edge");
}

/// Replaying a peer's edits must not consume this replica's counter range,
/// or the next local mint would reuse an id the peer already used.
#[test]
fn replaying_a_peers_edits_does_not_advance_our_counter() {
    let alice = Author::new("alice");
    let bob = Author::new("bob");

    let mut b = GraphLog::<Container, Relation>::new().for_writer(writer(0xb2));
    b.insert_node(&bob, Container::new("c"));
    b.insert_node(&bob, Container::new("d"));
    for _ in 0..5 {
        b.connect(&bob, &"c".to_string(), &"d".to_string(), cites())
            .expect("bob connects repeatedly");
    }

    // Alice replays bob's whole journal, then mints her own edge.
    let mut merged = Journal::new();
    for batch in b.log().entries() {
        merged.append(batch.clone());
    }
    let mut a = GraphLog::<Container, Relation>::replay(merged).for_writer(writer(0xa1));
    a.insert_node(&alice, Container::new("a"));
    a.insert_node(&alice, Container::new("b"));
    let a_edge = a
        .connect(&alice, &"a".to_string(), &"b".to_string(), cites())
        .expect("alice connects");

    assert_eq!(
        a_edge,
        EdgeId::new(writer(0xa1), 0),
        "alice starts her own range at zero, undisturbed by bob's five edges"
    );
}

/// Merge two journals in a stated order, as the replication layer's
/// deterministic `(verifying_key, log_id, seq_num)` sort would.
fn merge(
    first: &GraphLog<Container, Relation>,
    second: &GraphLog<Container, Relation>,
) -> GraphLog<Container, Relation> {
    let mut merged = Journal::new();
    for batch in first.log().entries().iter().chain(second.log().entries()) {
        merged.append(batch.clone());
    }
    GraphLog::replay(merged)
}

/// Merge two replicas that diverged from `base`: the shared prefix once,
/// then each side's own tail.
///
/// Concatenating two full journals instead would replay the shared history
/// **twice**, and the second copy re-applies inserts the first side had
/// already removed. A partition shares a common ancestor; only the tails
/// are concurrent.
fn merge_divergent(
    base: &GraphLog<Container, Relation>,
    first: &GraphLog<Container, Relation>,
    second: &GraphLog<Container, Relation>,
) -> GraphLog<Container, Relation> {
    let shared = base.log().entries().len();
    let mut merged = Journal::new();
    for batch in base
        .log()
        .entries()
        .iter()
        .chain(first.log().entries().iter().skip(shared))
        .chain(second.log().entries().iter().skip(shared))
    {
        merged.append(batch.clone());
    }
    GraphLog::replay(merged)
}

/// M2 of the commons multi-writer convergence plan: **remove wins over a
/// concurrent connect**, and it does so in *either* merge order, so the
/// outcome does not depend on which writer's key sorts first.
///
/// Order matters to the mechanism but not the result. Remove-then-connect
/// drops the connect, because `Connect` only lands when both endpoints are
/// present. Connect-then-remove lands the edge and then reaps it, because
/// `RemoveNode` takes incident edges with it. The two paths converge.
#[test]
fn remove_wins_over_a_concurrent_connect_in_either_order() {
    let alice = Author::new("alice");
    let bob = Author::new("bob");

    // Shared history both replicas already had, then they partition.
    let mut base = GraphLog::<Container, Relation>::new();
    base.insert_node(&alice, Container::new("a"));
    base.insert_node(&alice, Container::new("b"));

    let mut remover = GraphLog::replay(base.log().clone()).for_writer(writer(0xa1));
    remover.remove_node(&alice, &"a".to_string());

    let mut connector = GraphLog::replay(base.log().clone()).for_writer(writer(0xb2));
    connector
        .connect(&bob, &"a".to_string(), &"b".to_string(), cites())
        .expect("bob connects before he learns of the removal");

    for (label, merged) in [
        (
            "remove then connect",
            merge_divergent(&base, &remover, &connector),
        ),
        (
            "connect then remove",
            merge_divergent(&base, &connector, &remover),
        ),
    ] {
        assert!(
            merged.graph().key_of(&"a".to_string()).is_none(),
            "{label}: the removed node stays removed"
        );
        assert_eq!(
            merged.graph().edge_count(),
            0,
            "{label}: the concurrent edge does not survive the removal"
        );
    }
}

/// M2: a concurrent edit to one node is **whole-node last-writer-wins**
/// under the merge order, and the loser's payload is discarded entirely
/// rather than merged field-by-field.
///
/// This pins the coarseness, not just the convergence. Per-facet merge is
/// the one-node facets lane's work; until a facet write is its own edit,
/// concurrent edits to unrelated parts of a node still cost one of them.
#[test]
fn concurrent_node_edits_are_whole_node_last_writer_wins() {
    let alice = Author::new("alice");
    let bob = Author::new("bob");

    let mut a = GraphLog::<Container, Relation>::new().for_writer(writer(0xa1));
    a.insert_node(&alice, Container::new("n").with_title("alice's title"));

    let mut b = GraphLog::<Container, Relation>::new().for_writer(writer(0xb2));
    b.insert_node(&bob, Container::new("n").with_tag("bob-only-tag"));

    let merged = merge(&a, &b);
    let node = merged
        .graph()
        .node(merged.graph().key_of(&"n".to_string()).expect("node n"))
        .expect("node payload");

    assert!(
        node.tags.iter().any(|t| t == "bob-only-tag"),
        "the last writer's payload survives"
    );
    assert_eq!(
        node.title, "",
        "the earlier writer's title is DISCARDED, not merged: whole-node LWW"
    );
}

#[test]
fn concurrent_edits_to_different_facets_both_survive() {
    let alice = Author::new("alice");
    let bob = Author::new("bob");
    let node = "n".to_string();
    let title = FacetId::new("content.title");
    let pinned = FacetId::new("arrangement.pin");

    let mut base = GraphLog::<Container, Relation>::new();
    base.insert_node(&alice, Container::new(node.clone()));
    let mut a = GraphLog::replay(base.log().clone()).for_writer(writer(0xa1));
    let mut b = GraphLog::replay(base.log().clone()).for_writer(writer(0xb2));
    assert!(a.set_facet(&alice, &node, title.clone(), json!("Alice")));
    assert!(b.set_facet(&bob, &node, pinned.clone(), json!(true)));

    for merged in [
        merge_divergent(&base, &a, &b),
        merge_divergent(&base, &b, &a),
    ] {
        assert_eq!(merged.facets().get(&node, &title), Some(&json!("Alice")));
        assert_eq!(merged.facets().get(&node, &pinned), Some(&json!(true)));
    }
}

#[test]
fn node_removal_suppresses_a_concurrent_facet_edit_in_either_order() {
    let alice = Author::new("alice");
    let bob = Author::new("bob");
    let node = "n".to_string();
    let title = FacetId::new("content.title");

    let mut base = GraphLog::<Container, Relation>::new();
    base.insert_node(&alice, Container::new(node.clone()));
    let mut remover = GraphLog::replay(base.log().clone()).for_writer(writer(0xa1));
    remover.remove_node(&alice, &node);
    let mut editor = GraphLog::replay(base.log().clone()).for_writer(writer(0xb2));
    assert!(editor.set_facet(&bob, &node, title.clone(), json!("gone")));

    for merged in [
        merge_divergent(&base, &remover, &editor),
        merge_divergent(&base, &editor, &remover),
    ] {
        assert!(merged.graph().key_of(&node).is_none());
        assert!(merged.facets().get(&node, &title).is_none());
    }
}

/// M2, the case the plan's "remove wins" phrasing does **not** cover, and
/// which is recorded as open rather than silently decided.
///
/// Remove-versus-*connect* is commutative (above). Remove-versus-*insert*
/// is not: `InsertNode` is an upsert, so whichever of the two sorts later
/// wins, and a concurrent insert that sorts after a removal **resurrects
/// the node**. Both peers agree, so this converges; it is the *choice*
/// that is unmade, because with no causality recorded a concurrent insert
/// is indistinguishable from a deliberate re-creation after the delete.
#[test]
fn a_concurrent_insert_that_sorts_later_resurrects_a_removed_node() {
    let alice = Author::new("alice");
    let bob = Author::new("bob");

    let mut remover = GraphLog::<Container, Relation>::new().for_writer(writer(0xa1));
    remover.insert_node(&alice, Container::new("a"));
    remover.remove_node(&alice, &"a".to_string());

    let mut editor = GraphLog::<Container, Relation>::new().for_writer(writer(0xb2));
    editor.insert_node(&bob, Container::new("a").with_title("still editing"));

    assert!(
        merge(&remover, &editor)
            .graph()
            .key_of(&"a".to_string())
            .is_some(),
        "insert sorted last: the node is back (current behavior, decision open)"
    );
    assert!(
        merge(&editor, &remover)
            .graph()
            .key_of(&"a".to_string())
            .is_none(),
        "remove sorted last: the node is gone"
    );
}

/// A 0.1.x journal wrote `EdgeId` as a bare integer. Those stores still
/// load, as the single-writer graphs they always were.
#[test]
fn a_legacy_bare_counter_deserializes_as_a_local_id() {
    let legacy: EdgeId = serde_json::from_str("7").expect("bare counter still reads");
    assert_eq!(legacy, EdgeId::local(7));
    assert!(legacy.writer.is_local());

    let current: EdgeId =
        serde_json::from_str(&serde_json::to_string(&EdgeId::new(writer(0xa1), 3)).unwrap())
            .expect("current form round-trips");
    assert_eq!(current, EdgeId::new(writer(0xa1), 3));
}
