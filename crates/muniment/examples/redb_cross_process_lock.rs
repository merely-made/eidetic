// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! S10: an actual, runtime answer to the one thing the custody-backend
//! pricing report left unverified (`REPORT.md` section 2/3): does redb's
//! write lock serialize two *processes*, or only two handles inside one
//! process? `Backend::transact`'s guarantee (see `src/backend.rs`) that
//! nothing else observes or mutates state between a closure's reads and its
//! writes' commit depends on this holding across processes, not just across
//! tasks.
//!
//! This spawns itself as a child process (`std::process::Command`, not a
//! thread) so the two `redb::Database` handles genuinely live in separate
//! OS processes with separate address spaces, then has both try to open the
//! same database file.
//!
//! Reading redb 2.6.3's own source settles what *should* happen before this
//! runs it: `src/tree_store/page_store/file_backend/windows.rs`'s
//! `FileBackend::new` calls `LockFile` (whole-file, exclusive) on the
//! database file the moment a `Database` is created or opened — not merely
//! around each `begin_write`. A failed `LockFile` call maps to
//! `DatabaseError::DatabaseAlreadyOpen`, and `Drop for FileBackend` calls
//! `UnlockFile`. So the claim to verify is: **opening** a redb database file
//! is exclusive across processes on Windows, for the whole lifetime of the
//! handle, which is strictly stronger protection than "concurrent
//! `begin_write` blocks" — a second process cannot even open the file while
//! the first has it open, transaction or no transaction.
//!
//! Run with the database already built (`cargo build --example
//! redb_cross_process_lock --features redb`), then invoke the built binary
//! directly with no arguments; it drives its own child processes.
use std::env;
use std::path::PathBuf;
use std::process::Command;

const TABLE: ::redb::TableDefinition<&str, &[u8]> = ::redb::TableDefinition::new("muniment");

fn main() {
    let mut args = env::args().skip(1);
    match args.next().as_deref() {
        Some("child") => child(PathBuf::from(args.next().expect("child needs a path arg"))),
        _ => parent(),
    }
}

/// Try to open the database at `path` once and report the outcome. A real
/// second process, invoked by the parent below.
fn child(path: PathBuf) {
    match ::redb::Database::open(&path) {
        Ok(_db) => println!("CHILD: Database::open SUCCEEDED (lock was not held, or not cross-process)"),
        Err(e) => println!("CHILD: Database::open FAILED: {e}"),
    }
}

fn parent() {
    let path = env::temp_dir().join(format!(
        "muniment_cross_process_lock_{}.redb",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&path);

    let db = ::redb::Database::create(&path).expect("parent create");
    // Materialize the table, same as `RedbBackend::open`.
    {
        let txn = db.begin_write().expect("parent begin_write");
        txn.open_table(TABLE).expect("parent open_table");
        txn.commit().expect("parent commit");
    }

    println!("PARENT: holding an open Database handle on {path:?}");
    let exe = env::current_exe().expect("current_exe");

    let status = Command::new(&exe)
        .arg("child")
        .arg(&path)
        .status()
        .expect("spawn first child");
    println!("PARENT: first child (spawned while the parent's handle is open) exited {status}");

    drop(db);
    println!("PARENT: dropped its handle (releases the Windows LockFile)");

    let status = Command::new(&exe)
        .arg("child")
        .arg(&path)
        .status()
        .expect("spawn second child");
    println!("PARENT: second child (spawned after release) exited {status}");

    let _ = std::fs::remove_file(&path);
}
