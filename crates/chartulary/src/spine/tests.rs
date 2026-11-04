// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;
use crate::container::{Container, Relation};
use crate::taxonomy::{Recognized, RelationClass};
use muniment::{JsonSlots, MemoryBackend};

type Fingerprint = (Vec<String>, Vec<(String, String, String)>);

/// A canonical, key-independent view of a graph: sorted node ids and sorted
/// (from, to, relation) edge triples. Two graphs equal iff their fingerprints
/// are equal, regardless of internal petgraph keys.
fn fingerprint(graph: &Graph<Container, Relation>) -> Fingerprint {
    let mut nodes: Vec<String> = graph.nodes().map(|(_, n)| n.id.clone()).collect();
    nodes.sort();
    let keys: Vec<NodeKey> = graph.nodes().map(|(k, _)| k).collect();
    let mut edges = Vec::new();
    for key in keys {
        let from = graph.node(key).unwrap().id.clone();
        for (_, target, edge) in graph.out_edges(key) {
            let to = graph.node(target).unwrap().id.clone();
            edges.push((from.clone(), to, format!("{:?}", edge.class)));
        }
    }
    edges.sort();
    (nodes, edges)
}

fn cites() -> Relation {
    Relation::new(RelationClass::recognized(Recognized::Cites))
}

fn me() -> Author {
    Author::new("test")
}

#[test]
fn replay_reconstructs_the_live_graph() {
    let mut live = GraphLog::new();
    live.insert_node(&me(), Container::new("a"));
    live.insert_node(&me(), Container::new("b"));
    let e = live
        .connect(&me(), &"a".to_string(), &"b".to_string(), cites())
        .unwrap();
    live.insert_node(&me(), Container::new("c"));
    live.connect(&me(), &"a".to_string(), &"c".to_string(), cites());
    live.disconnect(&me(), e);

    let replayed = GraphLog::replay(live.log().clone());
    assert_eq!(fingerprint(replayed.graph()), fingerprint(live.graph()));
}

#[test]
fn removing_a_node_reaps_its_edges_on_replay() {
    let mut live = GraphLog::new();
    live.insert_node(&me(), Container::new("a"));
    live.insert_node(&me(), Container::new("b"));
    live.connect(&me(), &"a".to_string(), &"b".to_string(), cites());
    live.remove_node(&me(), &"b".to_string());

    let replayed = GraphLog::replay(live.log().clone());
    assert_eq!(replayed.graph().node_count(), 1);
    assert_eq!(
        replayed.graph().edge_count(),
        0,
        "the edge went with the node"
    );
    assert_eq!(fingerprint(replayed.graph()), fingerprint(live.graph()));
    let assertion = live
        .assertion(EdgeId::local(0))
        .expect("edge is retained as history");
    assert_eq!(assertion.from, "a");
    assert_eq!(assertion.to, "b");
    assert_eq!(assertion.span.withdrawn.as_ref().unwrap().at_ms, None);
    assert_eq!(assertion.span.withdrawn.as_ref().unwrap().by, me());
    assert_eq!(replayed.assertion(EdgeId::local(0)), Some(assertion));
}

#[test]
fn stale_replay_with_duplicate_assertion_ids_cannot_revive_tombstones() {
    let author = me();
    let explicit_id = EdgeId::local(70);
    let legacy_id = EdgeId::local(71);
    let mut log = Journal::new();
    log.append(Batch {
        author: author.clone(),
        edits: vec![
            GraphEdit::InsertNode(Container::new("a")),
            GraphEdit::InsertNode(Container::new("b")),
            GraphEdit::Assert {
                id: explicit_id,
                from: "a".into(),
                to: "b".into(),
                edge: cites(),
                asserter: Some(Author::new("original-source")),
                at_ms: Some(10),
                basis: AssertionBasis::default(),
            },
            GraphEdit::WithdrawAssertion {
                id: explicit_id,
                at_ms: Some(20),
            },
            GraphEdit::Assert {
                id: explicit_id,
                from: "b".into(),
                to: "a".into(),
                edge: cites(),
                asserter: Some(Author::new("stale-source")),
                at_ms: Some(30),
                basis: AssertionBasis::default(),
            },
            GraphEdit::Connect {
                id: legacy_id,
                from: "a".into(),
                to: "b".into(),
                edge: cites(),
            },
            GraphEdit::Disconnect(legacy_id),
            GraphEdit::Connect {
                id: legacy_id,
                from: "b".into(),
                to: "a".into(),
                edge: cites(),
            },
        ],
    });

    let replayed = GraphLog::replay(log);
    assert_eq!(replayed.graph().edge_count(), 0);
    let explicit = replayed.assertion(explicit_id).unwrap();
    assert_eq!(explicit.from, "a");
    assert_eq!(explicit.to, "b");
    assert_eq!(
        explicit.asserter.as_ref().unwrap().as_str(),
        "original-source"
    );
    assert_eq!(explicit.span.withdrawn.as_ref().unwrap().at_ms, Some(20));
    let legacy = replayed.assertion(legacy_id).unwrap();
    assert_eq!(legacy.from, "a");
    assert_eq!(legacy.to, "b");
    assert!(legacy.span.withdrawn.is_some());
}

