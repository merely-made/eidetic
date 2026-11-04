// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! A directory [`Backend`]: each key is a file, so the store is a folder a
//! person can read.
//!
//! Like the [zip backend](crate::ZipBackend), it suits a consumer that names
//! its keys after real files (`sessions/<id>/graph.json`): the folder is then
//! the data, readable without the app that wrote it. Unlike zip it is
//! incremental. A write touches only the files it names, so it also suits
//! logs.
//!
//! **Logs.** A key shaped `<path>.jsonl/<seq>`, where `<seq>` is sixteen
//! lowercase hex digits, is entry `seq` of the file `<path>.jsonl`, one entry
//! per line. That is the shape [`Journal::append_entries`] writes under a
//! `.jsonl` prefix, so appending an entry appends a line. A log is dense: entry
//! `n` exists only after entries `0..n`. A write that would leave a hole is
//! refused, while deleting a log's tail, or all of it, is fine. An entry's
//! value must be one line, with no `\n` or `\r`.
//!
//! **Atomic batches.** [`apply`](Backend::apply) records a batch in a
//! checksummed redo file before changing anything, applies it, then removes
//! the redo file. Opening the store finishes a batch a crash cut short, so a
//! reader sees all of a batch or none of it. A batch that changes one file in
//! one step skips the redo file, since that step is atomic already: a file is
//! replaced by writing a temporary sibling and renaming it over the original,
//! and a single log line is appended in place, where a line a crash left
//! without its newline is dropped because its write never finished. Appending
//! several lines, or changing several files, goes through the redo file. If
//! applying a recorded batch fails part-way, the backend refuses further calls
//! until it is reopened, which completes the batch. Editing the store's files
//! by hand while it is open is unsupported.
//!
//! **One owner.** Opening takes an exclusive lock on `.muniment-lock` in the
//! root and holds it for the backend's lifetime, so a second opener, in this
//! process or another, is refused at once.
//!
//! **Keys** are `/`-separated relative paths whose segments are portable file
//! names: no `.` or `..`, no character Windows forbids, no trailing dot or
//! space, and no reserved device name. A key cannot also be the directory of
//! another key, and keys differing only in case collide on a case-insensitive
//! file system. Names beginning `.muniment` belong to the store.
//!
//! Native only, behind the `directory` feature: it uses filesystem paths.
//!
//! [`Journal::append_entries`]: crate::Journal::append_entries

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::fs::{self, File, OpenOptions, TryLockError};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use crate::backend::{Backend, TransactFn, TransactionReader, WriteOp};
use crate::error::StoreError;

const RESERVED: &str = ".muniment";
const LOCK_FILE: &str = ".muniment-lock";
const REDO_FILE: &str = ".muniment-redo";
const REDO_STAGING_FILE: &str = ".muniment-redo.staging";
const TEMP_PREFIX: &str = ".muniment-tmp-";
const LOG_EXTENSION: &str = ".jsonl";
const REDO_MAGIC: &[u8] = b"muniment-redo/1\n";

/// A durable store in a directory, one file per key; see the module docs.
/// Cheap to clone: clones share one handle, one lock and one write lock.
#[derive(Clone)]
pub struct DirectoryBackend {
    inner: Arc<Inner>,
}

struct Inner {
    root: PathBuf,
    /// Held for the backend's lifetime; the lock is what refuses a second owner.
    _lock: File,
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    logs: LogCache,
    /// Set when a recorded batch failed part-way; cleared only by reopening.
    poisoned: Option<String>,
}

/// Where each log file's lines start, loaded on first use.
#[derive(Default)]
struct LogCache(HashMap<PathBuf, LogIndex>);

#[derive(Default)]
struct LogIndex {
    starts: Vec<u64>,
    /// One past the last complete line's newline.
    end: u64,
    /// Kept open so reading a long log entry by entry does not reopen the
    /// file each time. Dropped before the file is renamed over or removed.
    reader: Option<File>,
}

impl LogIndex {
    fn len(&self) -> u64 {
        self.starts.len() as u64
    }

    /// Entry `index`'s bytes, without its newline.
    fn span(&self, index: u64) -> Option<(u64, u64)> {
        let at = usize::try_from(index).ok()?;
        let start = *self.starts.get(at)?;
        let next = self.starts.get(at + 1).copied().unwrap_or(self.end);
        Some((start, next - 1))
    }

