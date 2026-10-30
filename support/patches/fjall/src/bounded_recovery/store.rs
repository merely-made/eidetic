// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Whole-keyspace read-only recovery preparation and admitted commit.

mod commit;

use super::{
    JournalSetPlan, PartitionOptionsPlan, PartitionRecoveryBudget, RecoveryBudget, RecoveryError,
    RecoveryLimitKind as Kind, RecoveryLimits, ReplayPartition,
};
use crate::{
    file::{FJALL_MARKER, LSM_MANIFEST_FILE, PARTITIONS_FOLDER, PARTITION_DELETED_MARKER},
    journal::bounded_set::{
        charge_directory_entry, charge_open_directory, charge_path_io, reserve_one,
    },
    Config, Keyspace,
};
use lsm_tree::{
    bounded_prepare::{PrepareError, PreparedTree, RecoveryResource},
    AbstractTree,
};
use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

struct Partition {
    name: String,
    options: PartitionOptionsPlan,
    tree: PreparedTree,
    sealed_tasks: Vec<(u64, std::sync::Arc<lsm_tree::Memtable>)>,
}

struct Watermark {
    journal_id: u64,
    partition_index: usize,
    seqno: u64,
}

struct CleanupPartition {
    manifest_to_remove: Option<PathBuf>,
    actions: Vec<CleanupAction>,
}

enum CleanupAction {
    File(PathBuf),
    Directory(PathBuf),
}

/// Retains all admitted source objects and deferred cleanup under the owner lock.
pub(crate) struct RecoveryPlan {
    config: Config,
    budget: RecoveryBudget,
    journals: JournalSetPlan,
    partitions: Vec<Partition>,
    cleanup: Vec<CleanupPartition>,
    watermarks: Vec<Watermark>,
}

fn map_tree(error: PrepareError) -> RecoveryError {
    match error {
        PrepareError::Limit { resource, limit } => RecoveryError::Limit {
            kind: match resource {
                RecoveryResource::InputBytes => Kind::InputBytes,
                RecoveryResource::AllocationBytes => Kind::AllocationBytes,
                RecoveryResource::MetadataBytes => Kind::MetadataBytes,
                RecoveryResource::VisitedItems => Kind::ReplayItems,
                RecoveryResource::KeyBytes => Kind::KeyBytes,
            },
            limit,
        },
        PrepareError::Unsupported(reason) => RecoveryError::Unsupported(reason),
        PrepareError::Corruption(reason) => RecoveryError::Corruption(reason),
        PrepareError::Io(error) => RecoveryError::Io(error),
    }
}

fn join(path: &Path, name: &str, budget: &mut RecoveryBudget) -> Result<PathBuf, RecoveryError> {
    let path_bytes = path.as_os_str().as_encoded_bytes().len() as u64;
    let bytes = path_bytes
        .checked_mul(3)
        .and_then(|n| n.checked_add(name.len() as u64))
        .and_then(|n| n.checked_add(16))
        .ok_or(RecoveryError::Limit {
            kind: Kind::MetadataBytes,
            limit: budget.limits.max_metadata_bytes,
        })?;
    budget.metadata(bytes)?;
    Ok(path.join(name))
}

fn exists(path: &Path, budget: &mut RecoveryBudget) -> Result<bool, RecoveryError> {
    charge_path_io(path, budget)?;
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err(RecoveryError::Unsupported("symlink in recovery metadata"))
        }
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(RecoveryError::Io(error)),
    }
}

fn clone_path(path: &Path, budget: &mut RecoveryBudget) -> Result<PathBuf, RecoveryError> {
    budget.metadata(path.as_os_str().as_encoded_bytes().len() as u64)?;
    Ok(path.to_path_buf())
}

