// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! A clone shares every node's facets; a write copies only the node it
//! touches, leaves the original as it was, and is all `unshared_nodes` lists.

use super::*;
use serde_json::json;

fn three() -> FacetStore<String> {
    let mut store = FacetStore::new();
    for node in ["a", "b", "c"] {
        store
            .set(node.into(), FacetId::new("x"), json!(node), &AcceptAll)
            .unwrap();
    }
    store
}

fn ids(list: Vec<&String>) -> Vec<&str> {
    list.into_iter().map(String::as_str).collect()
}

#[test]
fn a_clone_shares_every_node_until_one_is_written() {
    let store = three();
    let mut copy = store.clone();
    assert!(copy.unshared_nodes(&store).is_empty(), "control: all shared");
    copy.set("b".into(), FacetId::new("y"), json!(1), &AcceptAll)
        .unwrap();
    assert_eq!(ids(copy.unshared_nodes(&store)), ["b"]);
    assert_eq!(ids(store.unshared_nodes(&copy)), ["b"]);
    assert_eq!(store, three(), "copy on write: the original is untouched");
    assert!(store.get(&"b".into(), &FacetId::new("y")).is_none());
}

#[test]
fn added_and_removed_nodes_are_unshared() {
    let store = three();
    let mut copy = store.clone();
    copy.remove(&"a".into(), &FacetId::new("x"));
    copy.set("d".into(), FacetId::new("x"), json!(4), &AcceptAll)
        .unwrap();
    assert_eq!(ids(copy.unshared_nodes(&store)), ["a", "d"]);
    copy.remove_node(&"c".into());
    assert_eq!(ids(store.unshared_nodes(&copy)), ["a", "c", "d"]);
}

#[test]
fn a_write_that_changes_nothing_may_unshare_but_stays_equal() {
    let store = three();
    let mut copy = store.clone();
    copy.set("a".into(), FacetId::new("x"), json!("a"), &AcceptAll)
        .unwrap();
    assert_eq!(ids(copy.unshared_nodes(&store)), ["a"]);
    assert_eq!(copy, store, "listed, yet equal: the caller compares");
    // Removing an absent facet, and an overlay of nothing, keep sharing.
    let mut kept = store.clone();
    assert_eq!(kept.remove(&"a".into(), &FacetId::new("absent")), None);
    kept.overlay(FacetStore::new());
    assert!(kept.unshared_nodes(&store).is_empty());
}

#[test]
fn an_overlay_unshares_only_the_nodes_it_names() {
    let store = three();
    let mut copy = store.clone();
    let mut sidecar = FacetStore::new();
    sidecar
        .set("c".into(), FacetId::new("x"), json!("new"), &AcceptAll)
        .unwrap();
    copy.overlay(sidecar);
    assert_eq!(ids(copy.unshared_nodes(&store)), ["c"]);
    assert_eq!(copy.get(&"c".into(), &FacetId::new("x")), Some(&json!("new")));
}
