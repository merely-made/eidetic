// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Small, recoverable replacement for product-owned settings documents.
//!
//! Moved from Mere's `pandect` 2026-10-31 (Mere distillery-extraction D13), so
//! a store-owning crate below Mere can use it.
//!
//! This module deliberately shares only write mechanics. Each product keeps
//! its path, schema, validation, and load policy. Replacement first uses the
//! platform's normal rename-overwrite behavior. A backup rename is used only
//! when that operation reports that the existing destination blocks it.
//! [`replace_bytes_if_unchanged`] adds the guard for read-modify-write
//! callers: it writes only if the file still holds the bytes the caller read.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);

/// Fully write and sync `contents` to a uniquely created sibling temporary file,
/// then replace `target`.
///
/// The direct rename preserves the filesystem's atomic replacement behavior
/// where supported. If the platform rejects replacing an existing destination,
/// the previous document moves to `<extension>.previous` until the temporary
/// file is installed. That fallback restores the previous document on ordinary
/// replacement errors; interruption during the two-rename fallback can leave
/// the target absent with its old contents in the backup. A stale backup is
/// removed as part of a successful replacement.
pub fn write_bytes_with_backup(target: &Path, contents: &[u8]) -> io::Result<()> {
    let temporary = stage_bytes(target, contents)?;
    replace_temporary_with_backup(target, &temporary, &backup_path(target), |from, to| {
        fs::rename(from, to)
    })
}

/// How a guarded replacement ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[must_use = "a conflict means nothing was written"]
pub enum Replacement {
    /// The target held the baseline and now holds the new contents.
    Installed,
    /// The target no longer matched the baseline. Nothing was written.
    Conflict,
}

/// Replace `target` with `contents` only if it still holds `baseline`: the
/// bytes the caller read before computing `contents`, or `None` if the file did
/// not exist then. Otherwise report [`Replacement::Conflict`] and write nothing,
/// so the caller can reload and reapply its change.
///
/// An advisory lock on the sibling `<target>.lock` file covers the compare and
/// the rename, so two guarded writers cannot both pass the compare and then
/// replace each other's result. Staging and syncing the new contents happen
/// before the lock is taken. The lock excludes other guarded replacements only;
/// readers and plain [`write_bytes_with_backup`] callers do not take it. The
/// lock file stays on disk: removing it would let two writers lock different
/// files.
pub fn replace_bytes_if_unchanged(
    target: &Path,
    baseline: Option<&[u8]>,
    contents: &[u8],
) -> io::Result<Replacement> {
    let temporary = stage_bytes(target, contents)?;
    let outcome = install_if_unchanged(target, baseline, &temporary);
    if !matches!(outcome, Ok(Replacement::Installed)) {
        let _ = fs::remove_file(&temporary);
    }
    outcome
}

fn install_if_unchanged(
    target: &Path,
    baseline: Option<&[u8]>,
    temporary: &Path,
) -> io::Result<Replacement> {
    let mut lock_name = target.file_name().unwrap_or_default().to_os_string();
    lock_name.push(".lock");
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(target.with_file_name(lock_name))?;
    // Dropping `lock` on any return releases it.
    lock.lock()?;

    let current = match fs::read(target) {
        Ok(bytes) => Some(bytes),
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => return Err(error),
    };
    if current.as_deref() != baseline {
        return Ok(Replacement::Conflict);
    }
    replace_temporary_with_backup(target, temporary, &backup_path(target), |from, to| {
        fs::rename(from, to)
    })?;
    Ok(Replacement::Installed)
}

fn backup_path(target: &Path) -> PathBuf {
    target.with_extension(format!(
        "{}.previous",
        target
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or_default()
    ))
}

/// Fully write and sync `contents` to a uniquely created sibling temporary file,
/// then install it with one direct rename to `target`.
///
/// On a filesystem where rename can replace an existing destination, readers
/// observe either the old complete file or the new complete file. If the
/// platform cannot replace an existing destination, this returns the rename
/// error and leaves the old target in place. Staging cleanup is best-effort on
/// ordinary errors. This guarantees atomic visibility only on rename-capable
/// filesystems; it does not sync the containing directory or guarantee survival
/// across power loss.
pub fn write_bytes_atomic(target: &Path, contents: &[u8]) -> io::Result<()> {
    write_bytes_atomic_with(target, contents, |from, to| fs::rename(from, to))
}

fn write_bytes_atomic_with(
    target: &Path,
    contents: &[u8],
    mut rename: impl FnMut(&Path, &Path) -> io::Result<()>,
) -> io::Result<()> {
    let temporary = stage_bytes(target, contents)?;
    match rename(&temporary, target) {
        Ok(()) => Ok(()),
        Err(error) => {
            let _ = fs::remove_file(&temporary);
            Err(error)
        },
    }
}

