// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;

fn store() -> (tempfile::TempDir, DirectoryBackend) {
    let dir = tempfile::tempdir().unwrap();
    let backend = DirectoryBackend::open(dir.path().join("store")).unwrap();
    (dir, backend)
}

fn put(key: &str, value: &str) -> WriteOp {
    WriteOp::Put {
        key: key.to_string(),
        value: value.as_bytes().to_vec(),
    }
}

fn delete(key: &str) -> WriteOp {
    WriteOp::Delete {
        key: key.to_string(),
    }
}

fn entry(log: &str, seq: u64) -> String {
    format!("{log}/{seq:016x}")
}

#[test]
fn a_key_is_a_file_holding_its_value() {
    pollster::block_on(async {
        let (_dir, store) = store();
        store
            .put("sessions/a/graph.json", b"{\"n\":1}")
            .await
            .unwrap();
        let on_disk = store.root().join("sessions").join("a").join("graph.json");
        assert_eq!(fs::read(&on_disk).unwrap(), b"{\"n\":1}");
        assert_eq!(
            store.get("sessions/a/graph.json").await.unwrap(),
            Some(b"{\"n\":1}".to_vec())
        );
        assert_eq!(store.get("sessions/a/facets.json").await.unwrap(), None);
        // A directory holds keys; it is not one.
        assert_eq!(store.get("sessions/a").await.unwrap(), None);

        store.delete("sessions/a/graph.json").await.unwrap();
        assert_eq!(store.get("sessions/a/graph.json").await.unwrap(), None);
        assert!(
            !store.root().join("sessions").exists(),
            "empty directories are pruned"
        );
        store.delete("sessions/a/graph.json").await.unwrap();
    });
}

#[test]
fn a_log_is_one_file_with_a_line_per_entry() {
    pollster::block_on(async {
        let (_dir, store) = store();
        let log = "s/journal.jsonl";
        store.put(&entry(log, 0), b"{\"a\":0}").await.unwrap();
        store
            .apply(&[
                put(&entry(log, 1), "{\"a\":1}"),
                put(&entry(log, 2), "{\"a\":2}"),
            ])
            .await
            .unwrap();
        let path = store.root().join("s").join("journal.jsonl");
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "{\"a\":0}\n{\"a\":1}\n{\"a\":2}\n"
        );
        assert_eq!(
            store.get(&entry(log, 1)).await.unwrap(),
            Some(b"{\"a\":1}".to_vec())
        );
        assert_eq!(store.get(&entry(log, 3)).await.unwrap(), None);
        assert_eq!(
            store
                .scan("s/journal.jsonl/", "s/journal.jsonl0")
                .await
                .unwrap(),
            [entry(log, 0), entry(log, 1), entry(log, 2)]
        );
    });
}

#[test]
fn a_log_stays_dense() {
    pollster::block_on(async {
        let (_dir, store) = store();
        let log = "journal.jsonl";
        for seq in 0..3 {
            store.put(&entry(log, seq), b"x").await.unwrap();
        }
        let gap = store.put(&entry(log, 4), b"x").await.unwrap_err();
        assert!(gap.to_string().contains("dense"), "{gap}");
        let middle = store.delete(&entry(log, 1)).await.unwrap_err();
        assert!(middle.to_string().contains("dense"), "{middle}");

        // Rewriting an entry in place and trimming the tail are fine.
        store.put(&entry(log, 1), b"y").await.unwrap();
        store.delete(&entry(log, 2)).await.unwrap();
        assert_eq!(
            fs::read_to_string(store.root().join(log)).unwrap(),
            "x\ny\n"
        );

        // Deleting every entry removes the file.
        store
            .apply(&[delete(&entry(log, 0)), delete(&entry(log, 1))])
            .await
            .unwrap();
        assert!(!store.root().join(log).exists());
    });
}

#[test]
fn keys_must_be_portable_file_names() {
    pollster::block_on(async {
        let (_dir, store) = store();
        for key in [
            "",
            "../escape",
            "a//b",
            "a/./b",
            "con",
            "aux.json",
            "com1",
            "a:b",
            "a\\b",
            "trailing.",
            ".muniment-lock",
            "plain.jsonl",
            "a.jsonl/nothex",
            "a.jsonl/0000000000000000/deeper",
        ] {
            assert!(
                store.put(key, b"v").await.is_err(),
                "{key:?} should be refused"
            );
        }
        assert!(
            store
                .put("a.jsonl/0000000000000000", b"two\nlines")
                .await
                .is_err()
        );
    });
}

