// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Admission of all journals while retaining the cooperative owner lease.

use crate::bounded_recovery::{
    JournalPlan, JournalSetPlan, RecoveryBudget, RecoveryError, RecoveryLimitKind as Kind,
};
use std::{
    io,
    path::{Path, PathBuf},
};

// Rust 1.99 Unix ReadDir::next copies d_name into a CString, file_name copies
// it again, and path clones/grows the root. Linux and Darwin dirent record
// lengths are u16, so 65536 covers the name plus its terminator, including
// filesystems with entries longer than the usual NAME_MAX.
#[cfg(any(target_os = "linux", target_os = "macos"))]
const NAME_COPIES_RESERVE: u64 = 3 * 65_536;

// Windows WIN32_FIND_DATAW has 260 UTF-16 filename units. Rust 1.99's
// OsString::from_wide starts at that capacity and geometrically grows while
// encoding at most 3 WTF-8 bytes per unit; at most 260+520+1040 is requested
// per conversion. file_name and path each perform one conversion, and path
// then copies at most 780 name bytes into its grown buffer.
#[cfg(target_os = "windows")]
const NAME_COPIES_RESERVE: u64 = 2 * (260 + 520 + 1040) + 780;

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
const NAME_COPIES_RESERVE: u64 = 0;

fn metadata_capacity<T>(budget: &mut RecoveryBudget, count: usize) -> Result<(), RecoveryError> {
    let bytes = count
        .checked_mul(std::mem::size_of::<T>())
        .ok_or(RecoveryError::Limit {
            kind: Kind::MetadataBytes,
            limit: budget.limits.max_metadata_bytes,
        })?;
    budget.metadata(bytes as u64)
}

pub(crate) fn reserve_one<T>(
    items: &mut Vec<T>,
    budget: &mut RecoveryBudget,
) -> Result<(), RecoveryError> {
    if items.len() == items.capacity() {
        let capacity =
            items
                .capacity()
                .checked_mul(2)
                .map(|n| n.max(4))
                .ok_or(RecoveryError::Limit {
                    kind: Kind::MetadataBytes,
                    limit: budget.limits.max_metadata_bytes,
                })?;
        metadata_capacity::<T>(budget, capacity)?;
        items
            .try_reserve_exact(capacity - items.len())
            .map_err(|_| {
                RecoveryError::Io(io::Error::new(
                    io::ErrorKind::OutOfMemory,
                    "journal directory allocation failed",
                ))
            })?;
    }
    Ok(())
}

pub(crate) fn charge_directory_entry(
    directory: &Path,
    budget: &mut RecoveryBudget,
) -> Result<(), RecoveryError> {
    let root_bytes = directory.as_os_str().as_encoded_bytes().len() as u64;
    let reserve = root_bytes
        .checked_mul(3)
        .and_then(|v| v.checked_add(NAME_COPIES_RESERVE))
        .and_then(|v| v.checked_add(16))
        .ok_or(RecoveryError::Limit {
            kind: Kind::MetadataBytes,
            limit: budget.limits.max_metadata_bytes,
        })?;
    budget.metadata(reserve)
}

// Precharge the native path conversion requested by Rust 1.99's std FS
// wrappers. Unix uses a stack C string below 384 bytes and otherwise requests
// len+1 bytes. Windows converts to UTF-16 and may normalize a long absolute
// path; the bound covers the input vector, normalization buffer and prefixed
// output growth. Other targets are refused by admission.
pub(crate) fn charge_path_io(
    path: &Path,
    budget: &mut RecoveryBudget,
) -> Result<(), RecoveryError> {
    let len = path.as_os_str().as_encoded_bytes().len() as u64;
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    let capacity = if len < 384 {
        0
    } else {
        len.checked_add(1).ok_or(RecoveryError::Limit {
            kind: Kind::MetadataBytes,
            limit: budget.limits.max_metadata_bytes,
        })?
    };
    #[cfg(target_os = "windows")]
    let capacity =
        len.checked_add(16)
            .and_then(|n| n.checked_mul(8))
            .ok_or(RecoveryError::Limit {
                kind: Kind::MetadataBytes,
                limit: budget.limits.max_metadata_bytes,
            })?;
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    let capacity = 0;
    budget.metadata(capacity)
}

