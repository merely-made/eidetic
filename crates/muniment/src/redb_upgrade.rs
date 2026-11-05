// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Open-time file-format migration for [`RedbBackend`](crate::RedbBackend).
//!
//! redb 2.6 creates v2 files by default and offers `Database::upgrade()` to
//! move a file to format v3, which redb 3.x requires. Upgrading is one-way
//! (redb 2.5 and older cannot open a v3 file) and redb documents no
//! crash-atomicity for it: it rewrites the commit headers in place and marks the
//! handle irrecoverable on an I/O error partway. So a v2 file is copied to
//! `<file>.pre-v3` first, and that copy is the recovery path:
//!
//! - upgrade fails: the backup is renamed back over the file, the open returns
//!   [`StoreError::Unsupported`], and the file is exactly as it was;
//! - the process dies mid-upgrade: the next open finds `<file>.pre-v3` beside
//!   either a v2 file (redb recovers it; a fresh backup is taken and the upgrade
//!   retried), a v3 file (the upgrade finished; the stale backup is removed), or
//!   a file redb cannot open (the open fails with [`StoreError::Corruption`]
//!   naming the backup, which the operator copies back over the file).
//!
//! The backup is taken before the database is opened because redb locks the
//! file for the life of its handle (mandatory on Windows). Another process
//! writing between the copy and the open is therefore not captured; one process
//! per file is already the redb contract.

use std::ffi::OsString;
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use ::redb::{Builder, Database, UpgradeError};

use crate::error::StoreError;

/// What opening a [`RedbBackend`](crate::RedbBackend) did to the file's format.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RedbOpenOutcome {
    /// The file did not exist (or was empty) and was created as format v3.
    Created,
    /// The file was already format v3 and was left alone.
    Current,
    /// The file was upgraded in place to format v3 from `from_format`.
    Upgraded { from_format: u8 },
}

const MAGIC: [u8; 9] = [b'r', b'e', b'd', b'b', 0x1A, 0x0A, 0xA9, 0x0D, 0x0A];
const GOD_BYTE: usize = MAGIC.len();
const PRIMARY_BIT: u8 = 1;
const SLOT_0: usize = 64;
const SLOT_SIZE: usize = 128;

fn io_err(what: &str, err: io::Error) -> StoreError {
    StoreError::Backend(format!("{what}: {err}"))
}

/// The backup path for `path`: the file name with `.pre-v3` appended.
pub fn backup_path(path: &Path) -> PathBuf {
    sibling(path, ".pre-v3")
}

fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut name: OsString = path.file_name().map(OsString::from).unwrap_or_default();
    name.push(suffix);
    path.with_file_name(name)
}

/// The file-format version byte in the primary commit slot, or `None` when the
/// file is absent, empty, or not a redb file (redb then reports it itself).
pub fn file_format(path: &Path) -> Result<Option<u8>, StoreError> {
    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(io_err("read redb header", err)),
    };
    let mut header = [0u8; SLOT_0 + 2 * SLOT_SIZE];
    let mut filled = 0;
    while filled < header.len() {
        match file.read(&mut header[filled..]) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(err) if err.kind() == io::ErrorKind::Interrupted => {},
            Err(err) => return Err(io_err("read redb header", err)),
        }
    }
    if filled < header.len() || header[..MAGIC.len()] != MAGIC {
        return Ok(None);
    }
    let slot = usize::from(header[GOD_BYTE] & PRIMARY_BIT != 0);
    Ok(Some(header[SLOT_0 + slot * SLOT_SIZE]))
}

fn sync_dir(path: &Path) {
    #[cfg(unix)]
    if let Some(dir) = path.parent() {
        let dir = if dir.as_os_str().is_empty() { Path::new(".") } else { dir };
        if let Ok(dir) = File::open(dir) {
            let _ = dir.sync_all();
        }
    }
    #[cfg(not(unix))]
    let _ = path;
}