#[test]
fn legacy_snapshot_without_assertion_ledger_replays_history_from_journal() {
    pollster::block_on(async {
        let slots = JsonSlots::new(MemoryBackend::new());
        let mut live = GraphLog::new();
        live.insert_node(&me(), Container::new("a"));
        live.insert_node(&me(), Container::new("b"));
        let id = live
            .connect(&me(), &"a".into(), &"b".into(), cites())
            .unwrap();
        live.disconnect(&me(), id);
        live.save_log(&slots, "log").await.unwrap();

        // This is the old snapshot shape: it has only materialized nodes,
        // held edges, derivations, facets, writer/counter and sequence.
        let legacy = Snapshot::<Container, Relation> {
            nodes: vec![Container::new("a"), Container::new("b")],
            edges: vec![],
            derivations: vec![],
            facets: FacetStore::new(),
            writer: WriterId::LOCAL,
            next_edge: 1,
            assertions: None,
            at_seq: live.log().len() as u64,
            journal_digest: None,
        };
        let mut value = serde_json::to_value(legacy).unwrap();
        value.as_object_mut().unwrap().remove("assertions");
        slots.save("snap", &value).await.unwrap();

        let loaded = GraphLog::<Container, Relation>::load_checkpointed(&slots, "log", "snap")
            .await
            .unwrap();
        let record = loaded
            .assertion(id)
            .expect("full retained journal reconstructs it");
        assert_eq!(
            record.span.first_asserted_ms, None,
            "legacy time stays unknown"
        );
        let withdrawal = record.span.withdrawn.as_ref().unwrap();
        assert_eq!(
            withdrawal.at_ms, None,
            "legacy disconnect has no invented time"
        );
        assert_eq!(withdrawal.by, me());
        assert_eq!(loaded.graph().edge_count(), 0);
    });
}

#[test]
fn new_empty_assertion_ledger_is_encoded_distinctly_from_legacy_absence() {
    pollster::block_on(async {
        let slots = JsonSlots::new(MemoryBackend::new());
        let empty = GraphLog::<Container, Relation>::new();
        empty.save_log(&slots, "log").await.unwrap();
        empty.snapshot(&slots, "snap").await.unwrap();

        let raw: serde_json::Value = slots.load("snap").await.unwrap().unwrap();
        assert_eq!(raw.get("assertions"), Some(&serde_json::json!([])));
        let loaded = GraphLog::<Container, Relation>::load_checkpointed(&slots, "log", "snap")
            .await
            .unwrap();
        assert_eq!(loaded.assertions().count(), 0);
    });
}

