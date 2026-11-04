// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[test]
fn a_snapshot_of_another_graph_is_replayed_past_in_favour_of_the_durable_journal() {
    pollster::block_on(async {
        let slots = JsonSlots::new(MemoryBackend::new());
        let mut durable = GraphLog::<Container, Relation>::new();
        durable.insert_node(&me(), Container::new("durable"));
        durable.save_log(&slots, "log").await.unwrap();
        // The same length as the durable log, from a different history.
        let mut foreign = GraphLog::<Container, Relation>::new();
        foreign.insert_node(&me(), Container::new("foreign"));
        foreign.snapshot(&slots, "snap").await.unwrap();

        let loaded = GraphLog::<Container, Relation>::load_checkpointed(&slots, "log", "snap")
            .await
            .expect("a snapshot of another journal's prefix is skipped, not trusted");
        assert_eq!(names(&loaded), ["durable"]);
        assert_eq!(loaded.revision(), 1);
    });
}

#[test]
fn a_snapshot_of_a_rewritten_journal_prefix_is_replayed_past_and_the_next_one_names_the_new_prefix()
{
    pollster::block_on(async {
        let slots = JsonSlots::new(MemoryBackend::new());
        let mut before = GraphLog::<Container, Relation>::new();
        before.insert_node(&me(), Container::new("a"));
        before.insert_node(&me(), Container::new("b"));
        before.snapshot(&slots, "snap").await.unwrap();
        // The journal is later replaced by one of the same length whose
        // second entry differs.
        let mut after = GraphLog::<Container, Relation>::new();
        after.insert_node(&me(), Container::new("a"));
        after.insert_node(&me(), Container::new("c"));
        after.save_log(&slots, "log").await.unwrap();

        let loaded = GraphLog::<Container, Relation>::load_checkpointed(&slots, "log", "snap")
            .await
            .unwrap();
        assert_eq!(names(&loaded), ["a", "c"]);
        loaded.snapshot(&slots, "snap").await.unwrap();
        let again = GraphLog::<Container, Relation>::load_checkpointed(&slots, "log", "snap")
            .await
            .unwrap();
        assert_eq!(names(&again), ["a", "c"]);
        let stored: Snapshot<Container, Relation> = slots.load("snap").await.unwrap().unwrap();
        assert_eq!(
            stored.journal_digest,
            Some(after.log().prefix_digest(2).unwrap().unwrap().to_hex())
        );
    });
}

#[test]
fn a_snapshot_past_the_end_of_the_journal_is_replayed_past() {
    pollster::block_on(async {
        let slots = JsonSlots::new(MemoryBackend::new());
        let mut long = GraphLog::<Container, Relation>::new();
        long.insert_node(&me(), Container::new("a"));
        long.insert_node(&me(), Container::new("b"));
        long.snapshot(&slots, "snap").await.unwrap();
        // Only the first entry is durable.
        let mut short = GraphLog::<Container, Relation>::new();
        short.insert_node(&me(), Container::new("a"));
        short.save_log(&slots, "log").await.unwrap();

        let loaded = GraphLog::<Container, Relation>::load_checkpointed(&slots, "log", "snap")
            .await
            .expect("a snapshot the journal has not reached is skipped");
        assert_eq!(names(&loaded), ["a"]);
    });
}

#[test]
fn a_snapshot_names_the_journal_prefix_it_reflects_and_still_loads_beside_a_longer_one() {
    pollster::block_on(async {
        let slots = JsonSlots::new(MemoryBackend::new());
        let mut live = GraphLog::<Container, Relation>::new();
        live.insert_node(&me(), Container::new("a"));
        live.insert_node(&me(), Container::new("b"));
        live.save_log(&slots, "log").await.unwrap();
        live.snapshot(&slots, "snap").await.unwrap();

        let stored: Snapshot<Container, Relation> = slots.load("snap").await.unwrap().unwrap();
        let prefix = live.log().prefix_digest(2).unwrap().unwrap();
        assert_eq!(stored.journal_digest, Some(prefix.to_hex()));

        live.insert_node(&me(), Container::new("c"));
        live.save_log(&slots, "log").await.unwrap();
        let loaded = GraphLog::<Container, Relation>::load_checkpointed(&slots, "log", "snap")
            .await
            .unwrap();
        assert_eq!(names(&loaded), ["a", "b", "c"]);
        assert_eq!(loaded.revision(), 3);
    });
}