/// Copy `path` to its backup path durably: stage, sync, then rename into place.
fn write_backup(path: &Path) -> Result<PathBuf, StoreError> {
    let backup = backup_path(path);
    let staged = sibling(path, ".pre-v3.tmp");
    fs::copy(path, &staged).map_err(|e| io_err("back up redb file before upgrade", e))?;
    File::open(&staged)
        .and_then(|f| f.sync_all())
        .map_err(|e| io_err("sync redb upgrade backup", e))?;
    fs::rename(&staged, &backup).map_err(|e| io_err("install redb upgrade backup", e))?;
    sync_dir(&backup);
    Ok(backup)
}

fn upgrade_failure(err: &UpgradeError, backup: &Path, restored: io::Result<()>) -> StoreError {
    let tail = match restored {
        Ok(()) => "the original file was restored from its backup".to_string(),
        Err(e) => format!("restoring the backup failed ({e}); copy {} over the database file", backup.display()),
    };
    StoreError::Unsupported(format!("redb upgrade to file format v3 failed ({err:?}); {tail}"))
}

/// Open `path` as format v3, creating or upgrading as needed. `upgrade` is the
/// upgrade step, injectable so tests can fail it.
pub(crate) fn open_v3(
    path: &Path,
    upgrade: &dyn Fn(&mut Database) -> Result<bool, UpgradeError>,
) -> Result<(Database, RedbOpenOutcome), StoreError> {
    let before = file_format(path)?;
    let backup = backup_path(path);
    let stale_backup = backup.exists();
    let needs_upgrade = matches!(before, Some(v) if v < 3);
    if matches!(before, Some(v) if v > 3) {
        return Err(StoreError::Unsupported(format!(
            "redb file format {} is newer than this muniment understands",
            before.unwrap_or(0)
        )));
    }
    // redb 2.6 asserts, rather than erring, on some damaged files (a truncated
    // file, a corrupt region header), so a panic during open is caught and
    // reported as corruption. Under `panic = "abort"` it still aborts.
    let open = || {
        std::panic::catch_unwind(|| {
            let mut builder = Builder::new();
            builder.create_with_file_format_v3(true);
            builder.create(path)
        })
        .unwrap_or_else(|_| {
            Err(::redb::DatabaseError::Storage(::redb::StorageError::Corrupted(
                "redb panicked while opening the file".into(),
            )))
        })
    };
    let refuse = |err: ::redb::DatabaseError| {
        if matches!(err, ::redb::DatabaseError::Storage(::redb::StorageError::Corrupted(_)))
            && !stale_backup
        {
            return StoreError::Corruption(err.to_string());
        }
        if stale_backup {
            StoreError::Corruption(format!(
                "cannot open redb file ({err}); an interrupted upgrade left {}: copy it over the database file",
                backup.display()
            ))
        } else {
            StoreError::Backend(err.to_string())
        }
    };
    // A backup left by an interrupted upgrade may be the only good copy, so it
    // is replaced only once the current file has been shown to open.
    if needs_upgrade && stale_backup {
        drop(open().map_err(refuse)?);
    }
    // Back up before the open: redb locks the file while the handle lives.
    let made_backup = if needs_upgrade { Some(write_backup(path)?) } else { None };

    let mut db = open().map_err(refuse)?;

    let outcome = if needs_upgrade {
        let backup = made_backup.as_ref().expect("backup exists when upgrading");
        match upgrade(&mut db) {
            Ok(true) => RedbOpenOutcome::Upgraded { from_format: before.unwrap_or(2) },
            Ok(false) => RedbOpenOutcome::Current,
            Err(err) => {
                drop(db);
                let restored = fs::rename(backup, path);
                sync_dir(path);
                return Err(upgrade_failure(&err, backup, restored));
            },
        }
    } else if before.is_some() {
        // The header byte only decides whether to back up; `upgrade` is the
        // authority, and on a v3 file it is a no-op.
        match upgrade(&mut db) {
            Ok(false) => RedbOpenOutcome::Current,
            Ok(true) => RedbOpenOutcome::Upgraded { from_format: 2 },
            Err(err) => {
                return Err(StoreError::Unsupported(format!(
                    "redb upgrade to file format v3 failed ({err:?}); no backup was taken because the header read as v3"
                )));
            },
        }
    } else {
        RedbOpenOutcome::Created
    };

    // The file is v3 and open; any backup is now stale.
    if stale_backup || made_backup.is_some() {
        let _ = fs::remove_file(&backup);
    }
    Ok((db, outcome))
}