fn stage_bytes(target: &Path, contents: &[u8]) -> io::Result<PathBuf> {
    let directory = target.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "settings target must have a parent directory",
        )
    })?;
    fs::create_dir_all(directory)?;

    let (temporary, mut file) = create_temporary_file(target)?;
    let write_result = file.write_all(contents).and_then(|()| file.sync_all());
    drop(file);
    if let Err(error) = write_result {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }
    Ok(temporary)
}

fn create_temporary_file(target: &Path) -> io::Result<(PathBuf, File)> {
    create_temporary_file_with(target, std::process::id(), || {
        NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed)
    })
}

fn create_temporary_file_with(
    target: &Path,
    process_id: u32,
    mut next_id: impl FnMut() -> u64,
) -> io::Result<(PathBuf, File)> {
    let directory = target.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "settings target must have a parent directory",
        )
    })?;
    let file_name = target.file_name().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "settings target must have a file name",
        )
    })?;

    loop {
        let mut temporary_name = file_name.to_os_string();
        temporary_name.push(format!(".{process_id}.{}.tmp", next_id()));
        let temporary = directory.join(temporary_name);
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
        {
            Ok(file) => return Ok((temporary, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
}

fn replace_temporary_with_backup(
    target: &Path,
    temporary: &Path,
    backup: &Path,
    mut rename: impl FnMut(&Path, &Path) -> io::Result<()>,
) -> io::Result<()> {
    match rename(temporary, target) {
        Ok(()) => {
            if backup.exists() {
                fs::remove_file(backup)?;
            }
            Ok(())
        },
        Err(replacement_error)
            if target.exists() && replacement_error.kind() == io::ErrorKind::AlreadyExists =>
        {
            replace_with_backup(target, temporary, backup, &mut rename)
        },
        Err(replacement_error) => Err(replacement_error),
    }
}

fn replace_with_backup(
    target: &Path,
    temporary: &Path,
    backup: &Path,
    rename: &mut impl FnMut(&Path, &Path) -> io::Result<()>,
) -> io::Result<()> {
    if backup.exists() {
        fs::remove_file(backup)?;
    }
    rename(target, backup)?;
    match rename(temporary, target) {
        Ok(()) => {
            fs::remove_file(backup)?;
            Ok(())
        },
        Err(replacement_error) => {
            if let Err(restore_error) = rename(backup, target) {
                return Err(io::Error::other(format!(
                    "could not replace {target:?}: {replacement_error}; could not restore backup: {restore_error}"
                )));
            }
            Err(replacement_error)
        },
    }
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    fn scratch(label: &str) -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "mere-atomic-file-{label}-{}-{nonce}",
            std::process::id()
        ))
    }

    #[test]
    fn a_second_write_replaces_the_document_and_cleans_up() {
        let directory = scratch("replace");
        let target = directory.join("settings.json");

        write_bytes_with_backup(&target, b"research").expect("first write");
        write_bytes_with_backup(&target, b"work").expect("replacement write");

        assert_eq!(fs::read(&target).expect("read target"), b"work");
        assert!(!target.with_extension("json.previous").exists());
        assert_eq!(
            fs::read_dir(&directory).expect("list scratch").count(),
            1,
            "only the target remains after a successful write"
        );
        fs::remove_dir_all(directory).expect("remove scratch");
    }

    #[test]
    fn overwrite_uses_one_direct_rename_when_supported() {
        let directory = scratch("direct");
        let target = directory.join("settings.json");
        let temporary = directory.join("settings.json.test.tmp");
        let backup = target.with_extension("json.previous");
        fs::create_dir_all(&directory).expect("create scratch");
        fs::write(&target, b"research").expect("write target");
        fs::write(&temporary, b"work").expect("write temporary");
        let mut rename_count = 0;

        replace_temporary_with_backup(&target, &temporary, &backup, |from, to| {
            rename_count += 1;
            assert_eq!(from, temporary);
            assert_eq!(to, target);
            assert_eq!(
                fs::read(to).expect("old target remains until rename"),
                b"research"
            );
            fs::rename(from, to)
        })
        .expect("direct replacement");

        assert_eq!(rename_count, 1, "target is never moved aside");
        assert_eq!(fs::read(&target).expect("read target"), b"work");
        assert!(!backup.exists());
        fs::remove_dir_all(directory).expect("remove scratch");
    }

    #[test]
    fn a_non_destination_conflict_does_not_trigger_backup_fallback() {
        let directory = scratch("direct-failure");
        let target = directory.join("settings.json");
        let temporary = directory.join("settings.json.test.tmp");
        let backup = target.with_extension("json.previous");
        fs::create_dir_all(&directory).expect("create scratch");
        fs::write(&target, b"research").expect("write target");
        fs::write(&temporary, b"work").expect("write temporary");
        let mut rename_count = 0;

        let error = replace_temporary_with_backup(&target, &temporary, &backup, |from, to| {
            rename_count += 1;
            assert_eq!(from, temporary);
            assert_eq!(to, target);
            Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "simulated direct replacement failure",
            ))
        })
        .expect_err("direct replacement fails");

        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(rename_count, 1, "no backup renames are attempted");
        assert_eq!(
            fs::read(&target).expect("old target preserved"),
            b"research"
        );
        assert!(!backup.exists());
        fs::remove_dir_all(directory).expect("remove scratch");
    }

    #[test]
    fn a_completed_write_cleans_a_backup_left_by_an_interrupted_earlier_write() {
        let directory = scratch("recovery");
        let target = directory.join("settings.json");
        let backup = target.with_extension("json.previous");

        write_bytes_with_backup(&target, b"research").expect("first write");
        fs::rename(&target, &backup).expect("simulate interrupted replacement");
        write_bytes_with_backup(&target, b"work").expect("recovery write");

        assert_eq!(fs::read(&target).expect("read target"), b"work");
        assert!(!backup.exists());
        fs::remove_dir_all(directory).expect("remove scratch");
    }

    #[test]
    fn a_failed_replacement_restores_the_previous_document() {
        let directory = scratch("restore");
        let target = directory.join("settings.json");
        let temporary = directory.join("settings.json.test.tmp");
        let backup = target.with_extension("json.previous");
        fs::create_dir_all(&directory).expect("create scratch");
        fs::write(&target, b"research").expect("write target");
        fs::write(&temporary, b"work").expect("write temporary");
        let mut rename_count = 0;

        let error = replace_temporary_with_backup(&target, &temporary, &backup, |from, to| {
            rename_count += 1;
            if rename_count == 1 {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "simulated platform cannot overwrite destination",
                ));
            }
            if rename_count == 3 {
                return Err(io::Error::other("simulated replacement failure"));
            }
            fs::rename(from, to)
        })
        .expect_err("replacement fails");

        assert_eq!(error.kind(), io::ErrorKind::Other);
        assert_eq!(fs::read(&target).expect("restored target"), b"research");
        assert!(!backup.exists());
        assert_eq!(fs::read(&temporary).expect("retained temporary"), b"work");
        fs::remove_dir_all(directory).expect("remove scratch");
    }

    #[test]
    fn temporary_creation_skips_a_name_already_created_by_another_writer() {
        let directory = scratch("collision");
        let target = directory.join("settings.json");
        fs::create_dir_all(&directory).expect("create scratch");
        let collision = directory.join("settings.json.42.7.tmp");
        fs::write(&collision, b"another writer's temp").expect("create collision");
        let mut ids = [7, 8].into_iter();

        let (temporary, mut file) =
            create_temporary_file_with(&target, 42, || ids.next().expect("next test id"))
                .expect("create unique temp");
        file.write_all(b"ours").expect("write our temp");

        assert_eq!(temporary, directory.join("settings.json.42.8.tmp"));
        assert_eq!(
            fs::read(&collision).expect("read collision"),
            b"another writer's temp"
        );
        assert_eq!(fs::read(&temporary).expect("read our temp"), b"ours");
        drop(file);
        fs::remove_dir_all(directory).expect("remove scratch");
    }

    #[test]
    fn atomic_write_failure_preserves_target_and_cleans_staging_file() {
        let directory = scratch("atomic-failure");
        let target = directory.join("receipt.json");
        fs::create_dir_all(&directory).expect("create scratch");
        fs::write(&target, b"old complete receipt").expect("write old target");

        let error = write_bytes_atomic_with(&target, b"new complete receipt", |from, to| {
            assert_eq!(to, target);
            assert!(from.starts_with(&directory));
            assert_eq!(
                fs::read(&target).expect("read before install"),
                b"old complete receipt"
            );
            Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "simulated rename failure",
            ))
        })
        .expect_err("install fails");

        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(
            fs::read(&target).expect("old target remains"),
            b"old complete receipt"
        );
        assert_eq!(
            fs::read_dir(&directory).expect("list scratch").count(),
            1,
            "failed installation removes its staging file"
        );
        fs::remove_dir_all(directory).expect("remove scratch");
    }

    #[test]
    fn atomic_write_exposes_old_contents_until_direct_rename_then_new_contents() {
        let directory = scratch("atomic-visibility");
        let target = directory.join("receipt.json");
        fs::create_dir_all(&directory).expect("create scratch");
        fs::write(&target, b"old complete receipt").expect("write old target");

        write_bytes_atomic_with(&target, b"new complete receipt", |from, to| {
            assert_eq!(to, target);
            assert_eq!(
                fs::read(&target).expect("old file is visible before rename"),
                b"old complete receipt"
            );
            assert_eq!(
                fs::read(from).expect("staged file is complete"),
                b"new complete receipt"
            );
            fs::rename(from, to)
        })
        .expect("install staged file");

        assert_eq!(
            fs::read(&target).expect("new file is visible after rename"),
            b"new complete receipt"
        );
        assert_eq!(
            fs::read_dir(&directory).expect("list scratch").count(),
            1,
            "successful installation consumes the staging file"
        );
        fs::remove_dir_all(directory).expect("remove scratch");
    }

    fn replace(target: &Path, baseline: Option<&[u8]>, contents: &[u8]) -> Replacement {
        replace_bytes_if_unchanged(target, baseline, contents).expect("guarded replacement")
    }

    fn open_lock(target: &Path) -> File {
        OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(target.with_file_name("settings.json.lock"))
            .expect("open lock file")
    }

    #[test]
    fn a_changed_target_is_a_conflict_that_writes_nothing_and_releases_the_lock() {
        let directory = scratch("guard-conflict");
        let target = directory.join("settings.json");
        write_bytes_with_backup(&target, b"loaded").expect("first write");
        write_bytes_with_backup(&target, b"another writer").expect("competing write");

        let outcome = replace(&target, Some(b"loaded"), b"mine");

        assert_eq!(outcome, Replacement::Conflict);
        assert_eq!(fs::read(&target).expect("read target"), b"another writer");
        open_lock(&target)
            .try_lock()
            .expect("a conflict leaves the lock free");
        let mut names: Vec<_> = fs::read_dir(&directory)
            .expect("list scratch")
            .map(|entry| entry.expect("entry").file_name())
            .collect();
        names.sort();
        assert_eq!(
            names,
            ["settings.json", "settings.json.lock"],
            "the refused contents leave no staging file"
        );
        fs::remove_dir_all(directory).expect("remove scratch");
    }

    #[test]
    fn an_unchanged_target_is_replaced_and_a_missing_one_only_while_it_is_missing() {
        let directory = scratch("guard-install");
        let target = directory.join("settings.json");

        assert_eq!(replace(&target, None, b"first"), Replacement::Installed);
        assert_eq!(fs::read(&target).expect("read target"), b"first");
        assert_eq!(
            replace(&target, None, b"second"),
            Replacement::Conflict,
            "the file exists now, so a baseline of absent is stale"
        );
        assert_eq!(
            replace(&target, Some(b"first"), b"second"),
            Replacement::Installed
        );
        assert_eq!(fs::read(&target).expect("read target"), b"second");

        fs::remove_file(&target).expect("remove target");
        assert_eq!(
            replace(&target, Some(b"second"), b"third"),
            Replacement::Conflict,
            "a file deleted since the load is a change"
        );
        assert!(!target.exists());
        fs::remove_dir_all(directory).expect("remove scratch");
    }

    #[test]
    fn a_held_lock_defers_the_compare_until_it_is_released() {
        let directory = scratch("guard-lock");
        let target = directory.join("settings.json");
        write_bytes_with_backup(&target, b"loaded").expect("first write");
        let lock = open_lock(&target);
        lock.lock().expect("take the lock");

        let (sender, receiver) = std::sync::mpsc::channel();
        let writer = {
            let target = target.clone();
            std::thread::spawn(move || {
                let outcome = replace(&target, Some(b"loaded"), b"mine");
                sender.send(outcome).expect("report outcome");
            })
        };

        assert!(
            receiver
                .recv_timeout(std::time::Duration::from_millis(200))
                .is_err(),
            "the writer must wait while another holds the lock"
        );
        assert_eq!(fs::read(&target).expect("read target"), b"loaded");

        lock.unlock().expect("release the lock");
        let outcome = receiver
            .recv_timeout(std::time::Duration::from_secs(10))
            .expect("writer finishes once the lock is free");
        writer.join().expect("writer thread");
        assert_eq!(outcome, Replacement::Installed);
        assert_eq!(fs::read(&target).expect("read target"), b"mine");
        fs::remove_dir_all(directory).expect("remove scratch");
    }
}