    fn push(&mut self, line_len: u64) {
        self.starts.push(self.end);
        self.end += line_len + 1;
    }
}

/// What a key names on disk.
enum Target {
    File(PathBuf),
    Entry { log: PathBuf, index: u64 },
}

/// A batch's effect on each file, worked out before anything is written.
enum LogEffect {
    Unchanged,
    Append(Vec<Vec<u8>>),
    Truncate(u64),
    Rewrite(Vec<Vec<u8>>),
    Remove,
}

struct Effects {
    files: BTreeMap<PathBuf, Option<Vec<u8>>>,
    logs: Vec<(PathBuf, LogEffect)>,
}

impl Effects {
    /// Whether the batch needs the redo file to land whole. One file changes
    /// atomically on its own (a rename, a truncation, a removal, or a one-line
    /// append whose torn line is dropped), but a crash can stop a multi-line
    /// append after some of its lines.
    fn needs_redo(&self) -> bool {
        let changed = self
            .logs
            .iter()
            .filter(|(_, effect)| !matches!(effect, LogEffect::Unchanged));
        let multi_line = changed
            .clone()
            .any(|(_, effect)| matches!(effect, LogEffect::Append(lines) if lines.len() > 1));
        self.files.len() + changed.count() > 1 || multi_line
    }
}

fn io_error(path: &Path, error: io::Error) -> StoreError {
    StoreError::Backend(format!("{}: {error}", path.display()))
}

fn bad_key(key: &str, why: &str) -> StoreError {
    StoreError::Backend(format!("key {key:?} {why}"))
}

fn is_entry_segment(segment: &str) -> bool {
    segment.len() == 16
        && segment
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

fn segment_problem(segment: &str) -> Option<&'static str> {
    if segment.is_empty() || segment == "." || segment == ".." {
        return Some("has an empty, `.` or `..` segment");
    }
    if segment.starts_with(RESERVED) {
        return Some("uses a name the store reserves (`.muniment…`)");
    }
    if segment
        .chars()
        .any(|c| c.is_control() || matches!(c, '<' | '>' | ':' | '"' | '\\' | '|' | '?' | '*'))
    {
        return Some("holds a character some file systems forbid");
    }
    if segment.ends_with('.') || segment.ends_with(' ') {
        return Some("has a segment ending in a dot or a space");
    }
    let stem = segment
        .split('.')
        .next()
        .unwrap_or(segment)
        .to_ascii_uppercase();
    let device = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.as_bytes()[3].is_ascii_digit());
    device.then_some("names a reserved device")
}

fn remove_if_present(path: &Path) -> Result<(), StoreError> {
    match fs::remove_file(path) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => Err(io_error(path, error)),
        _ => Ok(()),
    }
}

/// Make a rename or a new file durable. Windows has no directory handle to
/// sync; its metadata journal covers the same ground.
#[cfg(unix)]
fn sync_dir(dir: &Path) -> Result<(), StoreError> {
    File::open(dir)
        .and_then(|handle| handle.sync_all())
        .map_err(|error| io_error(dir, error))
}

#[cfg(not(unix))]
fn sync_dir(_dir: &Path) -> Result<(), StoreError> {
    Ok(())
}

/// Replace `path` with `bytes` through a temporary sibling and a rename.
fn replace_file(path: &Path, bytes: &[u8]) -> Result<(), StoreError> {
    let parent = path.parent().expect("a key's file has a parent");
    fs::create_dir_all(parent).map_err(|error| io_error(parent, error))?;
    let name = path.file_name().expect("a key's file has a name");
    let temp = parent.join(format!("{TEMP_PREFIX}{}", name.to_string_lossy()));
    let mut file = File::create(&temp).map_err(|error| io_error(&temp, error))?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|error| io_error(&temp, error))?;
    drop(file);
    fs::rename(&temp, path).map_err(|error| io_error(path, error))?;
    sync_dir(parent)
}

