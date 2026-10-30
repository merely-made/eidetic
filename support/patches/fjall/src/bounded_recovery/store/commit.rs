// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Prepare Fjall's in-memory managers before applying admitted actions.

use super::*;
use crate::{
    compaction::manager::CompactionManager,
    flush::manager::{FlushManager, Task},
    journal::{
        manager::{EvictionWatermark, Item, JournalManager},
        writer::PersistMode,
        Journal,
    },
    keyspace::{KeyspaceInner, Partitions},
    snapshot_tracker::SnapshotTracker,
    write_buffer_manager::WriteBufferManager,
    PartitionHandle,
};
use lsm_tree::SequenceNumberCounter;
use std::{
    fs::OpenOptions,
    sync::{atomic::Ordering, Arc, RwLock},
};
use std_semaphore::Semaphore;

fn alloc_error() -> RecoveryError {
    RecoveryError::Io(std::io::Error::new(
        std::io::ErrorKind::OutOfMemory,
        "recovery manager allocation failed",
    ))
}

#[cfg(test)]
thread_local! {
    // On the selected path copy, tighten the allocation limit one byte below
    // the request and expose its exact precharge to the regression test.
    static PATH_CHARGE: std::cell::Cell<(usize, u64, u64)> = const {
        std::cell::Cell::new((0, 0, 0))
    };
}

fn charge_handle_path(
    path: &std::path::Path,
    budget: &mut RecoveryBudget,
) -> Result<(), RecoveryError> {
    let bytes = path.as_os_str().as_encoded_bytes().len() as u64;
    #[cfg(test)]
    PATH_CHARGE.with(|target| {
        let (remaining, _, _) = target.get();
        if remaining == 1 {
            let before = budget.allocated_bytes();
            target.set((0, before, bytes));
            budget.tighten_allocation_limit(before + bytes - 1);
        } else if remaining > 1 {
            target.set((remaining - 1, 0, 0));
        }
    });
    budget.metadata(bytes)
}
fn capacity<T>(count: usize, budget: &mut RecoveryBudget) -> Result<(), RecoveryError> {
    let bytes = count
        .checked_mul(std::mem::size_of::<T>())
        .ok_or(RecoveryError::Limit {
            kind: Kind::MetadataBytes,
            limit: budget.limits.max_metadata_bytes,
        })?;
    budget.metadata(bytes as u64)
}
fn vector<T>(count: usize, budget: &mut RecoveryBudget) -> Result<Vec<T>, RecoveryError> {
    capacity::<T>(count, budget)?;
    let mut values = Vec::new();
    values.try_reserve_exact(count).map_err(|_| alloc_error())?;
    Ok(values)
}
fn arc<T>(count: usize, budget: &mut RecoveryBudget) -> Result<(), RecoveryError> {
    let each = std::mem::size_of::<T>()
        .checked_add(2 * std::mem::size_of::<usize>())
        .and_then(|n| n.checked_add(std::mem::align_of::<T>()))
        .ok_or(RecoveryError::Limit {
            kind: Kind::MetadataBytes,
            limit: budget.limits.max_metadata_bytes,
        })?;
    let bytes = count.checked_mul(each).ok_or(RecoveryError::Limit {
        kind: Kind::MetadataBytes,
        limit: budget.limits.max_metadata_bytes,
    })?;
    budget.metadata(bytes as u64)
}

// Rust 1.99 std HashMap (hashbrown 0.17.1): for a requested count, at most
// next_power_of_two(2*count), with a minimum 16 buckets, is needed at its 7/8
// load factor. Each allocation contains buckets, one control byte per bucket,
// and a group-width/alignment tail. Check and charge before try_reserve.
fn hash_table<K, V>(count: usize, budget: &mut RecoveryBudget) -> Result<(), RecoveryError> {
    if count == 0 {
        return Ok(());
    }
    let buckets = count
        .checked_mul(2)
        .and_then(usize::checked_next_power_of_two)
        .map(|n| n.max(16))
        .ok_or(RecoveryError::Limit {
            kind: Kind::MetadataBytes,
            limit: budget.limits.max_metadata_bytes,
        })?;
    let alignment = std::mem::align_of::<(K, V)>().max(16);
    let bytes = std::mem::size_of::<(K, V)>()
        .checked_add(1)
        .and_then(|v| v.checked_mul(buckets))
        .and_then(|v| v.checked_add(alignment))
        .and_then(|v| v.checked_add(16))
        .ok_or(RecoveryError::Limit {
            kind: Kind::MetadataBytes,
            limit: budget.limits.max_metadata_bytes,
        })?;
    budget.metadata(bytes as u64)
}

