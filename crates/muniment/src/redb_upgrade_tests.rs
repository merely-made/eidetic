// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! v2 -> v3 upgrade tests for `RedbBackend::open`.

use std::path::{Path, PathBuf};

use ::redb::{Builder, Database, TableDefinition, UpgradeError};

use super::*;
use crate::redb_upgrade::{backup_path, file_format};

const T: TableDefinition<&str, &[u8]> = TableDefinition::new("muniment");

/// A v2 file (redb 2.6's default) holding muniment's table and a few keys.
fn v2_file(dir: &Path) -> PathBuf {
    let path = dir.join("store.redb");
    let db = Builder::new().create(&path).unwrap();
    let txn = db.begin_write().unwrap();
    {
        let mut t = txn.open_table(T).unwrap();
        for (k, v) in [("blob/a", "1"), ("blob/b", "2"), ("log/a/0", "x"), ("log/a/1", "y"), ("slot/s", "z")] {
            t.insert(k, v.as_bytes()).unwrap();
        }
    }
    txn.commit().unwrap();
    drop(db);
    assert_eq!(file_format(&path).unwrap(), Some(2));
    path
}

#[test]
fn v2_file_is_upgraded_once_with_data_intact() {
    pollster::block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let path = v2_file(dir.path());
        let (b, outcome) = RedbBackend::open_reporting(&path).unwrap();
        assert_eq!(outcome, RedbOpenOutcome::Upgraded { from_format: 2 });
        assert_eq!(b.get("slot/s").await.unwrap(), Some(b"z".to_vec()));
        let mut blobs = b.list("blob/").await.unwrap();
        blobs.sort();
        assert_eq!(blobs, ["blob/a", "blob/b"]);
        assert_eq!(b.scan("log/a/0", "log/a/2").await.unwrap(), ["log/a/0", "log/a/1"]);
        b.apply(&[
            WriteOp::Put { key: "n".into(), value: b"new".to_vec() },
            WriteOp::Delete { key: "blob/a".into() },
        ])
        .await
        .unwrap();
        b.transact(Box::new(|r| {
            assert_eq!(r.get("n").unwrap(), Some(b"new".to_vec()));
            vec![WriteOp::Put { key: "t".into(), value: b"tx".to_vec() }]
        }))
        .await
        .unwrap();
        drop(b);
        assert_eq!(file_format(&path).unwrap(), Some(3));
        assert!(!backup_path(&path).exists(), "backup removed after success");

        // Second open: nothing to upgrade, data still there.
        let (b, outcome) = RedbBackend::open_reporting(&path).unwrap();
        assert_eq!(outcome, RedbOpenOutcome::Current);
        assert_eq!(b.get("t").await.unwrap(), Some(b"tx".to_vec()));
        assert_eq!(b.get("blob/a").await.unwrap(), None);
    });
}

#[test]
fn failed_upgrade_restores_the_v2_file_and_errors() {
    pollster::block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let path = v2_file(dir.path());
        let fail = |_: &mut Database| Err(UpgradeError::TransactionInProgress);
        let err = RedbBackend::open_with_upgrade(&path, &fail).err().expect("must fail");
        assert!(matches!(err, StoreError::Unsupported(_)), "{err:?}");
        assert!(!backup_path(&path).exists(), "backup renamed back over the file");
        assert_eq!(file_format(&path).unwrap(), Some(2));
        // Still a readable v2 database, and a later open upgrades it.
        let (b, outcome) = RedbBackend::open_reporting(&path).unwrap();
        assert_eq!(outcome, RedbOpenOutcome::Upgraded { from_format: 2 });
        assert_eq!(b.get("blob/b").await.unwrap(), Some(b"2".to_vec()));
    });
}

#[test]
fn crash_after_backup_before_upgrade_is_retried() {
    pollster::block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let path = v2_file(dir.path());
        std::fs::copy(&path, backup_path(&path)).unwrap();
        let (b, outcome) = RedbBackend::open_reporting(&path).unwrap();
        assert_eq!(outcome, RedbOpenOutcome::Upgraded { from_format: 2 });
        assert_eq!(b.get("log/a/1").await.unwrap(), Some(b"y".to_vec()));
        assert!(!backup_path(&path).exists());
    });
}

#[test]
fn crash_after_upgrade_before_backup_removal_cleans_up() {
    pollster::block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let path = v2_file(dir.path());
        std::fs::copy(&path, backup_path(&path)).unwrap();
        drop(RedbBackend::open(&path).unwrap());
        std::fs::copy(&path, backup_path(&path)).unwrap(); // stale leftover beside a v3 file
        let (b, outcome) = RedbBackend::open_reporting(&path).unwrap();
        assert_eq!(outcome, RedbOpenOutcome::Current);
        assert_eq!(b.get("slot/s").await.unwrap(), Some(b"z".to_vec()));
        assert!(!backup_path(&path).exists());
    });
}

#[test]
fn unopenable_file_beside_a_backup_names_the_backup() {
    let dir = tempfile::tempdir().unwrap();
    let path = v2_file(dir.path());
    std::fs::copy(&path, backup_path(&path)).unwrap();
    std::fs::write(&path, b"garbage that is not a redb database at all").unwrap();
    let err = RedbBackend::open(&path).err().expect("must fail");
    assert!(matches!(&err, StoreError::Corruption(m) if m.contains(".pre-v3")), "{err:?}");
    assert!(backup_path(&path).exists(), "backup is kept for recovery");
}