/// Remove directories left empty below `root`, from `dir` upward.
fn prune_empty(root: &Path, mut dir: &Path) {
    while dir != root && dir.starts_with(root) {
        if fs::remove_dir(dir).is_err() {
            break;
        }
        match dir.parent() {
            Some(parent) => dir = parent,
            None => break,
        }
    }
}

fn load_index(log: &Path) -> Result<LogIndex, StoreError> {
    let bytes = match fs::read(log) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(LogIndex::default()),
        Err(error) => return Err(io_error(log, error)),
    };
    let mut index = LogIndex::default();
    for (at, byte) in bytes.iter().enumerate() {
        if *byte == b'\n' {
            index.push(at as u64 - index.end);
        }
    }
    if bytes.len() as u64 > index.end {
        // A line without its newline: the append it belonged to never finished.
        let file = OpenOptions::new()
            .write(true)
            .open(log)
            .map_err(|error| io_error(log, error))?;
        file.set_len(index.end)
            .and_then(|()| file.sync_all())
            .map_err(|error| io_error(log, error))?;
    }
    Ok(index)
}

impl LogCache {
    fn index(&mut self, log: &Path) -> Result<&mut LogIndex, StoreError> {
        if !self.0.contains_key(log) {
            let index = load_index(log)?;
            self.0.insert(log.to_path_buf(), index);
        }
        Ok(self.0.get_mut(log).expect("just inserted"))
    }

    /// Entry `seq` of `log`, or `None` past its end.
    fn read(&mut self, log: &Path, seq: u64) -> Result<Option<Vec<u8>>, StoreError> {
        let index = self.index(log)?;
        let Some((start, end)) = index.span(seq) else {
            return Ok(None);
        };
        if index.reader.is_none() {
            index.reader = Some(File::open(log).map_err(|error| io_error(log, error))?);
        }
        let reader = index.reader.as_mut().expect("just opened");
        let mut bytes = vec![0; (end - start) as usize];
        reader
            .seek(SeekFrom::Start(start))
            .and_then(|_| reader.read_exact(&mut bytes))
            .map_err(|error| io_error(log, error))?;
        Ok(Some(bytes))
    }

    /// Drop what is cached for `log`, its read handle included.
    fn forget(&mut self, log: &Path) {
        self.0.remove(log);
    }
}

fn encode_redo(ops: &[WriteOp]) -> Vec<u8> {
    fn push(out: &mut Vec<u8>, bytes: &[u8]) {
        out.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
        out.extend_from_slice(bytes);
    }
    let mut out = REDO_MAGIC.to_vec();
    out.extend_from_slice(&(ops.len() as u64).to_le_bytes());
    for op in ops {
        match op {
            WriteOp::Put { key, value } => {
                out.push(1);
                push(&mut out, key.as_bytes());
                push(&mut out, value);
            }
            WriteOp::Delete { key } => {
                out.push(2);
                push(&mut out, key.as_bytes());
            }
        }
    }
    let hash = blake3::hash(&out);
    out.extend_from_slice(hash.as_bytes());
    out
}

fn decode_redo(bytes: &[u8]) -> Option<Vec<WriteOp>> {
    fn take<'a>(rest: &mut &'a [u8], len: usize) -> Option<&'a [u8]> {
        if rest.len() < len {
            return None;
        }
        let (head, tail) = rest.split_at(len);
        *rest = tail;
        Some(head)
    }
    fn number(rest: &mut &[u8]) -> Option<u64> {
        Some(u64::from_le_bytes(take(rest, 8)?.try_into().ok()?))
    }
    fn field<'a>(rest: &mut &'a [u8]) -> Option<&'a [u8]> {
        let len = usize::try_from(number(rest)?).ok()?;
        take(rest, len)
    }

    let body_len = bytes.len().checked_sub(blake3::OUT_LEN)?;
    let (body, hash) = bytes.split_at(body_len);
    if blake3::hash(body).as_bytes() != hash {
        return None;
    }
    let mut rest = body.strip_prefix(REDO_MAGIC)?;
    let count = number(&mut rest)?;
    let mut ops = Vec::new();
    for _ in 0..count {
        let tag = take(&mut rest, 1)?[0];
        let key = String::from_utf8(field(&mut rest)?.to_vec()).ok()?;
        ops.push(match tag {
            1 => WriteOp::Put {
                key,
                value: field(&mut rest)?.to_vec(),
            },
            2 => WriteOp::Delete { key },
            _ => return None,
        });
    }
    rest.is_empty().then_some(ops)
}