fn map_fjall(error: crate::Error) -> RecoveryError {
    match error {
        crate::Error::Io(error) => RecoveryError::Io(error),
        _ => RecoveryError::Unsupported("Fjall recovery setup failed"),
    }
}

impl RecoveryPlan {
    pub(crate) fn commit(mut self) -> Result<Keyspace, RecoveryError> {
        let Some((_, active_path, _)) = self.journals.journals.last() else {
            return Err(RecoveryError::Unsupported("keyspace has no active journal"));
        };
        let active_name_bytes = active_path.as_os_str().as_encoded_bytes().len();
        // The local path, writer and manager each retain an owned copy.
        capacity::<u8>(
            active_name_bytes
                .checked_mul(3)
                .ok_or(RecoveryError::Limit {
                    kind: Kind::MetadataBytes,
                    limit: self.budget.limits.max_metadata_bytes,
                })?,
            &mut self.budget,
        )?;
        let active_path = active_path.clone();
        let sealed_count = self.journals.journals.len().saturating_sub(1);
        let mut journal_sizes = vector::<u64>(sealed_count, &mut self.budget)?;
        for (_, path, _) in self.journals.journals.iter().take(sealed_count) {
            charge_path_io(path, &mut self.budget)?;
            journal_sizes.push(path.metadata().map_err(RecoveryError::Io)?.len());
        }
        let mut initial_map = Partitions::with_hasher(xxhash_rust::xxh3::Xxh3Builder::new());
        hash_table::<crate::batch::PartitionKey, PartitionHandle>(
            self.partitions.len(),
            &mut self.budget,
        )?;
        initial_map
            .try_reserve(self.partitions.len())
            .map_err(|_| alloc_error())?;
        capacity::<Item>(10, &mut self.budget)?;
        if sealed_count > 10 {
            capacity::<Item>(sealed_count, &mut self.budget)?;
        }
        let mut journal_manager = JournalManager::from_active(active_path.clone());
        journal_manager
            .reserve_recovery(sealed_count)
            .map_err(|_| alloc_error())?;
        charge_path_io(&active_path, &mut self.budget)?;
        let journal = Arc::new(Journal::prepare_existing(&active_path).map_err(map_fjall)?);
        // The lease cleanup remains disarmed until all open actions succeed.
        self.config.owner_lock = Some(self.journals.owner_lock.clone());
        let tracker = SnapshotTracker::with_owner_lock(self.config.owner_lock.clone());
        let inner = KeyspaceInner {
            config: self.config,
            journal: journal.clone(),
            partitions: Arc::new(RwLock::new(initial_map)),
            seqno: SequenceNumberCounter::default(),
            visible_seqno: SequenceNumberCounter::default(),
            flush_manager: Arc::new(RwLock::new(FlushManager::new())),
            journal_manager: Arc::new(RwLock::new(journal_manager)),
            flush_semaphore: Arc::new(Semaphore::new(0)),
            compaction_manager: CompactionManager::default(),
            stop_signal: lsm_tree::stop_signal::StopSignal::default(),
            active_background_threads: Arc::default(),
            write_buffer_manager: WriteBufferManager::default(),
            is_poisoned: Arc::default(),
            snapshot_tracker: tracker,
            stats: Arc::default(),
        };
        let keyspace = Keyspace(Arc::new(inner));
        let mut handles = vector::<PartitionHandle>(self.partitions.len(), &mut self.budget)?;
        arc::<crate::partition::PartitionHandleInner>(self.partitions.len(), &mut self.budget)?;
        let mut next_seqno = 0_u64;
        let mut write_buffer_bytes = 0_u64;
        for partition in &self.partitions {
            self.budget.charge(Kind::ReplayItems, 1)?;
            let name_charge = partition
                .name
                .len()
                .checked_add(std::mem::size_of::<u64>())
                .ok_or(RecoveryError::Limit {
                    kind: Kind::MetadataBytes,
                    limit: self.budget.limits.max_metadata_bytes,
                })?;
            self.budget.metadata(name_charge as u64)?;
            let name: crate::batch::PartitionKey = partition.name.as_str().into();
            let tree = partition.tree.tree();
            if let Some(seqno) = tree.get_highest_seqno() {
                next_seqno = next_seqno.max(seqno.checked_add(1).ok_or(
                    RecoveryError::Corruption("journal sequence overflow".into()),
                )?);
            }
            write_buffer_bytes = write_buffer_bytes
                .checked_add(u64::from(tree.active_memtable_size()))
                .ok_or(RecoveryError::Corruption(
                    "recovered memtable size overflow".into(),
                ))?;
            for (_, table) in &partition.sealed_tasks {
                self.budget.charge(Kind::ReplayItems, 1)?;
                write_buffer_bytes = write_buffer_bytes
                    .checked_add(u64::from(table.size()))
                    .ok_or(RecoveryError::Corruption(
                        "recovered memtable size overflow".into(),
                    ))?;
            }
            // Config::clone copies its owned root PathBuf for every handle.
            // This is separate from the LSM tree configuration path charges.
            charge_handle_path(&keyspace.config.path, &mut self.budget)?;
            let handle = PartitionHandle::from_keyspace(
                &keyspace,
                lsm_tree::AnyTree::Standard(tree.clone()),
                name.clone(),
                partition.options.options.clone(),
            );
            keyspace
                .partitions
                .write()
                .expect("lock is poisoned")
                .insert(name, handle.clone());
            handles.push(handle);
        }
        keyspace.seqno.fetch_max(next_seqno, Ordering::AcqRel);
        keyspace.visible_seqno.store(next_seqno, Ordering::Release);
        keyspace.write_buffer_manager.allocate(write_buffer_bytes);

        let task_partitions = self
            .partitions
            .iter()
            .filter(|p| !p.sealed_tasks.is_empty())
            .count();
        hash_table::<crate::batch::PartitionKey, crate::flush::queue::FlushQueue>(
            task_partitions,
            &mut self.budget,
        )?;
        {
            let mut flush = keyspace.flush_manager.write().expect("lock is poisoned");
            flush
                .reserve_recovery(task_partitions)
                .map_err(|_| alloc_error())?;
            for (partition, handle) in self.partitions.iter().zip(&handles) {
                self.budget.charge(Kind::ReplayItems, 1)?;
                if partition.sealed_tasks.is_empty() {
                    continue;
                }
                capacity::<Arc<Task>>(partition.sealed_tasks.len(), &mut self.budget)?;
                arc::<Task>(partition.sealed_tasks.len(), &mut self.budget)?;
                flush
                    .reserve_recovery_queue(handle.name.clone(), partition.sealed_tasks.len())
                    .map_err(|_| alloc_error())?;
                for (id, table) in &partition.sealed_tasks {
                    self.budget.charge(Kind::ReplayItems, 1)?;
                    flush.enqueue_task(
                        handle.name.clone(),
                        Task {
                            id: *id,
                            sealed_memtable: table.clone(),
                            partition: handle.clone(),
                        },
                    );
                }
            }
        }
        {
            let mut manager = keyspace.journal_manager.write().expect("lock is poisoned");
            for ((id, path, _), size) in self
                .journals
                .journals
                .iter()
                .take(sealed_count)
                .zip(journal_sizes)
            {
                let mut count = 0_usize;
                for watermark in &self.watermarks {
                    self.budget.charge(Kind::ReplayItems, 1)?;
                    if watermark.journal_id == *id {
                        count = count.checked_add(1).ok_or(RecoveryError::Limit {
                            kind: Kind::MetadataBytes,
                            limit: self.budget.limits.max_metadata_bytes,
                        })?;
                    }
                }
                let mut marks = vector::<EvictionWatermark>(count, &mut self.budget)?;
                for watermark in &self.watermarks {
                    self.budget.charge(Kind::ReplayItems, 1)?;
                    if watermark.journal_id != *id {
                        continue;
                    }
                    let handle = handles
                        .get(watermark.partition_index)
                        .ok_or(RecoveryError::Corruption("missing replay partition".into()))?;
                    marks.push(EvictionWatermark {
                        partition: handle.clone(),
                        lsn: watermark.seqno,
                    });
                }
                self.budget
                    .metadata(path.as_os_str().as_encoded_bytes().len() as u64)?;
                manager.enqueue(Item {
                    path: path.clone(),
                    size_in_bytes: size,
                    watermarks: marks,
                });
            }
        }
        // Native FS wrappers convert these retained paths when actions run.
        // Reserve each such request now, before any action can change a file.
        for cleanup in &self.cleanup {
            if let Some(manifest) = &cleanup.manifest_to_remove {
                self.budget.charge(Kind::ReplayItems, 1)?;
                charge_path_io(manifest, &mut self.budget)?;
            }
            for action in &cleanup.actions {
                self.budget.charge(Kind::ReplayItems, 1)?;
                match action {
                    CleanupAction::File(path) | CleanupAction::Directory(path) => {
                        charge_path_io(path, &mut self.budget)?
                    }
                }
            }
        }
        for (_, path, plan) in &self.journals.journals {
            if plan.truncate_to.is_some() {
                self.budget.charge(Kind::ReplayItems, 1)?;
                charge_path_io(path, &mut self.budget)?;
            }
        }

        // All stored-input reads, limit checks and input-driven allocations are
        // complete. From here apply only retained filesystem actions.
        journal.arm();
        for partition in self.partitions {
            drop(partition.tree.commit().map_err(map_tree)?);
        }
        for cleanup in self.cleanup {
            if let Some(manifest) = cleanup.manifest_to_remove {
                std::fs::remove_file(manifest).map_err(RecoveryError::Io)?;
            }
            for action in cleanup.actions {
                match action {
                    CleanupAction::File(path) => {
                        std::fs::remove_file(path).map_err(RecoveryError::Io)?
                    }
                    CleanupAction::Directory(path) => {
                        std::fs::remove_dir(path).map_err(RecoveryError::Io)?
                    }
                }
            }
        }
        for (_, path, plan) in &self.journals.journals {
            if let Some(length) = plan.truncate_to {
                let file = OpenOptions::new()
                    .write(true)
                    .open(path)
                    .map_err(RecoveryError::Io)?;
                file.set_len(length).map_err(RecoveryError::Io)?;
                file.sync_all().map_err(RecoveryError::Io)?;
            }
        }
        journal.persist(PersistMode::SyncAll).map_err(map_fjall)?;
        keyspace.start_background_threads().map_err(map_fjall)?;
        keyspace.set_temporary_cleanup(true);
        Ok(keyspace)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bounded_recovery::RecoveryLimits;
    use std::{collections::BTreeMap, path::Path};

    fn ceilings() -> RecoveryLimits {
        RecoveryLimits {
            max_value_bytes: 4096,
            max_key_bytes: 4096,
            max_replay_bytes: 1_000_000,
            max_replay_items: 1_000_000,
            max_metadata_bytes: 128 * 1024 * 1024,
            max_input_bytes: 256 * 1024 * 1024,
            max_alloc_bytes: 256 * 1024 * 1024,
        }
    }

    fn inventory(root: &Path) -> BTreeMap<std::path::PathBuf, Option<Vec<u8>>> {
        fn visit(
            root: &Path,
            path: &Path,
            items: &mut BTreeMap<std::path::PathBuf, Option<Vec<u8>>>,
        ) {
            for entry in std::fs::read_dir(path).unwrap() {
                let entry = entry.unwrap();
                let path = entry.path();
                let relative = path.strip_prefix(root).unwrap().to_path_buf();
                if entry.file_type().unwrap().is_dir() {
                    items.insert(relative, None);
                    visit(root, &path, items);
                } else {
                    items.insert(relative, Some(std::fs::read(path).unwrap()));
                }
            }
        }
        let mut items = BTreeMap::new();
        visit(root, root, &mut items);
        items
    }

    #[test]
    fn handle_path_charge_exact_boundary_and_refusal_preserves_many_partition_store() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("x".repeat(150)).join("y".repeat(150));
        {
            let keyspace = Config::new(&path)
                .flush_workers(0)
                .compaction_workers(0)
                .open()
                .unwrap();
            for n in 0..12 {
                let part = keyspace
                    .open_partition(
                        &format!("p{n:02}"),
                        crate::PartitionCreateOptions::default(),
                    )
                    .unwrap();
                part.insert("key", "value").unwrap();
            }
            keyspace.persist(crate::PersistMode::SyncAll).unwrap();
        }
        let orphan = path.join("partitions/orphan");
        std::fs::create_dir(&orphan).unwrap();
        std::fs::write(orphan.join("untouched"), b"retained").unwrap();
        let before = inventory(&path);
        let config = Config::new(&path)
            .temporary(true)
            .flush_workers(0)
            .compaction_workers(0);
        let lock = Keyspace::lock_owner(&path).unwrap();
        let plan = super::super::prepare(config, RecoveryBudget::new(ceilings()), lock).unwrap();
        PATH_CHARGE.with(|target| target.set((8, 0, 0)));
        let result = plan.commit();
        let (_, charged_before, path_bytes) = PATH_CHARGE.with(|target| target.replace((0, 0, 0)));
        assert_eq!(path_bytes, path.as_os_str().as_encoded_bytes().len() as u64);
        assert!(
            matches!(result, Err(RecoveryError::Limit { kind: Kind::AllocationBytes, limit }) if limit == charged_before + path_bytes - 1)
        );
        assert_eq!(inventory(&path), before);

        let mut exact = ceilings();
        exact.max_alloc_bytes = path_bytes;
        let mut budget = RecoveryBudget::new(exact.clone());
        charge_handle_path(&path, &mut budget).unwrap();
        assert_eq!(budget.allocated_bytes(), path_bytes);
        exact.max_alloc_bytes -= 1;
        assert!(matches!(
            charge_handle_path(&path, &mut RecoveryBudget::new(exact)),
            Err(RecoveryError::Limit {
                kind: Kind::AllocationBytes,
                ..
            })
        ));
    }

