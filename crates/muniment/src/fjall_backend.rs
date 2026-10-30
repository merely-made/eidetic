// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Fjall LSM [`Backend`], available to native Muniment consumers with `fjall`.
//!
//! Persists opaque byte keys in a single Fjall keyspace partition, opened at
//! a directory path the caller provides. The default partition remains
//! `eidetic`, preserving existing retained content across the ownership move.
//!
//! ## Async-but-blocking
//!
//! Fjall is sync-blocking. `FjallStore` implements the async [`Backend`]
//! trait by performing the blocking call inline and returning the
//! ready future — this matches the design-pass commitment that native
//! impls return ready futures (`?Send`-friendly, no real async work).
//! For workloads where blocking the executor is unacceptable, wrap the
//! call in `tokio::task::spawn_blocking` at the call site.

use crate::error::StoreError;
#[path = "fjall_bounded.rs"]
mod bounded;
use crate::{Backend, WriteOp};
use async_trait::async_trait;
use fjall::{Config, Keyspace, PartitionCreateOptions, PartitionHandle, PersistMode};
use std::path::Path;

#[cfg(test)]
#[path = "fjall_recovery_tests.rs"]
mod recovery_tests;
#[cfg(test)]
#[path = "fjall_recovery_fixture.rs"]
mod recovery_fixture;
#[cfg(test)]
#[path = "fjall_recovery_set_tests.rs"]
mod recovery_set_tests;

#[cfg(test)]
#[path = "fjall_recovery_open_tests.rs"]
mod recovery_open_tests;
#[cfg(test)]
#[path = "fjall_partition_bounds_tests.rs"]
mod partition_bounds_tests;
#[cfg(test)]
#[path = "fjall_bounds_tests.rs"]
mod bounds_tests;





/// Default partition name used by [`FjallStore::open`]. Callers that want
/// to host multiple eidetic stores in one fjall keyspace use
/// [`FjallStore::open_partition`] with distinct names.
pub const DEFAULT_PARTITION: &str = "eidetic";

/// Fjall-backed [`Backend`].
///
/// Holds a fjall [`Keyspace`] (the on-disk database) and a
/// [`PartitionHandle`] (the column-family-shaped sub-store within it).
/// Construct with [`FjallStore::open`] for the simple single-partition
/// case.
pub struct FjallStore {
    keyspace: Keyspace,
    partition: PartitionHandle,
    owner_limits: Option<crate::FjallOwnerLimits>,
}