#[test]
fn malformed_assertion_snapshots_are_refused_instead_of_loaded_partially() {
    pollster::block_on(async {
        let slots = JsonSlots::new(MemoryBackend::new());
        let mut live = GraphLog::new();
        live.insert_node(&me(), Container::new("a"));
        live.insert_node(&me(), Container::new("b"));
        live.connect(&me(), &"a".into(), &"b".into(), cites())
            .unwrap();
        live.save_log(&slots, "log").await.unwrap();
        live.snapshot(&slots, "valid").await.unwrap();
        let base: serde_json::Value = slots.load("valid").await.unwrap().unwrap();

        for malformed_kind in [
            "duplicate assertion id",
            "missing ledger record",
            "missing held endpoint",
            "mismatched endpoint tuple",
        ] {
            let mut malformed = base.clone();
            match malformed_kind {
                "duplicate assertion id" => {
                    let assertions = malformed["assertions"].as_array_mut().unwrap();
                    let duplicate = assertions[0].clone();
                    assertions.push(duplicate);
                }
                "missing ledger record" => malformed["assertions"].as_array_mut().unwrap().clear(),
                "missing held endpoint" => {
                    malformed["assertions"][0]["from"] = serde_json::json!("ghost")
                }
                "mismatched endpoint tuple" => {
                    malformed["assertions"][0]["from"] = serde_json::json!("b")
                }
                _ => unreachable!(),
            }
            slots.save("bad", &malformed).await.unwrap();
            let error = GraphLog::<Container, Relation>::load_checkpointed(&slots, "log", "bad")
                .await
                .err()
                .expect("malformed checkpoint must be refused");
            assert!(
                matches!(&error, StoreError::Codec(_)),
                "{malformed_kind}: unexpected error {error:?}"
            );
        }

        // The retained assertion ledger owns the payload. A divergent
        // compatibility tuple cannot override it or cause codec-dependent
        // false refusal (notably for unordered payloads).
        let mut divergent_compatibility_payload = base;
        divergent_compatibility_payload["edges"][0][3]["label"] =
            serde_json::json!("compatibility payload must be ignored");
        slots
            .save("divergent-payload", &divergent_compatibility_payload)
            .await
            .unwrap();
        let loaded =
            GraphLog::<Container, Relation>::load_checkpointed(&slots, "log", "divergent-payload")
                .await
                .expect("the authoritative ledger payload determines materialization");
        assert_eq!(loaded.assertions().next().unwrap().edge, cites());
    });
}

#[test]
fn checkpoint_round_trips_unordered_hash_map_edge_payloads() {
    pollster::block_on(async {
        let slots = JsonSlots::new(MemoryBackend::new());
        let mut live = GraphLog::<Container, HashMap<String, String>>::new();
        live.insert_node(&me(), Container::new("a"));
        live.insert_node(&me(), Container::new("b"));
        let payload: HashMap<_, _> = (0..256)
            .map(|index| (format!("key-{index:03}"), format!("value-{index:03}")))
            .collect();
        let id = live
            .connect(&me(), &"a".into(), &"b".into(), payload.clone())
            .unwrap();

        live.save_log(&slots, "hash-map-log").await.unwrap();
        live.snapshot(&slots, "hash-map-snapshot").await.unwrap();
        for _ in 0..16 {
            let loaded = GraphLog::<Container, HashMap<String, String>>::load_checkpointed(
                &slots,
                "hash-map-log",
                "hash-map-snapshot",
            )
            .await
            .expect("unordered payload serialization must not create false mismatch");
            assert_eq!(loaded.assertion(id).unwrap().edge, payload);
        }
    });
}

#[test]
fn checkpoint_refuses_counter_that_would_reuse_withdrawn_assertion_id() {
    pollster::block_on(async {
        let slots = JsonSlots::new(MemoryBackend::new());
        let mut live = GraphLog::new();
        live.insert_node(&me(), Container::new("a"));
        live.insert_node(&me(), Container::new("b"));
        let id = live
            .connect(&me(), &"a".into(), &"b".into(), cites())
            .unwrap();
        assert!(live.withdraw_assertion(&me(), id, Some(10)));
        live.save_log(&slots, "log").await.unwrap();
        live.snapshot(&slots, "valid").await.unwrap();

        let mut malformed: serde_json::Value = slots.load("valid").await.unwrap().unwrap();
        assert_eq!(malformed["assertions"][0]["span"]["withdrawn"]["at_ms"], 10);
        malformed["next_edge"] = serde_json::json!(id.counter);
        slots.save("bad", &malformed).await.unwrap();

        let error = GraphLog::<Container, Relation>::load_checkpointed(&slots, "log", "bad")
            .await
            .err()
            .expect("withdrawn IDs stay reserved by their writer");
        assert!(matches!(error, StoreError::Codec(_)), "{error:?}");
    });
}

fn names(graph: &GraphLog<Container, Relation>) -> Vec<String> {
    let mut names: Vec<String> = graph.graph().nodes().map(|(_, n)| n.id.clone()).collect();
    names.sort();
    names
}