#[test]
fn a_snapshot_without_a_journal_digest_is_replayed_in_full_and_the_next_one_names_its_prefix() {
    pollster::block_on(async {
        let slots = JsonSlots::new(MemoryBackend::new());
        let mut durable = GraphLog::<Container, Relation>::new();
        durable.insert_node(&me(), Container::new("durable"));
        durable.save_log(&slots, "log").await.unwrap();
        // A foreign snapshot with its digest taken off: it names no journal,
        // so nothing about it is trusted.
        let mut foreign = GraphLog::<Container, Relation>::new();
        foreign.insert_node(&me(), Container::new("foreign"));
        foreign.snapshot(&slots, "snap").await.unwrap();
        let mut raw: serde_json::Value = slots.load("snap").await.unwrap().unwrap();
        raw.as_object_mut().unwrap().remove("journal_digest");
        slots.save("snap", &raw).await.unwrap();

        let loaded = GraphLog::<Container, Relation>::load_checkpointed(&slots, "log", "snap")
            .await
            .expect("a digest-less snapshot is skipped, not refused");
        assert_eq!(names(&loaded), ["durable"]);

        loaded.snapshot(&slots, "snap").await.unwrap();
        let stored: Snapshot<Container, Relation> = slots.load("snap").await.unwrap().unwrap();
        assert!(
            stored.journal_digest.is_some(),
            "the new checkpoint names its prefix"
        );
        let again = GraphLog::<Container, Relation>::load_checkpointed(&slots, "log", "snap")
            .await
            .unwrap();
        assert_eq!(names(&again), ["durable"]);
    });
}

#[test]
fn a_checkpoint_of_nodes_with_many_tags_never_mismatches_after_a_reload() {
    pollster::block_on(async {
        let slots = JsonSlots::new(MemoryBackend::new());
        let mut live = GraphLog::<Container, Relation>::new();
        let mut node = Container::new("tagged");
        node.tags = (0..64).map(|n| format!("tag-{n:02}")).collect();
        live.insert_node(&me(), node);
        live.save_log(&slots, "log").await.unwrap();
        live.snapshot(&slots, "snap").await.unwrap();
        // Each load builds the tag set again, in its own hash order.
        for _ in 0..8 {
            let loaded = GraphLog::<Container, Relation>::load_checkpointed(&slots, "log", "snap")
                .await
                .expect("hash order is not part of the journal's content");
            assert_eq!(names(&loaded), ["tagged"]);
        }
    });
}

#[test]
fn checkpointed_load_equals_full_replay() {
    pollster::block_on(async {
        let slots = JsonSlots::new(MemoryBackend::new());

        let mut live = GraphLog::new();
        live.insert_node(&me(), Container::new("a").with_tag("keep"));
        live.insert_node(&me(), Container::new("b"));
        live.insert_node(&me(), Container::new("c"));
        let e_ab = live
            .connect(&me(), &"a".to_string(), &"b".to_string(), cites())
            .unwrap();
        let e_ac = live
            .connect(&me(), &"a".to_string(), &"c".to_string(), cites())
            .unwrap();

        // Snapshot mid-history.
        live.snapshot(&slots, "snap").await.unwrap();

        // Then more edits, including retracting a pre-snapshot edge and removing
        // a pre-snapshot node (which reaps another pre-snapshot edge).
        live.remove_node(&me(), &"c".to_string());
        live.insert_node(&me(), Container::new("d"));
        let e_ad = live
            .assert_relation(
                me(),
                "a".to_string(),
                "d".to_string(),
                cites(),
                Some(Author::new("source-agent")),
                Some(500),
                AssertionBasis {
                    note: Some("independent source note".into()),
                    method: None,
                    evidence: vec!["urn:evidence:checkpoint".into()],
                },
            )
            .unwrap();
        assert!(live.confirm_assertion(&Author::new("reviewer"), e_ad, Some(600)));
        live.disconnect(&me(), e_ab);

        live.save_log(&slots, "log").await.unwrap();

        let full = GraphLog::<Container, Relation>::load_full(&slots, "log")
            .await
            .unwrap();
        let checkpointed =
            GraphLog::<Container, Relation>::load_checkpointed(&slots, "log", "snap")
                .await
                .unwrap();

        let live_fp = fingerprint(live.graph());
        assert_eq!(
            fingerprint(full.graph()),
            live_fp,
            "full replay matches live"
        );
        assert_eq!(
            fingerprint(checkpointed.graph()),
            live_fp,
            "checkpoint + tail matches live"
        );
        // Final state: nodes a, b, d; only the a->d edge survives.
        assert_eq!(checkpointed.graph().node_count(), 3);
        assert_eq!(checkpointed.graph().edge_count(), 1);
        for id in [e_ab, e_ac] {
            assert_eq!(checkpointed.assertion(id), full.assertion(id));
            assert!(checkpointed.assertion(id).unwrap().span.withdrawn.is_some());
        }
        let explicit = checkpointed.assertion(e_ad).unwrap();
        assert_eq!(explicit.span.first_asserted_ms, Some(500));
        assert_eq!(explicit.span.last_asserted_ms, Some(600));
        assert_eq!(explicit.asserter.as_ref().unwrap().as_str(), "source-agent");
        assert_eq!(
            explicit.basis.note.as_deref(),
            Some("independent source note")
        );
        assert_eq!(explicit.basis.evidence, vec!["urn:evidence:checkpoint"]);
    });
}