#[test]
fn a_key_cannot_also_be_a_directory() {
    pollster::block_on(async {
        let (_dir, store) = store();
        store.put("a", b"file").await.unwrap();
        assert!(store.put("a/b", b"inside").await.is_err());
        assert!(
            store
                .apply(&[put("x", "file"), put("x/y", "inside")])
                .await
                .is_err()
        );
        assert_eq!(
            store.get("x").await.unwrap(),
            None,
            "a refused batch writes nothing"
        );
    });
}

#[test]
fn list_and_scan_see_files_and_log_entries() {
    pollster::block_on(async {
        let (_dir, store) = store();
        store
            .apply(&[
                put("sessions/a/graph.json", "g"),
                put("sessions/a/journal.jsonl/0000000000000000", "0"),
                put("sessions/a/journal.jsonl/0000000000000001", "1"),
                put("sessions/b/graph.json", "g"),
                put("top", "t"),
            ])
            .await
            .unwrap();
        let mut listed = store.list("sessions/a/").await.unwrap();
        listed.sort();
        assert_eq!(
            listed,
            [
                "sessions/a/graph.json",
                "sessions/a/journal.jsonl/0000000000000000",
                "sessions/a/journal.jsonl/0000000000000001",
            ]
        );
        assert_eq!(store.list("").await.unwrap().len(), 5);
        assert_eq!(
            store.scan("sessions/a/", "sessions/b/").await.unwrap(),
            listed
        );
        assert!(store.list("../").await.unwrap().is_empty());
    });
}

#[test]
fn a_second_owner_is_refused_until_the_first_lets_go() {
    let dir = tempfile::tempdir().unwrap();
    let first = DirectoryBackend::open(dir.path()).unwrap();
    let refused = DirectoryBackend::open(dir.path())
        .err()
        .expect("the store is held");
    assert!(refused.to_string().contains("already open"), "{refused}");
    drop(first);
    DirectoryBackend::open(dir.path()).unwrap();
}

#[test]
fn a_batch_a_crash_cut_short_is_finished_on_open() {
    pollster::block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let log = "s/journal.jsonl";
        {
            let store = DirectoryBackend::open(dir.path()).unwrap();
            store.put(&entry(log, 0), b"0").await.unwrap();
        }
        // The crash: the batch was recorded, and only its first file written.
        let batch = [
            put("s/graph.json", "checkpoint"),
            put(&entry(log, 1), "1"),
            put(&entry(log, 2), "2"),
        ];
        fs::write(dir.path().join(REDO_FILE), encode_redo(&batch)).unwrap();
        fs::write(dir.path().join("s").join("graph.json"), "checkpoint").unwrap();
        // ...and entry 1 torn mid-append.
        let mut journal = OpenOptions::new()
            .append(true)
            .open(dir.path().join("s").join("journal.jsonl"))
            .unwrap();
        journal.write_all(b"1").unwrap();
        drop(journal);

        let store = DirectoryBackend::open(dir.path()).unwrap();
        assert!(!dir.path().join(REDO_FILE).exists());
        assert_eq!(
            store.get("s/graph.json").await.unwrap(),
            Some(b"checkpoint".to_vec())
        );
        assert_eq!(
            fs::read_to_string(dir.path().join("s").join("journal.jsonl")).unwrap(),
            "0\n1\n2\n"
        );
    });
}

#[test]
fn a_batch_staged_but_never_recorded_is_dropped() {
    pollster::block_on(async {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join(REDO_STAGING_FILE),
            encode_redo(&[put("a", "1")]),
        )
        .unwrap();
        let store = DirectoryBackend::open(dir.path()).unwrap();
        assert_eq!(store.get("a").await.unwrap(), None);
        assert!(!dir.path().join(REDO_STAGING_FILE).exists());
    });
}