impl DirectoryBackend {
    /// Open the store in `root`, creating the directory if it is missing.
    ///
    /// Refused at once while another handle holds the store. A batch a crash
    /// cut short is finished before this returns.
    pub fn open(root: impl AsRef<Path>) -> Result<Self, StoreError> {
        let root = root.as_ref().to_path_buf();
        fs::create_dir_all(&root).map_err(|error| io_error(&root, error))?;
        let lock_path = root.join(LOCK_FILE);
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)
            .map_err(|error| io_error(&lock_path, error))?;
        match lock.try_lock() {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => {
                return Err(StoreError::Backend(format!(
                    "{} is already open in another handle or process",
                    root.display()
                )));
            }
            Err(TryLockError::Error(error)) => return Err(io_error(&lock_path, error)),
        }
        let inner = Inner {
            root,
            _lock: lock,
            state: Mutex::new(State::default()),
        };
        inner.recover()?;
        Ok(Self {
            inner: Arc::new(inner),
        })
    }

    /// The directory this store lives in.
    pub fn root(&self) -> &Path {
        &self.inner.root
    }
}

/// A transaction's view: reads go straight to disk, under the write lock.
struct Reader<'a> {
    inner: &'a Inner,
    logs: RefCell<&'a mut LogCache>,
}

impl TransactionReader for Reader<'_> {
    fn get(&self, key: &str) -> Result<Option<Vec<u8>>, StoreError> {
        let mut logs = self.logs.borrow_mut();
        self.inner.get(&mut logs, key)
    }

    fn list(&self, prefix: &str) -> Result<Vec<String>, StoreError> {
        let mut logs = self.logs.borrow_mut();
        self.inner.keys(&mut logs, prefix)
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl Backend for DirectoryBackend {
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, StoreError> {
        let mut state = self.inner.locked()?;
        self.inner.get(&mut state.logs, key)
    }

    async fn put(&self, key: &str, bytes: &[u8]) -> Result<(), StoreError> {
        let op = WriteOp::Put {
            key: key.to_string(),
            value: bytes.to_vec(),
        };
        let mut state = self.inner.locked()?;
        self.inner.apply(&mut state, std::slice::from_ref(&op))
    }

    async fn delete(&self, key: &str) -> Result<(), StoreError> {
        let op = WriteOp::Delete {
            key: key.to_string(),
        };
        let mut state = self.inner.locked()?;
        self.inner.apply(&mut state, std::slice::from_ref(&op))
    }

    async fn list(&self, prefix: &str) -> Result<Vec<String>, StoreError> {
        let mut state = self.inner.locked()?;
        self.inner.keys(&mut state.logs, prefix)
    }

    async fn scan(&self, start: &str, end: &str) -> Result<Vec<String>, StoreError> {
        if start >= end {
            return Ok(Vec::new());
        }
        let common = start
            .char_indices()
            .zip(end.chars())
            .find(|((_, a), b)| a != b)
            .map_or(start.len().min(end.len()), |((at, _), _)| at);
        let mut state = self.inner.locked()?;
        let mut keys = self.inner.keys(&mut state.logs, &start[..common])?;
        keys.retain(|key| key.as_str() >= start && key.as_str() < end);
        keys.sort();
        Ok(keys)
    }

    /// Atomic across files through the redo file; see the module docs.
    async fn apply(&self, ops: &[WriteOp]) -> Result<(), StoreError> {
        let mut state = self.inner.locked()?;
        self.inner.apply(&mut state, ops)
    }

    /// Honest under the write lock: it is held from `f`'s reads through the
    /// writes it returns, so no other call lands between them.
    async fn transact(&self, f: TransactFn) -> Result<(), StoreError> {
        let mut state = self.inner.locked()?;
        let ops = {
            let reader = Reader {
                inner: &self.inner,
                logs: RefCell::new(&mut state.logs),
            };
            f(&reader)
        };
        self.inner.apply(&mut state, &ops)
    }
}

#[cfg(test)]
mod tests;

mod transaction;