impl FjallStore {
    /// Open (or create) a fjall keyspace at `path` and return a Store
    /// over the default partition.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        Self::open_partition(path, DEFAULT_PARTITION)
    }

    /// Open (or create) a keyspace at `path` and return a Store over the
    /// named partition. Useful when one fjall keyspace hosts multiple
    /// logical stores (e.g. one per identity).
    pub fn open_partition(path: impl AsRef<Path>, partition: &str) -> Result<Self, StoreError> {
        let keyspace = Config::new(path)
            .open()
            .map_err(|e| StoreError::Backend(format!("fjall open: {e}")))?;
        let partition = keyspace
            .open_partition(partition, PartitionCreateOptions::default())
            .map_err(|e| StoreError::Backend(format!("fjall partition: {e}")))?;
        Ok(Self {
            keyspace,
            partition,
            owner_limits: None,
        })
    }

    /// Flush and synchronize the keyspace's journal, including prior writes.
    ///
    /// This blocks on storage I/O and returns any persistence error to the caller.
    pub fn sync_all(&self) -> Result<(), StoreError> {
        self.keyspace
            .persist(PersistMode::SyncAll)
            .map_err(|e| StoreError::Backend(format!("fjall sync: {e}")))
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl Backend for FjallStore {
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, StoreError> {
        if let Some(owner) = &self.owner_limits {
            return self.get_bounded(key, &crate::FjallReadLimits::unlimited(), &mut crate::FjallReadBudget::new(owner.read.clone())).map_err(Into::into);
        }
        match self.partition.get(key) {
            Ok(Some(slice)) => Ok(Some(slice.to_vec())),
            Ok(None) => Ok(None),
            Err(e) => Err(StoreError::Backend(format!("fjall get {key}: {e}"))),
        }
    }

    async fn put(&self, key: &str, bytes: &[u8]) -> Result<(), StoreError> {
        self.admit_write(key, bytes)?;
        self.partition
            .insert(key, bytes)
            .map_err(|e| StoreError::Backend(format!("fjall insert {key}: {e}")))?;
        Ok(())
    }

    async fn list(&self, prefix: &str) -> Result<Vec<String>, StoreError> {
        if let Some(owner) = &self.owner_limits {
            return self.list_bounded(prefix, &crate::FjallReadLimits::unlimited(), &mut crate::FjallReadBudget::new(owner.read.clone())).map_err(Into::into);
        }
        let mut keys = Vec::new();
        for entry in self.partition.prefix(prefix) {
            let (key, _value) =
                entry.map_err(|e| StoreError::Backend(format!("fjall prefix scan: {e}")))?;
            let key_str = std::str::from_utf8(&key)
                .map_err(|e| StoreError::Backend(format!("fjall non-utf8 key: {e}")))?;
            keys.push(key_str.to_string());
        }
        Ok(keys)
    }

    /// Idempotent, per muniment's contract: an absent key is not an error.
    /// (Eidetic's old `delete_blob` returned "was it present?"; the one caller
    /// that needs that — `delete_manifest`, for the age-out eviction count —
    /// probes with `get` first.)
    async fn delete(&self, key: &str) -> Result<(), StoreError> {
        self.admit_key(key)?;
        self.partition
            .remove(key)
            .map_err(|e| StoreError::Backend(format!("fjall remove {key}: {e}")))?;
        Ok(())
    }

    /// Ordered range read, `[start, end)`. Fjall's LSM is key-ordered, so this
    /// is a native range scan — the ordered read a journal needs to replay
    /// a per-author range in `seq` order.
    async fn scan(&self, start: &str, end: &str) -> Result<Vec<String>, StoreError> {
        if let Some(owner) = &self.owner_limits {
            return self.scan_bounded(start, end, &crate::FjallReadLimits::unlimited(), &mut crate::FjallReadBudget::new(owner.read.clone())).map_err(Into::into);
        }
        let mut keys = Vec::new();
        for entry in self.partition.range(start.to_string()..end.to_string()) {
            let (key, _value) =
                entry.map_err(|e| StoreError::Backend(format!("fjall range scan: {e}")))?;
            let key_str = std::str::from_utf8(&key)
                .map_err(|e| StoreError::Backend(format!("fjall non-utf8 key: {e}")))?;
            keys.push(key_str.to_string());
        }
        Ok(keys)
    }

    /// Fjall has no cross-key transaction on a single partition, so the batch
    /// is applied in order. Muniment's contract allows this for backends
    /// without transactions: the writes it batches are content-addressed or
    /// idempotent, so a crash mid-batch leaves recoverable orphans, never a
    /// torn read of a mutated key.
    async fn apply(&self, ops: &[WriteOp]) -> Result<(), StoreError> {
        for op in ops {
            match op {
                WriteOp::Put { key, value } => self.admit_write(key, value)?,
                WriteOp::Delete { key } => self.admit_key(key)?,
            }
        }
        for op in ops {
            match op {
                WriteOp::Put { key, value } => {
                    self.partition
                        .insert(key, value.as_slice())
                        .map_err(|e| StoreError::Backend(format!("fjall batch put {key}: {e}")))?;
                },
                WriteOp::Delete { key } => {
                    self.partition.remove(key).map_err(|e| {
                        StoreError::Backend(format!("fjall batch delete {key}: {e}"))
                    })?;
                },
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn open_store() -> (FjallStore, TempDir) {
        let dir = TempDir::new().expect("tempdir");
        let store = FjallStore::open(dir.path()).expect("fjall open");
        (store, dir)
    }

    #[test]
    fn concurrent_keyspace_open_refuses_until_owner_drops() {
        let dir = TempDir::new().unwrap();
        let first = Config::new(dir.path()).open().unwrap();
        let partition = first.open_partition("held", PartitionCreateOptions::default()).unwrap();
        assert!(Config::new(dir.path()).open().is_err());
        drop(first);
        assert!(Config::new(dir.path()).open().is_err());
        drop(partition);
        assert!(Config::new(dir.path()).open().is_ok());
    }

    #[test]
    fn iterator_retains_keyspace_lease_after_partition_drops() {
        let dir = TempDir::new().unwrap();
        let keyspace = Config::new(dir.path()).open().unwrap();
        let partition = keyspace.open_partition("held", PartitionCreateOptions::default()).unwrap();
        partition.insert("key", "value").unwrap();
        let mut iter = partition.iter();
        drop(partition);
        drop(keyspace);
        assert!(Config::new(dir.path()).open().is_err());
        assert!(iter.next().unwrap().is_ok());
        drop(iter);
        assert!(Config::new(dir.path()).open().is_ok());
    }

    #[test]
    fn snapshot_retains_keyspace_lease_after_partition_drops() {
        let dir = TempDir::new().unwrap();
        let keyspace = Config::new(dir.path()).open().unwrap();
        let partition = keyspace.open_partition("held", PartitionCreateOptions::default()).unwrap();
        partition.insert("key", "value").unwrap();
        let snapshot = partition.snapshot();
        drop(partition);
        drop(keyspace);
        assert!(Config::new(dir.path()).open().is_err());
        assert_eq!(snapshot.get("key").unwrap().unwrap().as_ref(), b"value");
        drop(snapshot);
        assert!(Config::new(dir.path()).open().is_ok());
    }

    #[test]
    fn snapshot_iterator_retains_lease_after_snapshot_drops() {
        let dir = TempDir::new().unwrap();
        let keyspace = Config::new(dir.path()).open().unwrap();
        let partition = keyspace.open_partition("held", PartitionCreateOptions::default()).unwrap();
        partition.insert("key", "value").unwrap();
        let snapshot = partition.snapshot();
        let mut iter = snapshot.iter();
        drop(snapshot);
        drop(partition);
        drop(keyspace);
        assert!(Config::new(dir.path()).open().is_err());
        assert!(iter.next().unwrap().is_ok());
        drop(iter);
        assert!(Config::new(dir.path()).open().is_ok());
    }

    #[test]
    fn transaction_iterator_retains_lease_after_owner_drops() {
        let dir = TempDir::new().unwrap();
        let keyspace = Config::new(dir.path()).open_transactional().unwrap();
        let partition = keyspace.open_partition("held", PartitionCreateOptions::default()).unwrap();
        partition.insert("key", "value").unwrap();
        let tx = keyspace.read_tx();
        let mut iter = tx.iter(&partition);
        drop(tx);
        drop(partition);
        drop(keyspace);
        assert!(Config::new(dir.path()).open().is_err());
        assert!(iter.next().unwrap().is_ok());
        drop(iter);
        assert!(Config::new(dir.path()).open().is_ok());
    }

    #[test]
    fn owner_lock_refuses_second_process_until_drop() {
        const CHILD_DIR: &str = "MUNIMENT_FJALL_LOCK_TEST_DIR";
        const CHILD_EXPECT: &str = "MUNIMENT_FJALL_LOCK_TEST_EXPECT_OPEN";
        if let Some(path) = std::env::var_os(CHILD_DIR) {
            let opened = Config::new(path).open().is_ok();
            let expected = std::env::var(CHILD_EXPECT).unwrap() == "yes";
            std::process::exit(i32::from(opened != expected));
        }
        let dir = TempDir::new().unwrap();
        let owner = Config::new(dir.path()).open().unwrap();
        let child = |expected: &str| std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "fjall_backend::tests::owner_lock_refuses_second_process_until_drop"])
            .env(CHILD_DIR, dir.path()).env(CHILD_EXPECT, expected).status().unwrap();
        assert!(child("no").success());
        drop(owner);
        assert!(child("yes").success());
    }

    #[test]
    fn journal_preparation_refuses_oversize_without_changing_file() {
        use fjall::bounded_recovery::{prepare_journal, RecoveryBudget, RecoveryError, RecoveryLimitKind, RecoveryLimits};
        let dir = TempDir::new().unwrap();
        {
            let store = FjallStore::open(dir.path()).unwrap();
            pollster::block_on(async {
                store.put("a", b"a").await.unwrap();
                store.put("b", b"larger").await.unwrap();
                store.put("c", b"c").await.unwrap();
            });
            store.sync_all().unwrap();
        }
        let paths: Vec<_> = std::fs::read_dir(dir.path().join("journals"))
            .unwrap().map(|entry| entry.unwrap().path()).collect();
        assert_eq!(paths.len(), 1);
        let before = std::fs::read(&paths[0]).unwrap();
        let limits = RecoveryLimits { max_value_bytes: 3, max_key_bytes: 128,
            max_replay_bytes: 4096, max_replay_items: 100, max_metadata_bytes: 4096,
            max_input_bytes: 134_217_728, max_alloc_bytes: 65_536 };
        let err = prepare_journal(&paths[0], &mut RecoveryBudget::new(limits.clone()));
        assert!(matches!(err, Err(RecoveryError::Limit { kind: RecoveryLimitKind::ValueBytes, .. })));
        assert_eq!(std::fs::read(&paths[0]).unwrap(), before);
        let larger = RecoveryLimits { max_value_bytes: 6, ..limits };
        let admitted = prepare_journal(&paths[0], &mut RecoveryBudget::new(larger)).unwrap();
        assert_eq!(admitted.item_count(), 3);
        assert_eq!(std::fs::read(&paths[0]).unwrap(), before);
    }

    #[test]
    fn save_then_load_returns_same_bytes() {
        pollster::block_on(async {
            let (mut store, _dir) = open_store();
            store.put("k", b"hello").await.unwrap();
            let got = store.get("k").await.unwrap();
            assert_eq!(got, Some(b"hello".to_vec()));
        });
    }

    #[test]
    fn load_missing_returns_none() {
        pollster::block_on(async {
            let (mut store, _dir) = open_store();
            let got = store.get("never-saved").await.unwrap();
            assert!(got.is_none());
        });
    }

    #[test]
    fn save_overwrites_existing() {
        pollster::block_on(async {
            let (mut store, _dir) = open_store();
            store.put("k", b"first").await.unwrap();
            store.put("k", b"second").await.unwrap();
            let got = store.get("k").await.unwrap();
            assert_eq!(got, Some(b"second".to_vec()));
        });
    }

    #[test]
    fn data_persists_across_store_reopen() {
        pollster::block_on(async {
            let dir = TempDir::new().expect("tempdir");
            {
                let mut store = FjallStore::open(dir.path()).expect("open 1");
                store.put("persistent", b"survive").await.unwrap();
            }
            {
                let mut store = FjallStore::open(dir.path()).expect("open 2");
                let got = store.get("persistent").await.unwrap();
                assert_eq!(got, Some(b"survive".to_vec()));
            }
        });
    }

    #[test]
    fn sync_all_preserves_written_bytes_after_process_exit() {
        const CHILD_DIR: &str = "MUNIMENT_FJALL_SYNC_TEST_DIR";
        if let Some(path) = std::env::var_os(CHILD_DIR) {
            let store = FjallStore::open(path).expect("open child store");
            pollster::block_on(store.put("receipt", b"pending")).expect("write receipt");
            store.sync_all().expect("sync receipt");
            // Skip Fjall's drop-time sync so the parent reopens after the explicit sync.
            std::process::exit(0);
        }

        let dir = TempDir::new().expect("tempdir");
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "fjall_backend::tests::sync_all_preserves_written_bytes_after_process_exit",
            ])
            .env(CHILD_DIR, dir.path())
            .status()
            .expect("run child");
        assert!(status.success(), "child process failed: {status}");

        let store = FjallStore::open(dir.path()).expect("reopen store");
        assert_eq!(
            pollster::block_on(store.get("receipt")).unwrap(),
            Some(b"pending".to_vec())
        );
    }

    #[test]
    fn default_partition_preserves_existing_eidetic_keys() {
        pollster::block_on(async {
            let dir = TempDir::new().expect("tempdir");
            {
                let legacy = FjallStore::open_partition(dir.path(), "eidetic").unwrap();
                legacy.put("manifest:existing", b"retained").await.unwrap();
            }
            let migrated = FjallStore::open(dir.path()).unwrap();
            assert_eq!(
                migrated.get("manifest:existing").await.unwrap(),
                Some(b"retained".to_vec())
            );
        });
    }

    #[test]
    fn iter_keys_returns_prefix_matches() {
        pollster::block_on(async {
            let (mut store, _dir) = open_store();
            store.put("manifest:a", b"1").await.unwrap();
            store.put("manifest:b", b"2").await.unwrap();
            store.put("blob:c", b"3").await.unwrap();

            let mut manifests = store.list("manifest:").await.unwrap();
            manifests.sort();
            assert_eq!(manifests, vec!["manifest:a", "manifest:b"]);

            let blobs = store.list("blob:").await.unwrap();
            assert_eq!(blobs, vec!["blob:c"]);

            let nothing = store.list("missing:").await.unwrap();
            assert!(nothing.is_empty());
        });
    }

    #[test]
    fn separate_partitions_isolate_keys() {
        pollster::block_on(async {
            let dir = TempDir::new().expect("tempdir");
            let alice = FjallStore::open_partition(dir.path(), "alice").unwrap();
            let keyspace = alice.keyspace.clone();
            let partition = keyspace
                .open_partition("bob", PartitionCreateOptions::default())
                .unwrap();
            let bob = FjallStore { keyspace, partition, owner_limits: None };

            alice.put("k", b"alice-value").await.unwrap();
            bob.put("k", b"bob-value").await.unwrap();

            assert_eq!(alice.get("k").await.unwrap(), Some(b"alice-value".to_vec()));
            assert_eq!(bob.get("k").await.unwrap(), Some(b"bob-value".to_vec()));
        });
    }
}