#[test]
fn edge_ids_stay_stable_across_reload() {
    pollster::block_on(async {
        let slots = JsonSlots::new(MemoryBackend::new());
        let mut live = GraphLog::new();
        live.insert_node(&me(), Container::new("a"));
        live.insert_node(&me(), Container::new("b"));
        let e = live
            .connect(&me(), &"a".to_string(), &"b".to_string(), cites())
            .unwrap();
        live.save_log(&slots, "log").await.unwrap();

        // Reload, then retract the edge by the id assigned before the reload.
        let mut reloaded = GraphLog::<Container, Relation>::load_full(&slots, "log")
            .await
            .unwrap();
        assert!(reloaded.edge_key(e).is_some(), "the pre-reload id resolves");
        assert!(
            reloaded.disconnect(&me(), e),
            "retract by the stable id works"
        );
        assert_eq!(reloaded.graph().edge_count(), 0);
    });
}

#[test]
fn writer_scoped_counter_resumes_after_full_replay() {
    pollster::block_on(async {
        let slots = JsonSlots::new(MemoryBackend::new());
        let writer = WriterId([0xa1; 32]);
        let mut live = GraphLog::new().for_writer(writer);
        live.insert_node(&me(), Container::new("a"));
        live.insert_node(&me(), Container::new("b"));
        let first = live
            .connect(&me(), &"a".to_string(), &"b".to_string(), cites())
            .unwrap();
        live.disconnect(&me(), first);
        live.save_log(&slots, "log").await.unwrap();

        let mut reloaded =
            GraphLog::<Container, Relation>::load_full_for_writer(&slots, "log", writer)
                .await
                .unwrap();
        let second = reloaded
            .connect(&me(), &"a".to_string(), &"b".to_string(), cites())
            .unwrap();

        assert_eq!(first, EdgeId::new(writer, 0));
        assert_eq!(
            second,
            EdgeId::new(writer, 1),
            "a disconnected pre-restart id still consumed its counter"
        );
    });
}

#[test]
fn fork_carries_history_and_provenance_then_diverges() {
    let mut source = GraphLog::new();
    // The source needs an identity for the fork to point back at it.
    source = GraphLog::replay(source.log().clone().fork(LogId::new("source")));
    source.insert_node(&me(), Container::new("a"));
    source.insert_node(&me(), Container::new("b"));
    source.connect(&me(), &"a".to_string(), &"b".to_string(), cites());

    let mut fork = source.fork(LogId::new("fork"));
    // The fork is a self-contained copy.
    assert_eq!(fingerprint(fork.graph()), fingerprint(source.graph()));
    // And it knows where it came from.
    let provenance = fork.provenance().expect("a fork has provenance");
    assert_eq!(provenance.source, Some(LogId::new("source")));
    assert_eq!(provenance.at, Seq(source.log().len() as u64));

    // Diverging the fork leaves the source untouched.
    fork.insert_node(&me(), Container::new("c"));
    fork.connect(&me(), &"a".to_string(), &"c".to_string(), cites());
    assert_eq!(source.graph().node_count(), 2, "source unchanged");
    assert_eq!(fork.graph().node_count(), 3);
}

#[test]
fn derivation_records_survive_round_trip() {
    use crate::edit::DerivationKind;

    pollster::block_on(async {
        let slots = JsonSlots::new(MemoryBackend::new());

        let mut live: GraphLog<Container, Relation> = GraphLog::new();
        live.insert_node(&me(), Container::new("clip"));
        // The node was copied from node "orig" in another graph, "source".
        live.derive(
            &me(),
            &"clip".to_string(),
            DerivationRecord {
                source_log: LogId::new("source"),
                source_node: "orig".to_string(),
                kind: DerivationKind::CopiedFrom,
            },
        );
        live.save_log(&slots, "log").await.unwrap();

        let reloaded = GraphLog::<Container, Relation>::load_full(&slots, "log")
            .await
            .unwrap();
        let derivations = reloaded.derivations(&"clip".to_string());
        assert_eq!(derivations.len(), 1, "the derivation survived reload");
        assert_eq!(derivations[0].source_node, "orig");
        assert_eq!(derivations[0].kind, DerivationKind::CopiedFrom);
        // Natively-minted nodes have none.
        assert!(reloaded.derivations(&"absent".to_string()).is_empty());
    });
}

#[path = "tests/checkpoints.rs"]
mod checkpoints;