    #[cfg(unix)]
    #[test]
    fn temporary_cleanup_keeps_inode_stable_for_concurrent_reopen() {
        use std::os::unix::fs::MetadataExt;
        use std::sync::mpsc;
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("store");
        let owner = Config::new(&path)
            .temporary(true)
            .flush_workers(0)
            .compaction_workers(0)
            .open()
            .unwrap();
        let part = owner
            .open_partition("data", crate::PartitionCreateOptions::default())
            .unwrap();
        part.insert("key", "old-value").unwrap();
        let lock = path.join(".fjall-owner.lock");
        let inode = std::fs::metadata(&lock).unwrap().ino();
        let extras = path.join("extras");
        std::fs::create_dir(&extras).unwrap();
        for n in 0..100 {
            std::fs::write(extras.join(n.to_string()), b"temporary").unwrap();
        }
        drop(owner);
        let (ready_tx, ready_rx) = mpsc::channel();
        let (go_tx, go_rx) = mpsc::channel();
        let thread_path = path.clone();
        let opener = std::thread::spawn(move || {
            let config = || {
                Config::new(&thread_path)
                    .flush_workers(0)
                    .compaction_workers(0)
            };
            assert!(config().open().is_err());
            ready_tx.send(()).unwrap();
            go_rx.recv().unwrap();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
            loop {
                if let Ok(opened) = config().open() {
                    return opened;
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "reopen did not acquire the lease"
                );
                std::thread::yield_now();
            }
        });
        ready_rx.recv().unwrap();
        go_tx.send(()).unwrap();
        drop(part);
        let reopened = opener.join().unwrap();
        assert_eq!(std::fs::metadata(lock).unwrap().ino(), inode);
        assert!(!reopened.partition_exists("data"));
        assert!(!extras.exists());
    }
}