fn prepare_cleanup(
    root: PathBuf,
    manifest_to_remove: Option<PathBuf>,
    budget: &mut RecoveryBudget,
) -> Result<CleanupPartition, RecoveryError> {
    let mut stack: Vec<(PathBuf, bool)> = Vec::new();
    reserve_one(&mut stack, budget)?;
    stack.push((root, false));
    let mut actions = Vec::new();
    while let Some((path, visited)) = stack.pop() {
        budget.charge(Kind::ReplayItems, 1)?;
        if visited {
            reserve_one(&mut actions, budget)?;
            actions.push(CleanupAction::Directory(path));
            continue;
        }
        let dir = clone_path(&path, budget)?;
        reserve_one(&mut stack, budget)?;
        stack.push((path, true));
        charge_open_directory(&dir, budget)?;
        let mut entries = std::fs::read_dir(&dir).map_err(RecoveryError::Io)?;
        loop {
            charge_directory_entry(&dir, budget)?;
            let Some(entry) = entries.next() else { break };
            let entry = entry.map_err(RecoveryError::Io)?;
            budget.charge(Kind::ReplayItems, 1)?;
            let child = entry.path();
            if manifest_to_remove
                .as_ref()
                .is_some_and(|manifest| manifest == &child)
            {
                continue;
            }
            charge_path_io(&child, budget)?;
            if std::fs::symlink_metadata(&child)
                .map_err(RecoveryError::Io)?
                .file_type()
                .is_dir()
            {
                reserve_one(&mut stack, budget)?;
                stack.push((child, false));
            } else {
                reserve_one(&mut actions, budget)?;
                actions.push(CleanupAction::File(child));
            }
        }
    }
    Ok(CleanupPartition {
        manifest_to_remove,
        actions,
    })
}

fn check_version(path: &Path, budget: &mut RecoveryBudget) -> Result<(), RecoveryError> {
    let marker = join(path, FJALL_MARKER, budget)?;
    if !exists(&marker, budget)? {
        return Err(RecoveryError::Unsupported(
            "new keyspace bounded creation is not implemented",
        ));
    }
    charge_path_io(&marker, budget)?;
    let mut file = match File::open(marker) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(RecoveryError::Unsupported(
                "new keyspace bounded creation is not implemented",
            ))
        }
        Err(error) => return Err(RecoveryError::Io(error)),
    };
    budget.charge(Kind::InputBytes, 4)?;
    let mut header = [0_u8; 4];
    file.read_exact(&mut header).map_err(|error| {
        if error.kind() == std::io::ErrorKind::UnexpectedEof {
            RecoveryError::Corruption("invalid keyspace version".into())
        } else {
            RecoveryError::Io(error)
        }
    })?;
    match crate::version::Version::parse_file_header(&header) {
        Some(crate::version::Version::V2) => Ok(()),
        Some(_) => Err(RecoveryError::Unsupported("keyspace version")),
        None => Err(RecoveryError::Corruption("invalid keyspace version".into())),
    }
}

pub(crate) fn open(config: Config, limits: RecoveryLimits) -> Result<Keyspace, RecoveryError> {
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    return Err(RecoveryError::Unsupported(
        "directory accounting not audited for this platform",
    ));

    let mut budget = RecoveryBudget::new(limits);
    let lock = Keyspace::lock_owner(&config.path).map_err(|error| match error {
        crate::Error::Io(error) => RecoveryError::Io(error),
        _ => RecoveryError::Unsupported("cannot acquire owner lock"),
    })?;
    let marker = join(&config.path, FJALL_MARKER, &mut budget)?;
    if exists(&marker, &mut budget)? {
        return prepare(config, budget, lock)?.commit();
    }
    // A missing version marker is a new store only when the directory is
    // otherwise empty. Never initialize over unrecognized retained files.
    charge_open_directory(&config.path, &mut budget)?;
    let mut entries = std::fs::read_dir(&config.path).map_err(RecoveryError::Io)?;
    loop {
        charge_directory_entry(&config.path, &mut budget)?;
        let Some(entry) = entries.next() else { break };
        let entry = entry.map_err(RecoveryError::Io)?;
        budget.charge(Kind::ReplayItems, 1)?;
        let path = entry.path();
        charge_path_io(&path, &mut budget)?;
        if entry.file_name() != ".fjall-owner.lock"
            || !std::fs::symlink_metadata(path)
                .map_err(RecoveryError::Io)?
                .file_type()
                .is_file()
        {
            return Err(RecoveryError::Unsupported(
                "nonempty uninitialized keyspace",
            ));
        }
    }
    drop(entries);
    // No stored data exists, so there are no limit-bearing inputs after this
    // point. Creation and background work occur only after admission.
    let keyspace = Keyspace::create_new_locked(config, lock).map_err(|error| match error {
        crate::Error::Io(error) => RecoveryError::Io(error),
        _ => RecoveryError::Unsupported("new keyspace creation failed"),
    })?;
    keyspace
        .start_background_threads()
        .map_err(|error| match error {
            crate::Error::Io(error) => RecoveryError::Io(error),
            _ => RecoveryError::Unsupported("new keyspace background startup failed"),
        })?;
    keyspace.set_temporary_cleanup(true);
    Ok(keyspace)
}

