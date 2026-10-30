// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Whole-keyspace admission test; connect when the bounded-open API exists.

use super::recovery_fixture::{inventory, limits, two_journals_with_repairable_tail};
use super::*;
use fjall::bounded_recovery::{RecoveryError, RecoveryLimitKind};

#[test]
fn bounded_open_refusal_preserves_earlier_repair_candidates_and_acknowledged_values() {
    let dir = two_journals_with_repairable_tail();
    let before = inventory(dir.path());
    let refused = Config::new(dir.path()).open_with_recovery_limits(limits(5));
    assert!(matches!(
        refused,
        Err(RecoveryError::Limit {
            kind: RecoveryLimitKind::ValueBytes,
            ..
        })
    ));
    assert_eq!(inventory(dir.path()), before);

    let keyspace = Config::new(dir.path())
        .open_with_recovery_limits(limits(32))
        .unwrap();
    let partition = keyspace
        .open_partition("eidetic", PartitionCreateOptions::default())
        .unwrap();
    assert_eq!(partition.get("small").unwrap().unwrap().as_ref(), b"first");
    assert_eq!(
        partition.get("large").unwrap().unwrap().as_ref(),
        b"a much larger value"
    );
}

#[test]
fn bounded_open_defers_orphan_cleanup_until_all_inputs_are_admitted() {
    let dir = two_journals_with_repairable_tail();
    let orphan = dir.path().join("partitions/orphan");
    std::fs::create_dir_all(orphan.join("nested")).unwrap();
    std::fs::write(orphan.join("nested/keep"), b"preexisting").unwrap();
    let before = inventory(dir.path());
    assert!(matches!(
        Config::new(dir.path()).open_with_recovery_limits(limits(5)),
        Err(RecoveryError::Limit {
            kind: RecoveryLimitKind::ValueBytes,
            ..
        })
    ));
    assert_eq!(inventory(dir.path()), before);
    let keyspace = Config::new(dir.path())
        .open_with_recovery_limits(limits(32))
        .unwrap();
    assert!(!orphan.exists());
    let partition = keyspace
        .open_partition("eidetic", PartitionCreateOptions::default())
        .unwrap();
    assert_eq!(
        partition.get("large").unwrap().unwrap().as_ref(),
        b"a much larger value"
    );
}

#[test]
fn bounded_open_creates_an_empty_owner_and_reopens_its_data() {
    let dir = tempfile::tempdir().unwrap();
    {
        let keyspace = Config::new(dir.path())
            .flush_workers(0)
            .compaction_workers(0)
            .open_with_recovery_limits(limits(32))
            .unwrap();
        let partition = keyspace
            .open_partition("eidetic", PartitionCreateOptions::default())
            .unwrap();
        partition.insert("new", "value").unwrap();
        keyspace.persist(fjall::PersistMode::SyncAll).unwrap();
    }
    let keyspace = Config::new(dir.path())
        .flush_workers(0)
        .compaction_workers(0)
        .open_with_recovery_limits(limits(32))
        .unwrap();
    let partition = keyspace
        .open_partition("eidetic", PartitionCreateOptions::default())
        .unwrap();
    assert_eq!(partition.get("new").unwrap().unwrap().as_ref(), b"value");
}

#[test]
fn bounded_open_rejects_nonempty_uninitialized_directory_without_mutation() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("retained"), b"do not overwrite").unwrap();
    let before = inventory(dir.path());
    assert!(matches!(
        Config::new(dir.path()).open_with_recovery_limits(limits(32)),
        Err(RecoveryError::Unsupported(_))
    ));
    let mut after = inventory(dir.path());
    // The only allowed newly-created coordination artifact.
    after.remove(".fjall-owner.lock");
    assert_eq!(after, before);
}

#[test]
fn bounded_open_metadata_exhaustion_after_journals_preserves_repair_candidate() {
    use fjall::bounded_recovery::{prepare_journal_set, RecoveryBudget};
    let dir = two_journals_with_repairable_tail();
    let before = inventory(dir.path());
    let mut measured = RecoveryBudget::new(limits(32));
    let journals = prepare_journal_set(dir.path(), &mut measured).unwrap();
    let after_journals = measured.metadata_bytes();
    drop(journals);
    let mut ceilings = limits(32);
    // Version-path setup fits; partition directory admission exceeds this.
    ceilings.max_metadata_bytes = after_journals + 2048;
    assert!(matches!(
        Config::new(dir.path()).open_with_recovery_limits(ceilings),
        Err(RecoveryError::Limit {
            kind: RecoveryLimitKind::MetadataBytes,
            ..
        })
    ));
    assert_eq!(inventory(dir.path()), before);
}

