// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! A file in redb 2's format is refused, not migrated.

use std::fs::OpenOptions;
use std::io::{Read, Seek, SeekFrom, Write};

use super::*;

// redb's header: a 9-byte magic number, the god byte (bit 0 picks the primary
// commit slot), then two 128-byte commit slots from offset 64, each opening
// with its file-format version byte.
const GOD_BYTE: u64 = 9;
const SLOT_0: u64 = 64;
const SLOT_SIZE: u64 = 128;

#[test]
fn a_redb_2_format_file_is_refused_as_unreadable() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("store.redb");
    drop(RedbBackend::open(&path).unwrap());

    // Mark both commit slots as format v2, the format redb 2 writes by default.
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&path)
        .unwrap();
    let mut god = [0u8; 1];
    file.seek(SeekFrom::Start(GOD_BYTE)).unwrap();
    file.read_exact(&mut god).unwrap();
    let mut version = [0u8; 1];
    file.seek(SeekFrom::Start(SLOT_0 + u64::from(god[0] & 1) * SLOT_SIZE))
        .unwrap();
    file.read_exact(&mut version).unwrap();
    assert_eq!(version[0], 3, "a new file is redb file format v3");
    for slot in 0..2 {
        file.seek(SeekFrom::Start(SLOT_0 + slot * SLOT_SIZE))
            .unwrap();
        file.write_all(&[2]).unwrap();
    }
    file.sync_all().unwrap();
    drop(file);

    match RedbBackend::open(&path) {
        Err(StoreError::Unsupported(msg)) => {
            assert!(msg.contains("written by redb 2"), "{msg}");
            assert!(msg.contains("not readable"), "{msg}");
        },
        Err(other) => panic!("expected Unsupported for a v2 file, got {other}"),
        Ok(_) => panic!("a v2 file opened"),
    }
}
