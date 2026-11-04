// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;
use crate::container::{Container, Relation};
use crate::edit::WriterId;
use crate::facet::FacetId;
use crate::taxonomy::{Recognized, RelationClass};
use muniment::{JsonSlots, MemoryBackend};
use serde_json::json;

fn cites() -> Relation {
    Relation::new(RelationClass::recognized(Recognized::Cites))
}

fn gate() -> Author {
    Author::new("servitor.trail-keeper")
}

fn seeded() -> GraphLog<Container, Relation> {
    let ui = Author::new("ui");
    let mut log = GraphLog::new();
    log.insert_node(&ui, Container::new("a"));
    log.insert_node(&ui, Container::new("b"));
    log
}

fn writer(tag: u8) -> WriterId {
    WriterId([tag; 32])
}

#[test]
fn stale_revision_is_refused_with_the_current_revision() {
    let mut log = seeded();
    let read = log.revision();
    log.insert_node(&Author::new("ui"), Container::new("c")); // concurrent edit

    let err = log
        .commit_batch(gate(), read, vec![EditSpec::RemoveNode("a".to_string())])
        .unwrap_err();
    assert_eq!(
        err,
        CommitError::RevisionConflict { current: read + 1 },
        "the refusal carries the revision to rebase against"
    );
    assert!(
        log.graph().key_of(&"a".to_string()).is_some(),
        "nothing applied"
    );
}

#[test]
fn a_batch_applies_atomically_or_not_at_all() {
    let mut log = seeded();
    let before = (log.graph().node_count(), log.revision());

    // The second spec references an unknown node: the whole batch refuses.
    let err = log
        .commit_batch(
            gate(),
            log.revision(),
            vec![
                EditSpec::InsertNode(Container::new("c")),
                EditSpec::Connect {
                    from: "c".to_string(),
                    to: "ghost".to_string(),
                    edge: cites(),
                },
            ],
        )
        .unwrap_err();
    assert_eq!(err, CommitError::UnknownNode("ghost".to_string()));
    assert_eq!(
        (log.graph().node_count(), log.revision()),
        before,
        "the insert earlier in the refused batch did not apply either"
    );
}

#[test]
fn assertion_keeps_source_metadata_and_withdrawal_is_terminal() {
    let mut log = seeded();
    let batch_author = Author::new("local-importer");
    let source_author = Author::new("remote-source");
    let basis = AssertionBasis {
        note: Some("copied from source record".into()),
        method: Some(crate::AssertionMethod {
            iri: "https://example.test/method/import".into(),
            version: Some("1".into()),
        }),
        evidence: vec!["urn:evidence:one".into(), "urn:evidence:two".into()],
    };
    let id = log
        .assert_relation(
            batch_author.clone(),
            "a".to_string(),
            "b".to_string(),
            cites(),
            Some(source_author.clone()),
            Some(10),
            basis.clone(),
        )
        .unwrap();
    assert_eq!(log.assertion(id).unwrap().asserter, Some(source_author));
    assert_eq!(log.assertion(id).unwrap().span.first_asserted_ms, Some(10));
    assert_eq!(log.assertion(id).unwrap().basis, basis);
    assert_eq!(log.log().entries().last().unwrap().author, batch_author);
    let independent_id = log
        .assert_relation(
            Author::new("another-importer"),
            "a".to_string(),
            "b".to_string(),
            cites(),
            Some(Author::new("another-source")),
            Some(11),
            AssertionBasis {
                note: Some("independent reason".into()),
                ..Default::default()
            },
        )
        .unwrap();
    assert_ne!(id, independent_id);
    assert_eq!(
        log.graph().edge_count(),
        2,
        "parallel reasons remain separate"
    );

    assert!(log.confirm_assertion(&Author::new("reviewer"), id, Some(20)));
    assert_eq!(log.assertion(id).unwrap().span.last_asserted_ms, Some(20));
    assert!(log.withdraw_assertion(&Author::new("moderator"), id, Some(30)));
    assert_eq!(
        log.graph().edge_count(),
        1,
        "the independent assertion remains held"
    );
    let record = log.assertion(id).unwrap();
    assert_eq!(record.span.withdrawn.as_ref().unwrap().at_ms, Some(30));
    assert_eq!(
        record.span.withdrawn.as_ref().unwrap().by.as_str(),
        "moderator"
    );
    assert!(!log.confirm_assertion(&Author::new("reviewer"), id, Some(40)));
    assert_eq!(log.assertion(id).unwrap().span.last_asserted_ms, Some(20));
}