#[test]
fn a_damaged_redo_file_refuses_the_open() {
    let dir = tempfile::tempdir().unwrap();
    let mut bytes = encode_redo(&[put("a", "1"), put("b", "2")]);
    bytes[REDO_MAGIC.len() + 9] ^= 0xff;
    fs::write(dir.path().join(REDO_FILE), bytes).unwrap();
    let refused = DirectoryBackend::open(dir.path())
        .err()
        .expect("damage is refused");
    assert!(refused.to_string().contains("damaged"), "{refused}");
    assert!(!dir.path().join("a").exists());
}

#[test]
fn only_single_step_batches_skip_the_redo_file() {
    let log = PathBuf::from("j.jsonl");
    let effects = |files: usize, logs: Vec<LogEffect>| Effects {
        files: (0..files)
            .map(|n| (PathBuf::from(format!("f{n}")), Some(Vec::new())))
            .collect(),
        logs: logs
            .into_iter()
            .map(|effect| (log.clone(), effect))
            .collect(),
    };
    assert!(!effects(1, vec![]).needs_redo());
    assert!(!effects(0, vec![LogEffect::Append(vec![b"a".to_vec()])]).needs_redo());
    assert!(!effects(1, vec![LogEffect::Unchanged]).needs_redo());
    assert!(!effects(0, vec![LogEffect::Truncate(1)]).needs_redo());
    // A crash can stop a multi-line append after some of its lines.
    assert!(
        effects(
            0,
            vec![LogEffect::Append(vec![b"a".to_vec(), b"b".to_vec()])]
        )
        .needs_redo()
    );
    assert!(effects(1, vec![LogEffect::Append(vec![b"a".to_vec()])]).needs_redo());
    assert!(effects(2, vec![]).needs_redo());
}

#[test]
fn a_multi_line_append_cut_short_is_finished_on_open() {
    pollster::block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let batch = [
            put(&entry("j.jsonl", 0), "0"),
            put(&entry("j.jsonl", 1), "1"),
        ];
        fs::write(dir.path().join(REDO_FILE), encode_redo(&batch)).unwrap();
        // Only the first line reached the disk.
        fs::write(dir.path().join("j.jsonl"), "0\n").unwrap();

        let store = DirectoryBackend::open(dir.path()).unwrap();
        assert_eq!(
            fs::read_to_string(dir.path().join("j.jsonl")).unwrap(),
            "0\n1\n"
        );
        assert_eq!(store.scan("j.jsonl/", "j.jsonl0").await.unwrap().len(), 2);
    });
}

#[test]
fn the_redo_record_round_trips() {
    let ops = [
        put("a/b.json", "value"),
        delete("c"),
        put("d.jsonl/0000000000000000", ""),
    ];
    assert_eq!(decode_redo(&encode_redo(&ops)).unwrap(), ops);
}

#[test]
fn a_torn_line_is_dropped_when_the_log_is_read() {
    pollster::block_on(async {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("j.jsonl"), "0\n1\n2").unwrap();
        let store = DirectoryBackend::open(dir.path()).unwrap();
        assert_eq!(store.scan("j.jsonl/", "j.jsonl0").await.unwrap().len(), 2);
        store.put(&entry("j.jsonl", 2), b"two").await.unwrap();
        assert_eq!(
            fs::read_to_string(dir.path().join("j.jsonl")).unwrap(),
            "0\n1\ntwo\n"
        );
    });
}

#[cfg(feature = "json")]
#[test]
fn a_journal_kept_under_a_jsonl_prefix_is_one_line_per_entry() {
    use crate::{Journal, JsonSlots, Seq};
    pollster::block_on(async {
        let (_dir, store) = store();
        let slots = JsonSlots::new(store.clone());
        let mut log: Journal<String> = Journal::new();
        log.append("first".into());
        log.append_entries(&slots, "s/journal.jsonl", Seq(0))
            .await
            .unwrap();
        let saved = log.next_seq();
        log.append_caused_by([Seq(0)], "second".into()).unwrap();
        log.append_entries(&slots, "s/journal.jsonl", saved)
            .await
            .unwrap();

        let text = fs::read_to_string(store.root().join("s").join("journal.jsonl")).unwrap();
        assert_eq!(
            text,
            "{\"entry\":\"first\",\"causes\":[]}\n{\"entry\":\"second\",\"causes\":[0]}\n"
        );
        let back: Journal<String> = Journal::load_entries(&slots, "s/journal.jsonl", None, None)
            .await
            .unwrap();
        assert_eq!(back, log);
    });
}