pub(crate) fn charge_open_directory(
    path: &Path,
    budget: &mut RecoveryBudget,
) -> Result<(), RecoveryError> {
    budget.metadata(path.as_os_str().as_encoded_bytes().len() as u64)?;
    charge_path_io(path, budget)
}

pub(crate) fn prepare(
    root: &Path,
    budget: &mut RecoveryBudget,
) -> Result<JournalSetPlan, RecoveryError> {
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    return Err(RecoveryError::Unsupported(
        "journal directory accounting is not audited on this platform",
    ));

    let lock = crate::Keyspace::lock_owner(root).map_err(|error| match error {
        crate::Error::Io(error) => RecoveryError::Io(error),
        _ => RecoveryError::Unsupported("cannot acquire owner lock"),
    })?;
    prepare_locked(root, budget, lock)
}

pub(crate) fn prepare_locked(
    root: &Path,
    budget: &mut RecoveryBudget,
    lock: std::sync::Arc<crate::keyspace::OwnerLease>,
) -> Result<JournalSetPlan, RecoveryError> {
    let directory = root.join(crate::file::JOURNALS_FOLDER);
    charge_open_directory(&directory, budget)?;
    let mut entries = std::fs::read_dir(&directory).map_err(RecoveryError::Io)?;
    let mut paths: Vec<(u64, PathBuf)> = Vec::new();
    loop {
        // Charge before requesting the entry: root cloning followed by a
        // geometric Vec growth can request at most three root lengths, plus
        // the appended name and a small separator/minimum-capacity allowance.
        charge_directory_entry(&directory, budget)?;
        let Some(entry) = entries.next() else {
            break;
        };
        let entry = entry.map_err(RecoveryError::Io)?;
        budget.charge(Kind::ReplayItems, 1)?;
        let filename = entry.file_name();
        let filename = filename
            .to_str()
            .ok_or(RecoveryError::Unsupported("non-UTF-8 journal filename"))?;
        if filename == ".DS_Store" || filename.starts_with("._") {
            continue;
        }
        let path = entry.path();
        charge_path_io(&path, budget)?;
        if !std::fs::symlink_metadata(&path)
            .map_err(RecoveryError::Io)?
            .file_type()
            .is_file()
        {
            return Err(RecoveryError::Unsupported(
                "journal directory contains a non-file",
            ));
        }
        let id = filename
            .strip_suffix(".sealed")
            .unwrap_or(filename)
            .parse::<u64>()
            .map_err(|_| RecoveryError::Corruption("invalid journal filename".into()))?;
        reserve_one(&mut paths, budget)?;
        paths.push((id, path));
    }
    for i in 1..paths.len() {
        let mut j = i;
        while j > 0 {
            budget.charge(Kind::ReplayItems, 1)?;
            let (Some(left), Some(right)) = (paths.get(j - 1), paths.get(j)) else {
                return Err(RecoveryError::Corruption("invalid journal order".into()));
            };
            if left.0 <= right.0 {
                break;
            }
            paths.swap(j - 1, j);
            j -= 1;
        }
    }
    for pair in paths.windows(2) {
        budget.charge(Kind::ReplayItems, 1)?;
        if pair
            .first()
            .zip(pair.get(1))
            .is_some_and(|(left, right)| left.0 == right.0)
        {
            return Err(RecoveryError::Corruption(
                "duplicate journal identifiers".into(),
            ));
        }
    }
    let mut journals: Vec<(u64, PathBuf, JournalPlan)> = Vec::new();
    for (id, path) in paths {
        let plan = super::bounded::prepare(&path, budget)?;
        reserve_one(&mut journals, budget)?;
        journals.push((id, path, plan));
    }
    budget.metadata(
        (std::mem::size_of::<std::fs::File>() + 2 * std::mem::size_of::<usize>()) as u64,
    )?;
    Ok(JournalSetPlan {
        owner_lock: lock,
        budget_identity: budget.identity.clone(),
        journals,
    })
}