#[test]
fn invalid_late_assertion_spec_refuses_the_whole_batch_and_does_not_mint() {
    let mut log = seeded();
    let before_revision = log.revision();
    let result = log.commit_batch(
        gate(),
        before_revision,
        vec![
            EditSpec::Assert {
                from: "a".into(),
                to: "b".into(),
                edge: cites(),
                asserter: Some(Author::new("source")),
                at_ms: Some(1),
                basis: AssertionBasis::default(),
            },
            EditSpec::ConfirmAssertion {
                id: EdgeId::local(99),
                at_ms: Some(2),
            },
        ],
    );
    assert_eq!(
        result.unwrap_err(),
        CommitError::UnknownAssertion(EdgeId::local(99))
    );
    assert_eq!(log.revision(), before_revision);
    assert_eq!(log.graph().edge_count(), 0);
    let first = log
        .assert_relation(
            gate(),
            "a".into(),
            "b".into(),
            cites(),
            None,
            None,
            AssertionBasis::default(),
        )
        .unwrap();
    assert_eq!(first, EdgeId::local(0), "refused batches consume no IDs");
}

#[test]
fn node_removal_invalidates_incident_operations_before_anything_commits() {
    for endpoint in ["a", "b"] {
        for reinsert in [false, true] {
            for operation in 0..3 {
                let mut log = seeded();
                let id = log
                    .connect(&gate(), &"a".into(), &"b".into(), cites())
                    .unwrap();
                let revision = log.revision();
                let mut specs = vec![
                    EditSpec::Connect {
                        from: "a".into(),
                        to: "b".into(),
                        edge: cites(),
                    },
                    EditSpec::RemoveNode(endpoint.into()),
                ];
                if reinsert {
                    specs.push(EditSpec::InsertNode(Container::new(endpoint)));
                }
                specs.push(match operation {
                    0 => EditSpec::Disconnect(id),
                    1 => EditSpec::ConfirmAssertion {
                        id,
                        at_ms: Some(10),
                    },
                    _ => EditSpec::WithdrawAssertion {
                        id,
                        at_ms: Some(10),
                    },
                });
                let expected = if operation == 0 {
                    CommitError::UnknownEdge(id)
                } else {
                    CommitError::AssertionAlreadyWithdrawn(id)
                };
                assert_eq!(log.commit_batch(gate(), revision, specs), Err(expected));
                assert_eq!(log.revision(), revision, "refusal appends nothing");
                assert_eq!(log.graph().node_count(), 2);
                assert_eq!(log.graph().edge_count(), 1);
                assert!(log.assertion(id).unwrap().is_held());
                assert_eq!(log.assertion(id).unwrap().span.last_asserted_ms, None);
                let next = log
                    .connect(&gate(), &"a".into(), &"b".into(), cites())
                    .unwrap();
                assert_eq!(next.counter, id.counter + 1, "refusal consumes no IDs");
            }
        }
    }
}

#[test]
fn reinserted_node_can_receive_a_new_assertion() {
    let mut log = seeded();
    let old = log
        .connect(&gate(), &"a".into(), &"b".into(), cites())
        .unwrap();
    let committed = log
        .commit_batch(
            gate(),
            log.revision(),
            vec![
                EditSpec::RemoveNode("a".into()),
                EditSpec::InsertNode(Container::new("a")),
                EditSpec::Connect {
                    from: "a".into(),
                    to: "b".into(),
                    edge: cites(),
                },
            ],
        )
        .unwrap();
    assert!(!log.assertion(old).unwrap().is_held());
    assert!(log.assertion(committed.edges[0]).unwrap().is_held());
    assert_eq!(log.graph().edge_count(), 1);
}

#[test]
fn a_committed_batch_reads_back_attributed_with_minted_edges() {
    let mut log = seeded();
    let committed = log
        .commit_batch(
            gate(),
            log.revision(),
            vec![
                EditSpec::InsertNode(Container::new("c")),
                EditSpec::Connect {
                    from: "a".to_string(),
                    to: "c".to_string(),
                    edge: cites(),
                },
            ],
        )
        .unwrap();

    assert_eq!(committed.edges.len(), 1, "one connect, one minted id");
    assert!(log.edge_key(committed.edges[0]).is_some());
    let entry = &log.log().entries()[committed.batch.0 as usize];
    assert_eq!(entry.author, gate(), "the journal knows who committed");
    assert_eq!(entry.edits.len(), 2, "the batch is one journal entry");
}