#[test]
fn bounded_open_rejects_any_blob_partition_before_mutation() {
    let dir = tempfile::tempdir().unwrap();
    {
        let keyspace = Config::new(dir.path())
            .flush_workers(0)
            .compaction_workers(0)
            .open()
            .unwrap();
        let plain = keyspace
            .open_partition("plain", PartitionCreateOptions::default())
            .unwrap();
        let blob = keyspace
            .open_partition(
                "blob",
                PartitionCreateOptions::default().with_kv_separation(Default::default()),
            )
            .unwrap();
        plain.insert("small", "value").unwrap();
        blob.insert("other", "bytes").unwrap();
        keyspace.persist(fjall::PersistMode::SyncAll).unwrap();
    }
    let before = inventory(dir.path());
    assert!(matches!(
        Config::new(dir.path()).open_with_recovery_limits(limits(32)),
        Err(RecoveryError::Unsupported(_))
    ));
    assert_eq!(inventory(dir.path()), before);
}

#[test]
fn bounded_open_recovers_partial_flush_sealed_and_active_then_flushes_again() {
    let dir = tempfile::tempdir().unwrap();
    {
        let keyspace = fjall::Keyspace::create_or_recover(
            Config::new(dir.path())
                .flush_workers(1)
                .compaction_workers(0),
        )
        .unwrap();
        let first = keyspace
            .open_partition("first", PartitionCreateOptions::default())
            .unwrap();
        let second = keyspace
            .open_partition("second", PartitionCreateOptions::default())
            .unwrap();
        first.insert("old", "disk-first").unwrap();
        second.insert("old", "journal-second").unwrap();
        assert!(first.rotate_memtable().unwrap());
        keyspace.force_flush().unwrap();
        assert!(first.flushes_completed() > 0);
        first.insert("middle", "sealed-first").unwrap();
        second.insert("middle", "sealed-second").unwrap();
        assert!(second.rotate_memtable().unwrap());
        first.insert("active", "active-first").unwrap();
        keyspace.persist(fjall::PersistMode::SyncAll).unwrap();
    }
    {
        let keyspace = Config::new(dir.path())
            .flush_workers(1)
            .compaction_workers(0)
            .open_with_recovery_limits(limits(64))
            .unwrap();
        let first = keyspace
            .open_partition("first", PartitionCreateOptions::default())
            .unwrap();
        let second = keyspace
            .open_partition("second", PartitionCreateOptions::default())
            .unwrap();
        assert_eq!(first.get("old").unwrap().unwrap().as_ref(), b"disk-first");
        assert_eq!(
            first.get("middle").unwrap().unwrap().as_ref(),
            b"sealed-first"
        );
        assert_eq!(
            first.get("active").unwrap().unwrap().as_ref(),
            b"active-first"
        );
        assert_eq!(
            second.get("old").unwrap().unwrap().as_ref(),
            b"journal-second"
        );
        assert_eq!(
            second.get("middle").unwrap().unwrap().as_ref(),
            b"sealed-second"
        );
        first.insert("later", "post-recovery").unwrap();
        assert!(first.rotate_memtable().unwrap());
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while (first.flushes_completed() < 2 || second.flushes_completed() < 2)
            && std::time::Instant::now() < deadline
        {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(
            first.flushes_completed() >= 2,
            "first partition's recovered flushes did not finish"
        );
        assert!(
            second.flushes_completed() >= 2,
            "second partition's recovered flushes did not finish"
        );
        keyspace.persist(fjall::PersistMode::SyncAll).unwrap();
    }
    let keyspace = Config::new(dir.path())
        .flush_workers(0)
        .compaction_workers(0)
        .open_with_recovery_limits(limits(64))
        .unwrap();
    let first = keyspace
        .open_partition("first", PartitionCreateOptions::default())
        .unwrap();
    let second = keyspace
        .open_partition("second", PartitionCreateOptions::default())
        .unwrap();
    assert_eq!(first.get("old").unwrap().unwrap().as_ref(), b"disk-first");
    assert_eq!(
        first.get("later").unwrap().unwrap().as_ref(),
        b"post-recovery"
    );
    assert_eq!(
        second.get("old").unwrap().unwrap().as_ref(),
        b"journal-second"
    );
    assert_eq!(
        second.get("middle").unwrap().unwrap().as_ref(),
        b"sealed-second"
    );
}

#[test]
fn bounded_open_accounts_recovered_sealed_write_buffer() {
    let dir = tempfile::tempdir().unwrap();
    let expected;
    {
        let keyspace = fjall::Keyspace::create_or_recover(
            Config::new(dir.path())
                .flush_workers(0)
                .compaction_workers(0),
        )
        .unwrap();
        let partition = keyspace
            .open_partition("data", PartitionCreateOptions::default())
            .unwrap();
        partition.insert("sealed", "value-before-rotation").unwrap();
        assert!(partition.rotate_memtable().unwrap());
        partition.insert("active", "value-after-rotation").unwrap();
        expected = keyspace.write_buffer_size();
        keyspace.persist(fjall::PersistMode::SyncAll).unwrap();
    }
    let keyspace = Config::new(dir.path())
        .flush_workers(0)
        .compaction_workers(0)
        .open_with_recovery_limits(limits(64))
        .unwrap();
    assert_eq!(keyspace.write_buffer_size(), expected);
}

#[test]
fn temporary_owner_survives_partition_snapshot_and_iterators() {
    for mode in 0..3 {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("store");
        if mode == 2 {
            drop(
                Config::new(&path)
                    .flush_workers(0)
                    .compaction_workers(0)
                    .open()
                    .unwrap(),
            );
        }
        let config = Config::new(&path)
            .temporary(true)
            .flush_workers(0)
            .compaction_workers(0);
        let keyspace = if mode != 0 {
            config.open_with_recovery_limits(limits(64)).unwrap()
        } else {
            config.open().unwrap()
        };
        #[cfg(unix)]
        let inode = {
            use std::os::unix::fs::MetadataExt;
            std::fs::metadata(path.join(".fjall-owner.lock"))
                .unwrap()
                .ino()
        };
        let partition = keyspace
            .open_partition("data", PartitionCreateOptions::default())
            .unwrap();
        partition.insert("key", "value").unwrap();
        let snapshot = partition.snapshot();
        let mut iter = partition.iter();
        let mut snapshot_iter = snapshot.iter();
        drop(keyspace);
        assert!(Config::new(&path).open().is_err());
        assert_eq!(partition.get("key").unwrap().unwrap().as_ref(), b"value");
        drop(partition);
        assert!(Config::new(&path).open().is_err());
        assert_eq!(snapshot.get("key").unwrap().unwrap().as_ref(), b"value");
        drop(snapshot);
        assert!(Config::new(&path).open().is_err());
        assert!(iter.next().unwrap().is_ok());
        drop(iter);
        assert!(Config::new(&path).open().is_err());
        assert!(snapshot_iter.next().unwrap().is_ok());
        drop(snapshot_iter);
        assert_eq!(std::fs::read_dir(&path).unwrap().count(), 1);
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            assert_eq!(
                std::fs::metadata(path.join(".fjall-owner.lock"))
                    .unwrap()
                    .ino(),
                inode
            );
        }
        assert!(Config::new(&path).open().is_ok());
    }
}

