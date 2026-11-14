// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! A fixed store's serde bytes, `Debug` text and equality, pinned so a change
//! to how the store holds its entries cannot change what it writes or reads.

use super::*;
use serde_json::json;

fn store(order: &[&str]) -> FacetStore<String> {
    let mut store = FacetStore::new();
    for node in order {
        for (facet, value) in [
            ("web.viewer", json!({ "mode": "reader", "zoom": 1.5 })),
            ("note.pin", json!([1, "two", null, { "b": false, "a": -0.0 }])),
        ] {
            store
                .set(node.to_string(), FacetId::new(facet), value, &AcceptAll)
                .unwrap();
        }
    }
    store
        .set("n2".into(), FacetId::new("gone"), json!(1), &AcceptAll)
        .unwrap();
    store.remove(&"n2".to_string(), &FacetId::new("gone"));
    store
}

const JSON: &str = r#"{"nodes":{"n1":{"note.pin":[1,"two",null,{"a":-0.0,"b":false}],"web.viewer":{"mode":"reader","zoom":1.5}},"n2":{"note.pin":[1,"two",null,{"a":-0.0,"b":false}],"web.viewer":{"mode":"reader","zoom":1.5}}}}"#;

#[test]
fn a_fixed_store_writes_reads_and_prints_the_pinned_vector() {
    let built = store(&["n1", "n2"]);
    assert_eq!(serde_json::to_string(&built).unwrap(), JSON);
    let read: FacetStore<String> = serde_json::from_str(JSON).unwrap();
    assert_eq!(read, built);
    assert_eq!(serde_json::to_string(&read).unwrap(), JSON);
    assert_eq!(
        format!("{built:?}"),
        r#"FacetStore { nodes: {"n1": NodeFacets { facets: {FacetId("note.pin"): Array [Number(1), String("two"), Null, Object {"a": Number(-0.0), "b": Bool(false)}], FacetId("web.viewer"): Object {"mode": String("reader"), "zoom": Number(1.5)}} }, "n2": NodeFacets { facets: {FacetId("note.pin"): Array [Number(1), String("two"), Null, Object {"a": Number(-0.0), "b": Bool(false)}], FacetId("web.viewer"): Object {"mode": String("reader"), "zoom": Number(1.5)}} }} }"#
    );
}

#[test]
fn equality_is_by_content_whatever_the_write_order() {
    assert_eq!(store(&["n1", "n2"]), store(&["n2", "n1"]));
    let mut other = store(&["n1", "n2"]);
    other
        .set("n2".into(), FacetId::new("note.pin"), json!(2), &AcceptAll)
        .unwrap();
    assert_ne!(other, store(&["n1", "n2"]), "control: a changed value differs");
    let mut emptied = store(&["n1", "n2"]);
    emptied.remove_node(&"n2".to_string());
    assert_ne!(emptied, store(&["n1", "n2"]), "control: a dropped node differs");
}

#[test]
fn an_empty_store_writes_an_empty_map() {
    let empty = FacetStore::<String>::new();
    assert_eq!(serde_json::to_string(&empty).unwrap(), r#"{"nodes":{}}"#);
    assert_eq!(
        serde_json::from_str::<FacetStore<String>>(r#"{"nodes":{}}"#).unwrap(),
        empty
    );
}