fn prepare(
    config: Config,
    mut budget: RecoveryBudget,
    lock: std::sync::Arc<crate::keyspace::OwnerLease>,
) -> Result<RecoveryPlan, RecoveryError> {
    // The common owner lease is retained before stored input is inspected.
    let journals = crate::journal::bounded_set::prepare_locked(&config.path, &mut budget, lock)?;
    check_version(&config.path, &mut budget)?;
    let directory = join(&config.path, PARTITIONS_FOLDER, &mut budget)?;
    charge_open_directory(&directory, &mut budget)?;
    let mut entries = std::fs::read_dir(&directory).map_err(RecoveryError::Io)?;
    let mut candidates: Vec<(String, PathBuf)> = Vec::new();
    let mut cleanup = Vec::new();
    loop {
        charge_directory_entry(&directory, &mut budget)?;
        let Some(entry) = entries.next() else {
            break;
        };
        let entry = entry.map_err(RecoveryError::Io)?;
        budget.charge(Kind::ReplayItems, 1)?;
        let path = entry.path();
        charge_path_io(&path, &mut budget)?;
        let ty = std::fs::symlink_metadata(&path)
            .map_err(RecoveryError::Io)?
            .file_type();
        if ty.is_file() {
            continue;
        }
        if !ty.is_dir() {
            return Err(RecoveryError::Unsupported("non-directory partition entry"));
        }
        let deleted_path = join(&path, PARTITION_DELETED_MARKER, &mut budget)?;
        let deleted = exists(&deleted_path, &mut budget)?;
        let manifest_path = join(&path, LSM_MANIFEST_FILE, &mut budget)?;
        let manifest = exists(&manifest_path, &mut budget)?;
        if deleted || !manifest {
            reserve_one(&mut cleanup, &mut budget)?;
            cleanup.push(prepare_cleanup(
                path,
                (deleted && manifest).then_some(manifest_path),
                &mut budget,
            )?);
            continue;
        }
        let filename = entry.file_name();
        let name = filename
            .to_str()
            .ok_or(RecoveryError::Unsupported("non-UTF-8 partition name"))?;
        if !crate::partition::name::is_valid_partition_name(name) {
            return Err(RecoveryError::Unsupported("partition name"));
        }
        budget.metadata(name.len() as u64)?;
        let mut owned_name = String::new();
        owned_name.try_reserve_exact(name.len()).map_err(|_| {
            RecoveryError::Io(std::io::Error::new(
                std::io::ErrorKind::OutOfMemory,
                "partition name allocation failed",
            ))
        })?;
        owned_name.push_str(name);
        reserve_one(&mut candidates, &mut budget)?;
        candidates.push((owned_name, path));
    }
    // Charge each comparison rather than hiding directory-driven sort work.
    for i in 1..candidates.len() {
        let mut j = i;
        while j > 0 {
            budget.charge(Kind::ReplayItems, 1)?;
            let (Some(left), Some(right)) = (candidates.get(j - 1), candidates.get(j)) else {
                return Err(RecoveryError::Corruption("invalid partition order".into()));
            };
            if left.0 <= right.0 {
                break;
            }
            candidates.swap(j - 1, j);
            j -= 1;
        }
    }
    let mut partitions = Vec::new();
    for (name, path) in candidates {
        let options = super::prepare_partition_options(&path, &mut budget)?;
        let path_bytes = path.as_os_str().as_encoded_bytes().len() as u64;
        let config_bytes = path_bytes
            .checked_mul(8)
            .and_then(|n| n.checked_add(256))
            .ok_or(RecoveryError::Limit {
                kind: Kind::MetadataBytes,
                limit: budget.limits.max_metadata_bytes,
            })?;
        budget.metadata(config_bytes)?;
        let mut tree_config = lsm_tree::Config::new(&path)
            .descriptor_table(config.descriptor_table.clone())
            .use_cache(config.cache.clone());
        tree_config.bloom_bits_per_key = options.options.bloom_bits_per_key;
        tree_config.data_block_size = options.options.data_block_size;
        tree_config.index_block_size = options.options.index_block_size;
        tree_config.compression = options.options.compression;
        let tree = lsm_tree::bounded_prepare::prepare(
            tree_config,
            &mut PartitionRecoveryBudget {
                ledger: &mut budget,
                partition_bytes: name.len() as u64,
            },
        )
        .map_err(map_tree)?;
        reserve_one(&mut partitions, &mut budget)?;
        partitions.push(Partition {
            name,
            options,
            tree,
            sealed_tasks: Vec::new(),
        });
    }
    let spec_bytes = partitions
        .len()
        .checked_mul(std::mem::size_of::<ReplayPartition<'_>>())
        .ok_or(RecoveryError::Limit {
            kind: Kind::MetadataBytes,
            limit: budget.limits.max_metadata_bytes,
        })?;
    budget.metadata(spec_bytes as u64)?;
    let mut specs = Vec::new();
    specs.try_reserve_exact(partitions.len()).map_err(|_| {
        RecoveryError::Io(std::io::Error::new(
            std::io::ErrorKind::OutOfMemory,
            "replay partition list allocation failed",
        ))
    })?;
    for partition in &partitions {
        specs.push(ReplayPartition {
            name: &partition.name,
            highest_persisted_seqno: partition.tree.tree().get_highest_persisted_seqno(),
        });
    }
    let replay = journals.prepare_replay(&specs, &mut budget)?;
    drop(specs);
    let mut watermarks = Vec::new();
    if replay.partitions.len() != partitions.len() {
        return Err(RecoveryError::Corruption(
            "replay partition mismatch".into(),
        ));
    }
    for (index, (partition, replay_part)) in
        partitions.iter_mut().zip(replay.partitions).enumerate()
    {
        if partition.name != replay_part.name {
            return Err(RecoveryError::Corruption(
                "replay partition mismatch".into(),
            ));
        }
        let sealed_count = replay_part
            .tables
            .iter()
            .filter(|table| !table.active && !table.skipped)
            .count();
        let bytes = sealed_count
            .checked_mul(std::mem::size_of::<lsm_tree::Memtable>())
            .ok_or(RecoveryError::Limit {
                kind: Kind::MetadataBytes,
                limit: budget.limits.max_metadata_bytes,
            })?;
        budget.metadata(bytes as u64)?;
        let mut sealed = Vec::new();
        sealed.try_reserve_exact(sealed_count).map_err(|_| {
            RecoveryError::Io(std::io::Error::new(
                std::io::ErrorKind::OutOfMemory,
                "sealed replay list allocation failed",
            ))
        })?;
        let mut active = None;
        for table in replay_part.tables {
            budget.charge(Kind::ReplayItems, 1)?;
            if !table.active {
                reserve_one(&mut watermarks, &mut budget)?;
                watermarks.push(Watermark {
                    journal_id: table.journal_id,
                    partition_index: index,
                    seqno: table.watermark,
                });
            }
            if table.active {
                active = Some(table.table);
            } else if !table.skipped {
                sealed.push(table.table);
            }
        }
        partition.sealed_tasks = partition
            .tree
            .install_replay(
                active,
                sealed,
                &mut PartitionRecoveryBudget {
                    ledger: &mut budget,
                    partition_bytes: partition.name.len() as u64,
                },
            )
            .map_err(map_tree)?;
    }
    Ok(RecoveryPlan {
        config,
        budget,
        journals,
        partitions,
        cleanup,
        watermarks,
    })
}