#[test]
fn batch_local_inserts_are_connectable_and_removable() {
    let mut log = seeded();
    let committed = log.commit_batch(
        gate(),
        log.revision(),
        vec![
            EditSpec::InsertNode(Container::new("c")),
            EditSpec::Connect {
                from: "c".to_string(),
                to: "b".to_string(),
                edge: cites(),
            },
            EditSpec::RemoveNode("c".to_string()),
        ],
    );
    assert!(committed.is_ok(), "batch-local presence tracking allows it");
    assert!(log.graph().key_of(&"c".to_string()).is_none());
    assert_eq!(log.graph().edge_count(), 0, "the reap took the edge");
}

#[test]
fn effects_enqueue_only_after_a_commit_lands() {
    // The consumer discipline in miniature: effects carry the batch id and
    // never enqueue on refusal.
    let mut log = seeded();
    let mut effects: Vec<(BatchId, &str)> = Vec::new();

    if let Ok(committed) =
        log.commit_batch(gate(), 999, vec![EditSpec::RemoveNode("a".to_string())])
    {
        effects.push((committed.batch, "navigate"));
    }
    assert!(effects.is_empty(), "a refused petition spawns no effects");

    let committed = log
        .commit_batch(
            gate(),
            log.revision(),
            vec![EditSpec::RemoveNode("a".to_string())],
        )
        .unwrap();
    effects.push((committed.batch, "navigate"));
    assert_eq!(effects, vec![(committed.batch, "navigate")]);
}

#[test]
fn a_pre_gate_edit_log_migrates_on_load() {
    pollster::block_on(async {
        let slots = JsonSlots::new(MemoryBackend::new());

        // A legacy log: bare GraphEdit entries, as B0-era code wrote them.
        let mut legacy: Journal<GraphEdit<Container, Relation>> =
            Journal::with_id(LogId::new("old"));
        legacy.append(GraphEdit::InsertNode(Container::new("a")));
        legacy.append(GraphEdit::InsertNode(Container::new("b")));
        legacy.append(GraphEdit::Connect {
            id: EdgeId::local(0),
            from: "a".to_string(),
            to: "b".to_string(),
            edge: cites(),
        });
        legacy.append(GraphEdit::Disconnect(EdgeId::local(0)));
        legacy.save(&slots, "log").await.unwrap();

        let mut loaded = GraphLog::<Container, Relation>::load_full(&slots, "log")
            .await
            .unwrap();
        assert_eq!(loaded.graph().node_count(), 2);
        assert_eq!(loaded.graph().edge_count(), 0);
        assert_eq!(loaded.id(), Some(&LogId::new("old")), "identity carried");
        assert_eq!(loaded.revision(), 4, "one batch per legacy edit");
        let assertion = loaded.assertion(EdgeId::local(0)).unwrap().clone();
        assert_eq!(assertion.asserter, Some(Author::pre_gate()));
        assert_eq!(assertion.span.first_asserted_ms, None);
        assert_eq!(assertion.span.withdrawn.as_ref().unwrap().at_ms, None);
        assert_eq!(
            assertion.span.withdrawn.as_ref().unwrap().by,
            Author::pre_gate()
        );
        let revision = loaded.revision();
        assert!(!loaded.confirm_assertion(&gate(), EdgeId::local(0), Some(55)));
        assert_eq!(
            loaded.revision(),
            revision,
            "stale confirmations do not append"
        );
        assert!(
            loaded
                .log()
                .entries()
                .iter()
                .all(|b| b.author == Author::pre_gate()),
            "migrated entries carry the synthetic pre-gate author"
        );

        // And the next save writes the batch format.
        loaded.save_log(&slots, "log").await.unwrap();
        let reloaded = GraphLog::<Container, Relation>::load_full(&slots, "log")
            .await
            .unwrap();
        assert_eq!(reloaded.revision(), 4);
        assert_eq!(reloaded.assertion(EdgeId::local(0)), Some(&assertion));
    });
}

#[path = "tests/concurrency.rs"]
mod concurrency;
