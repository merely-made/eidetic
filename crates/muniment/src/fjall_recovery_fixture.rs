// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
use fjall::bounded_recovery::RecoveryLimits;
use std::{collections::BTreeMap, io::Read, path::Path};
use tempfile::TempDir;

pub(super) fn limits(max_value_bytes: usize) -> RecoveryLimits {
    RecoveryLimits { max_value_bytes, max_key_bytes: 1024, max_replay_bytes: 65_536,
        max_replay_items: 1000, max_metadata_bytes: 16_777_216,
        max_input_bytes: 134_217_728, max_alloc_bytes: 33_554_432 }
}

pub(super) fn inventory(root: &Path) -> BTreeMap<String, Option<(u64, blake3::Hash)>> {
    fn visit(root: &Path, path: &Path, output: &mut BTreeMap<String, Option<(u64, blake3::Hash)>>) {
        for entry in std::fs::read_dir(path).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            let name = path.strip_prefix(root).unwrap().to_string_lossy().into_owned();
            if entry.file_type().unwrap().is_dir() {
                output.insert(name, None);
                visit(root, &path, output);
            } else {
                let mut file = std::fs::File::open(&path).unwrap();
                let length = file.metadata().unwrap().len();
                let mut hasher = blake3::Hasher::new();
                let mut chunk = [0_u8; 8192];
                loop {
                    let count = file.read(&mut chunk).unwrap();
                    if count == 0 { break; }
                    hasher.update(&chunk[..count]);
                }
                output.insert(name, Some((length, hasher.finalize())));
            }
        }
    }
    let mut result = BTreeMap::new();
    visit(root, root, &mut result);
    result
}

pub(super) fn two_journals_with_repairable_tail() -> TempDir {
    let dir = TempDir::new().unwrap();
    {
        let keyspace = Config::new(dir.path()).flush_workers(0).compaction_workers(0).open().unwrap();
        let partition = keyspace.open_partition("eidetic", PartitionCreateOptions::default()).unwrap();
        partition.insert("small", "first").unwrap();
        keyspace.persist(fjall::PersistMode::SyncAll).unwrap();
        assert!(partition.rotate_memtable().unwrap());
        partition.insert("large", "a much larger value").unwrap();
        keyspace.persist(fjall::PersistMode::SyncAll).unwrap();
    }
    let mut journals: Vec<_> = std::fs::read_dir(dir.path().join("journals")).unwrap()
        .map(|entry| entry.unwrap().path()).collect();
    journals.sort();
    assert_eq!(journals.len(), 2);
    // Start marker 15 bytes; item 26 bytes; end marker 13 bytes.
    let mut sealed = std::fs::read(&journals[0]).unwrap();
    sealed.truncate(54);
    // A real interrupted Start marker at physical EOF, after a complete batch.
    sealed.push(1);
    std::fs::write(&journals[0], sealed).unwrap();
    dir
}