#[test]
fn temporary_transaction_iterator_retains_owner() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("store");
    let keyspace = Config::new(&path)
        .temporary(true)
        .open_transactional()
        .unwrap();
    let partition = keyspace
        .open_partition("data", PartitionCreateOptions::default())
        .unwrap();
    partition.insert("key", "value").unwrap();
    let tx = keyspace.read_tx();
    let mut iter = tx.iter(&partition);
    drop(tx);
    drop(partition);
    drop(keyspace);
    assert!(Config::new(&path).open().is_err());
    assert!(iter.next().unwrap().is_ok());
    drop(iter);
    assert_eq!(std::fs::read_dir(&path).unwrap().count(), 1);
    assert!(Config::new(&path).open().is_ok());
}

#[cfg(unix)]
#[test]
fn failed_temporary_open_keeps_unrelated_store_data() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("store");
    {
        let keyspace = Config::new(&path)
            .flush_workers(0)
            .compaction_workers(0)
            .open()
            .unwrap();
        let partition = keyspace
            .open_partition("data", PartitionCreateOptions::default())
            .unwrap();
        partition.insert("key", "value").unwrap();
        keyspace.persist(fjall::PersistMode::SyncAll).unwrap();
    }
    let orphan = path.join("partitions/orphan");
    std::fs::create_dir(&orphan).unwrap();
    std::fs::write(orphan.join("cannot-remove"), b"sentinel").unwrap();
    std::fs::set_permissions(&orphan, std::fs::Permissions::from_mode(0o500)).unwrap();
    let result = Config::new(&path)
        .temporary(true)
        .flush_workers(0)
        .compaction_workers(0)
        .open_with_recovery_limits(limits(64));
    if orphan.exists() {
        std::fs::set_permissions(&orphan, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    assert!(matches!(result, Err(RecoveryError::Io(_))));
    assert_eq!(
        std::fs::read(orphan.join("cannot-remove")).unwrap(),
        b"sentinel"
    );
    assert!(path.join("version").exists());
}
