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
            },
            WriteOp::Delete { key } => {
                out.push(2);
                push(&mut out, key.as_bytes());
            },
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
            Ok(()) => {},
            Err(TryLockError::WouldBlock) => {
                return Err(StoreError::Backend(format!(
                    "{} is already open in another handle or process",
                    root.display()
                )));
            },
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

impl Inner {
    fn locked(&self) -> Result<std::sync::MutexGuard<'_, State>, StoreError> {
        let state = self.state.lock().expect("directory backend lock poisoned");
        match &state.poisoned {
            Some(why) => Err(StoreError::Backend(why.clone())),
            None => Ok(state),
        }
    }

    fn target(&self, key: &str) -> Result<Target, StoreError> {
        let segments: Vec<&str> = key.split('/').collect();
        if let Some(why) = segments.iter().find_map(|segment| segment_problem(segment)) {
            return Err(bad_key(key, why));
        }
        let join = |segments: &[&str]| {
            segments
                .iter()
                .fold(self.root.clone(), |path, segment| path.join(segment))
        };
        let last = segments.len() - 1;
        let log_at = segments
            .iter()
            .position(|segment| segment.ends_with(LOG_EXTENSION));
        match log_at {
            None => Ok(Target::File(join(&segments))),
            Some(at) if at + 1 == last && is_entry_segment(segments[last]) => Ok(Target::Entry {
                log: join(&segments[..last]),
                index: u64::from_str_radix(segments[last], 16).expect("checked hex"),
            }),
            Some(_) => Err(bad_key(
                key,
                "names a `.jsonl` log; its keys are `<log>.jsonl/<sixteen hex digits>`",
            )),
        }
    }

    fn recover(&self) -> Result<(), StoreError> {
        // Staged but never renamed into place: its batch was never begun.
        remove_if_present(&self.root.join(REDO_STAGING_FILE))?;
        let redo = self.root.join(REDO_FILE);
        let bytes = match fs::read(&redo) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(io_error(&redo, error)),
        };
        let ops = decode_redo(&bytes).ok_or_else(|| {
            StoreError::Backend(format!(
                "{}: an interrupted batch's redo file is damaged; nothing was changed",
                redo.display()
            ))
        })?;
        let mut state = self.state.lock().expect("directory backend lock poisoned");
        let effects = self.effects(&mut state.logs, &ops)?;
        self.perform(&mut state.logs, effects)?;
        fs::remove_file(&redo).map_err(|error| io_error(&redo, error))?;
        sync_dir(&self.root)
    }

    fn get(&self, logs: &mut LogCache, key: &str) -> Result<Option<Vec<u8>>, StoreError> {
        match self.target(key)? {
            Target::File(path) => {
                if path.is_dir() {
                    return Ok(None);
                }
                match fs::read(&path) {
                    Ok(bytes) => Ok(Some(bytes)),
                    Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
                    Err(error) => Err(io_error(&path, error)),
                }
            },
            Target::Entry { log, index } => logs.read(&log, index),
        }
    }

    /// Every key beginning with `prefix`, in no particular order.
    fn keys(&self, logs: &mut LogCache, prefix: &str) -> Result<Vec<String>, StoreError> {
        // Walk only below the deepest directory the prefix spells out.
        let dir = prefix.rfind('/').map_or("", |at| &prefix[..at]);
        let mut keys = Vec::new();
        if dir
            .split('/')
            .all(|segment| segment.is_empty() || segment_problem(segment).is_none())
        {
            let path = dir
                .split('/')
                .filter(|segment| !segment.is_empty())
                .fold(self.root.clone(), |path, segment| path.join(segment));
            if dir.ends_with(LOG_EXTENSION) && path.is_file() {
                self.push_log_keys(logs, &path, dir, &mut keys)?;
            } else if path.is_dir() {
                self.walk(logs, &path, dir, &mut keys)?;
            }
        }
        keys.retain(|key| key.starts_with(prefix));
        Ok(keys)
    }

    fn walk(
        &self,
        logs: &mut LogCache,
        dir: &Path,
        rel: &str,
        keys: &mut Vec<String>,
    ) -> Result<(), StoreError> {
        for entry in fs::read_dir(dir).map_err(|error| io_error(dir, error))? {
            let entry = entry.map_err(|error| io_error(dir, error))?;
            let name = entry.file_name();
            // A name that is not UTF-8, or that the store reserves, is no key.
            let Some(name) = name.to_str() else { continue };
            if name.starts_with(RESERVED) {
                continue;
            }
            let key = if rel.is_empty() {
                name.to_string()
            } else {
                format!("{rel}/{name}")
            };
            let path = entry.path();
            if path.is_dir() {
                self.walk(logs, &path, &key, keys)?;
            } else if name.ends_with(LOG_EXTENSION) {
                self.push_log_keys(logs, &path, &key, keys)?;
            } else {
                keys.push(key);
            }
        }
        Ok(())
    }

    fn push_log_keys(
        &self,
        logs: &mut LogCache,
        log: &Path,
        key: &str,
        keys: &mut Vec<String>,
    ) -> Result<(), StoreError> {
        let len = logs.index(log)?.len();
        keys.extend((0..len).map(|seq| format!("{key}/{seq:016x}")));
        Ok(())
    }

    /// Refuse a path whose place on disk is taken: an ancestor that is a file,
    /// or the path itself a directory.
    fn check_placeable(&self, path: &Path) -> Result<(), StoreError> {
        let taken = |path: &Path| {
            StoreError::Backend(format!(
                "{}: a key cannot also be the directory of another key",
                path.display()
            ))
        };
        if path.is_dir() {
            return Err(taken(path));
        }
        let mut ancestor = path.parent();
        while let Some(dir) = ancestor {
            if dir == self.root {
                break;
            }
            if dir.is_file() {
                return Err(taken(dir));
            }
            ancestor = dir.parent();
        }
        Ok(())
    }

    /// Work out a batch's effect on every file it touches, refusing it before
    /// anything is written if any part is invalid.
    fn effects(&self, logs: &mut LogCache, ops: &[WriteOp]) -> Result<Effects, StoreError> {
        let mut files = BTreeMap::new();
        let mut entries: BTreeMap<PathBuf, BTreeMap<u64, Option<Vec<u8>>>> = BTreeMap::new();
        for op in ops {
            let (key, value) = match op {
                WriteOp::Put { key, value } => (key, Some(value)),
                WriteOp::Delete { key } => (key, None),
            };
            match self.target(key)? {
                Target::File(path) => {
                    files.insert(path, value.cloned());
                },
                Target::Entry { log, index } => {
                    if value.is_some_and(|value| value.contains(&b'\n') || value.contains(&b'\r')) {
                        return Err(bad_key(
                            key,
                            "is a log entry, and its value is not one line",
                        ));
                    }
                    entries
                        .entry(log)
                        .or_default()
                        .insert(index, value.cloned());
                },
            }
        }
        let planned: Vec<&PathBuf> = files.keys().chain(entries.keys()).collect();
        for (path, value) in &files {
            if value.is_some() {
                self.check_placeable(path)?;
                if let Some(inside) = planned
                    .iter()
                    .find(|other| other.starts_with(path) && **other != path)
                {
                    return Err(StoreError::Backend(format!(
                        "{}: a key cannot also be the directory of {}",
                        path.display(),
                        inside.display()
                    )));
                }
            }
        }
        let mut log_effects = Vec::new();
        for (log, changes) in entries {
            self.check_placeable(&log)?;
            let effect = self.log_effect(logs, &log, &changes)?;
            log_effects.push((log, effect));
        }
        Ok(Effects {
            files,
            logs: log_effects,
        })
    }

    fn log_effect(
        &self,
        logs: &mut LogCache,
        log: &Path,
        changes: &BTreeMap<u64, Option<Vec<u8>>>,
    ) -> Result<LogEffect, StoreError> {
        let hole = || {
            StoreError::Backend(format!(
                "{}: a log is dense; this batch would leave a missing entry",
                log.display()
            ))
        };
        let len = logs.index(log)?.len();
        let keep = changes
            .iter()
            .find(|(seq, value)| **seq < len && value.is_none())
            .map_or(len, |(seq, _)| *seq);
        let removed = changes
            .iter()
            .filter(|(seq, value)| **seq < len && value.is_none())
            .count() as u64;
        let appended: Vec<(u64, &Vec<u8>)> = changes
            .iter()
            .filter(|(seq, _)| **seq >= len)
            .filter_map(|(seq, value)| Some((*seq, value.as_ref()?)))
            .collect();
        // Everything from `keep` on must go, and nothing may follow it.
        if removed != len - keep || (keep < len && !appended.is_empty()) {
            return Err(hole());
        }
        if appended
            .iter()
            .enumerate()
            .any(|(offset, (seq, _))| *seq != len + offset as u64)
        {
            return Err(hole());
        }
        let mut rewritten = Vec::new();
        for (seq, value) in changes.range(..keep) {
            let Some(value) = value else { continue };
            if logs.read(log, *seq)?.as_ref() != Some(value) {
                rewritten.push((*seq, value.clone()));
            }
        }
        let final_len = keep + appended.len() as u64;
        if final_len == 0 {
            return Ok(if len == 0 {
                LogEffect::Unchanged
            } else {
                LogEffect::Remove
            });
        }
        if !rewritten.is_empty() {
            let mut lines = Vec::with_capacity(final_len as usize);
            for seq in 0..keep {
                lines.push(logs.read(log, seq)?.expect("an entry below keep"));
            }
            for (seq, value) in rewritten {
                lines[seq as usize] = value;
            }
            lines.extend(appended.into_iter().map(|(_, value)| value.clone()));
            return Ok(LogEffect::Rewrite(lines));
        }
        if keep < len {
            return Ok(LogEffect::Truncate(keep));
        }
        if appended.is_empty() {
            return Ok(LogEffect::Unchanged);
        }
        Ok(LogEffect::Append(
            appended
                .into_iter()
                .map(|(_, value)| value.clone())
                .collect(),
        ))
    }

    fn perform(&self, logs: &mut LogCache, effects: Effects) -> Result<(), StoreError> {
        for (path, value) in effects.files {
            match value {
                Some(bytes) => replace_file(&path, &bytes)?,
                None => {
                    if path.is_file() {
                        fs::remove_file(&path).map_err(|error| io_error(&path, error))?;
                        prune_empty(
                            &self.root,
                            path.parent().expect("a key's file has a parent"),
                        );
                    }
                },
            }
        }
        for (log, effect) in effects.logs {
            self.perform_log(logs, &log, effect)?;
        }
        Ok(())
    }

    fn perform_log(
        &self,
        logs: &mut LogCache,
        log: &Path,
        effect: LogEffect,
    ) -> Result<(), StoreError> {
        match effect {
            LogEffect::Unchanged => {},
            LogEffect::Append(lines) => {
                let parent = log.parent().expect("a log file has a parent");
                fs::create_dir_all(parent).map_err(|error| io_error(parent, error))?;
                let created = !log.exists();
                let mut bytes = Vec::new();
                for line in &lines {
                    bytes.extend_from_slice(line);
                    bytes.push(b'\n');
                }
                let mut file = OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(log)
                    .map_err(|error| io_error(log, error))?;
                file.write_all(&bytes)
                    .and_then(|()| file.sync_data())
                    .map_err(|error| io_error(log, error))?;
                if created {
                    sync_dir(parent)?;
                }
                let index = logs.index(log)?;
                for line in &lines {
                    index.push(line.len() as u64);
                }
            },
            LogEffect::Truncate(keep) => {
                let index = logs.index(log)?;
                let end = index.starts[keep as usize];
                let file = OpenOptions::new()
                    .write(true)
                    .open(log)
                    .map_err(|error| io_error(log, error))?;
                file.set_len(end)
                    .and_then(|()| file.sync_all())
                    .map_err(|error| io_error(log, error))?;
                index.starts.truncate(keep as usize);
                index.end = end;
            },
            LogEffect::Rewrite(lines) => {
                let mut bytes = Vec::new();
                let mut rebuilt = LogIndex::default();
                for line in &lines {
                    bytes.extend_from_slice(line);
                    bytes.push(b'\n');
                    rebuilt.push(line.len() as u64);
                }
                logs.forget(log);
                replace_file(log, &bytes)?;
                logs.0.insert(log.to_path_buf(), rebuilt);
            },
            LogEffect::Remove => {
                logs.forget(log);
                fs::remove_file(log).map_err(|error| io_error(log, error))?;
                prune_empty(&self.root, log.parent().expect("a log file has a parent"));
            },
        }
        Ok(())
    }

    /// Apply `ops` all together. Called with the write lock held.
    fn apply(&self, state: &mut State, ops: &[WriteOp]) -> Result<(), StoreError> {
        let effects = self.effects(&mut state.logs, ops)?;
        if !effects.needs_redo() {
            return self.perform(&mut state.logs, effects);
        }
        let staging = self.root.join(REDO_STAGING_FILE);
        let redo = self.root.join(REDO_FILE);
        let mut file = File::create(&staging).map_err(|error| io_error(&staging, error))?;
        file.write_all(&encode_redo(ops))
            .and_then(|()| file.sync_all())
            .map_err(|error| io_error(&staging, error))?;
        drop(file);
        fs::rename(&staging, &redo).map_err(|error| io_error(&redo, error))?;
        sync_dir(&self.root)?;
        if let Err(error) = self.perform(&mut state.logs, effects) {
            state.poisoned = Some(format!(
                "{}: a batch failed part-way ({error}); reopen the store to finish it",
                self.root.display()
            ));
            return Err(error);
        }
        fs::remove_file(&redo).map_err(|error| io_error(&redo, error))?;
        sync_dir(&self.root)
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
mod tests {
    use super::*;

    fn store() -> (tempfile::TempDir, DirectoryBackend) {
        let dir = tempfile::tempdir().unwrap();
        let backend = DirectoryBackend::open(dir.path().join("store")).unwrap();
        (dir, backend)
    }

    fn put(key: &str, value: &str) -> WriteOp {
        WriteOp::Put {
            key: key.to_string(),
            value: value.as_bytes().to_vec(),
        }
    }

    fn delete(key: &str) -> WriteOp {
        WriteOp::Delete {
            key: key.to_string(),
        }
    }

    fn entry(log: &str, seq: u64) -> String {
        format!("{log}/{seq:016x}")
    }

    #[test]
    fn a_key_is_a_file_holding_its_value() {
        pollster::block_on(async {
            let (_dir, store) = store();
            store
                .put("sessions/a/graph.json", b"{\"n\":1}")
                .await
                .unwrap();
            let on_disk = store.root().join("sessions").join("a").join("graph.json");
            assert_eq!(fs::read(&on_disk).unwrap(), b"{\"n\":1}");
            assert_eq!(
                store.get("sessions/a/graph.json").await.unwrap(),
                Some(b"{\"n\":1}".to_vec())
            );
            assert_eq!(store.get("sessions/a/facets.json").await.unwrap(), None);
            // A directory holds keys; it is not one.
            assert_eq!(store.get("sessions/a").await.unwrap(), None);

            store.delete("sessions/a/graph.json").await.unwrap();
            assert_eq!(store.get("sessions/a/graph.json").await.unwrap(), None);
            assert!(
                !store.root().join("sessions").exists(),
                "empty directories are pruned"
            );
            store.delete("sessions/a/graph.json").await.unwrap();
        });
    }

    #[test]
    fn a_log_is_one_file_with_a_line_per_entry() {
        pollster::block_on(async {
            let (_dir, store) = store();
            let log = "s/journal.jsonl";
            store.put(&entry(log, 0), b"{\"a\":0}").await.unwrap();
            store
                .apply(&[
                    put(&entry(log, 1), "{\"a\":1}"),
                    put(&entry(log, 2), "{\"a\":2}"),
                ])
                .await
                .unwrap();
            let path = store.root().join("s").join("journal.jsonl");
            assert_eq!(
                fs::read_to_string(&path).unwrap(),
                "{\"a\":0}\n{\"a\":1}\n{\"a\":2}\n"
            );
            assert_eq!(
                store.get(&entry(log, 1)).await.unwrap(),
                Some(b"{\"a\":1}".to_vec())
            );
            assert_eq!(store.get(&entry(log, 3)).await.unwrap(), None);
            assert_eq!(
                store
                    .scan("s/journal.jsonl/", "s/journal.jsonl0")
                    .await
                    .unwrap(),
                [entry(log, 0), entry(log, 1), entry(log, 2)]
            );
        });
    }

    #[test]
    fn a_log_stays_dense() {
        pollster::block_on(async {
            let (_dir, store) = store();
            let log = "journal.jsonl";
            for seq in 0..3 {
                store.put(&entry(log, seq), b"x").await.unwrap();
            }
            let gap = store.put(&entry(log, 4), b"x").await.unwrap_err();
            assert!(gap.to_string().contains("dense"), "{gap}");
            let middle = store.delete(&entry(log, 1)).await.unwrap_err();
            assert!(middle.to_string().contains("dense"), "{middle}");

            // Rewriting an entry in place and trimming the tail are fine.
            store.put(&entry(log, 1), b"y").await.unwrap();
            store.delete(&entry(log, 2)).await.unwrap();
            assert_eq!(
                fs::read_to_string(store.root().join(log)).unwrap(),
                "x\ny\n"
            );

            // Deleting every entry removes the file.
            store
                .apply(&[delete(&entry(log, 0)), delete(&entry(log, 1))])
                .await
                .unwrap();
            assert!(!store.root().join(log).exists());
        });
    }

    #[test]
    fn keys_must_be_portable_file_names() {
        pollster::block_on(async {
            let (_dir, store) = store();
            for key in [
                "",
                "../escape",
                "a//b",
                "a/./b",
                "con",
                "aux.json",
                "com1",
                "a:b",
                "a\\b",
                "trailing.",
                ".muniment-lock",
                "plain.jsonl",
                "a.jsonl/nothex",
                "a.jsonl/0000000000000000/deeper",
            ] {
                assert!(
                    store.put(key, b"v").await.is_err(),
                    "{key:?} should be refused"
                );
            }
            assert!(
                store
                    .put("a.jsonl/0000000000000000", b"two\nlines")
                    .await
                    .is_err()
            );
        });
    }

    #[test]
    fn a_key_cannot_also_be_a_directory() {
        pollster::block_on(async {
            let (_dir, store) = store();
            store.put("a", b"file").await.unwrap();
            assert!(store.put("a/b", b"inside").await.is_err());
            assert!(
                store
                    .apply(&[put("x", "file"), put("x/y", "inside")])
                    .await
                    .is_err()
            );
            assert_eq!(
                store.get("x").await.unwrap(),
                None,
                "a refused batch writes nothing"
            );
        });
    }

    #[test]
    fn list_and_scan_see_files_and_log_entries() {
        pollster::block_on(async {
            let (_dir, store) = store();
            store
                .apply(&[
                    put("sessions/a/graph.json", "g"),
                    put("sessions/a/journal.jsonl/0000000000000000", "0"),
                    put("sessions/a/journal.jsonl/0000000000000001", "1"),
                    put("sessions/b/graph.json", "g"),
                    put("top", "t"),
                ])
                .await
                .unwrap();
            let mut listed = store.list("sessions/a/").await.unwrap();
            listed.sort();
            assert_eq!(
                listed,
                [
                    "sessions/a/graph.json",
                    "sessions/a/journal.jsonl/0000000000000000",
                    "sessions/a/journal.jsonl/0000000000000001",
                ]
            );
            assert_eq!(store.list("").await.unwrap().len(), 5);
            assert_eq!(
                store.scan("sessions/a/", "sessions/b/").await.unwrap(),
                listed
            );
            assert!(store.list("../").await.unwrap().is_empty());
        });
    }

    #[test]
    fn a_second_owner_is_refused_until_the_first_lets_go() {
        let dir = tempfile::tempdir().unwrap();
        let first = DirectoryBackend::open(dir.path()).unwrap();
        let refused = DirectoryBackend::open(dir.path())
            .err()
            .expect("the store is held");
        assert!(refused.to_string().contains("already open"), "{refused}");
        drop(first);
        DirectoryBackend::open(dir.path()).unwrap();
    }

    #[test]
    fn a_batch_a_crash_cut_short_is_finished_on_open() {
        pollster::block_on(async {
            let dir = tempfile::tempdir().unwrap();
            let log = "s/journal.jsonl";
            {
                let store = DirectoryBackend::open(dir.path()).unwrap();
                store.put(&entry(log, 0), b"0").await.unwrap();
            }
            // The crash: the batch was recorded, and only its first file written.
            let batch = [
                put("s/graph.json", "checkpoint"),
                put(&entry(log, 1), "1"),
                put(&entry(log, 2), "2"),
            ];
            fs::write(dir.path().join(REDO_FILE), encode_redo(&batch)).unwrap();
            fs::write(dir.path().join("s").join("graph.json"), "checkpoint").unwrap();
            // ...and entry 1 torn mid-append.
            let mut journal = OpenOptions::new()
                .append(true)
                .open(dir.path().join("s").join("journal.jsonl"))
                .unwrap();
            journal.write_all(b"1").unwrap();
            drop(journal);

            let store = DirectoryBackend::open(dir.path()).unwrap();
            assert!(!dir.path().join(REDO_FILE).exists());
            assert_eq!(
                store.get("s/graph.json").await.unwrap(),
                Some(b"checkpoint".to_vec())
            );
            assert_eq!(
                fs::read_to_string(dir.path().join("s").join("journal.jsonl")).unwrap(),
                "0\n1\n2\n"
            );
        });
    }

    #[test]
    fn a_batch_staged_but_never_recorded_is_dropped() {
        pollster::block_on(async {
            let dir = tempfile::tempdir().unwrap();
            fs::write(
                dir.path().join(REDO_STAGING_FILE),
                encode_redo(&[put("a", "1")]),
            )
            .unwrap();
            let store = DirectoryBackend::open(dir.path()).unwrap();
            assert_eq!(store.get("a").await.unwrap(), None);
            assert!(!dir.path().join(REDO_STAGING_FILE).exists());
        });
    }

    #[test]
    fn a_damaged_redo_file_refuses_the_open() {
        let dir = tempfile::tempdir().unwrap();
        let mut bytes = encode_redo(&[put("a", "1"), put("b", "2")]);
        bytes[REDO_MAGIC.len() + 9] ^= 0xff;
        fs::write(dir.path().join(REDO_FILE), bytes).unwrap();
        let refused = DirectoryBackend::open(dir.path())
            .err()
            .expect("damage is refused");
        assert!(refused.to_string().contains("damaged"), "{refused}");
        assert!(!dir.path().join("a").exists());
    }

    #[test]
    fn only_single_step_batches_skip_the_redo_file() {
        let log = PathBuf::from("j.jsonl");
        let effects = |files: usize, logs: Vec<LogEffect>| Effects {
            files: (0..files)
                .map(|n| (PathBuf::from(format!("f{n}")), Some(Vec::new())))
                .collect(),
            logs: logs
                .into_iter()
                .map(|effect| (log.clone(), effect))
                .collect(),
        };
        assert!(!effects(1, vec![]).needs_redo());
        assert!(!effects(0, vec![LogEffect::Append(vec![b"a".to_vec()])]).needs_redo());
        assert!(!effects(1, vec![LogEffect::Unchanged]).needs_redo());
        assert!(!effects(0, vec![LogEffect::Truncate(1)]).needs_redo());
        // A crash can stop a multi-line append after some of its lines.
        assert!(
            effects(
                0,
                vec![LogEffect::Append(vec![b"a".to_vec(), b"b".to_vec()])]
            )
            .needs_redo()
        );
        assert!(effects(1, vec![LogEffect::Append(vec![b"a".to_vec()])]).needs_redo());
        assert!(effects(2, vec![]).needs_redo());
    }

    #[test]
    fn a_multi_line_append_cut_short_is_finished_on_open() {
        pollster::block_on(async {
            let dir = tempfile::tempdir().unwrap();
            let batch = [
                put(&entry("j.jsonl", 0), "0"),
                put(&entry("j.jsonl", 1), "1"),
            ];
            fs::write(dir.path().join(REDO_FILE), encode_redo(&batch)).unwrap();
            // Only the first line reached the disk.
            fs::write(dir.path().join("j.jsonl"), "0\n").unwrap();

            let store = DirectoryBackend::open(dir.path()).unwrap();
            assert_eq!(
                fs::read_to_string(dir.path().join("j.jsonl")).unwrap(),
                "0\n1\n"
            );
            assert_eq!(store.scan("j.jsonl/", "j.jsonl0").await.unwrap().len(), 2);
        });
    }

    #[test]
    fn the_redo_record_round_trips() {
        let ops = [
            put("a/b.json", "value"),
            delete("c"),
            put("d.jsonl/0000000000000000", ""),
        ];
        assert_eq!(decode_redo(&encode_redo(&ops)).unwrap(), ops);
    }

    #[test]
    fn a_torn_line_is_dropped_when_the_log_is_read() {
        pollster::block_on(async {
            let dir = tempfile::tempdir().unwrap();
            fs::write(dir.path().join("j.jsonl"), "0\n1\n2").unwrap();
            let store = DirectoryBackend::open(dir.path()).unwrap();
            assert_eq!(store.scan("j.jsonl/", "j.jsonl0").await.unwrap().len(), 2);
            store.put(&entry("j.jsonl", 2), b"two").await.unwrap();
            assert_eq!(
                fs::read_to_string(dir.path().join("j.jsonl")).unwrap(),
                "0\n1\ntwo\n"
            );
        });
    }

    #[cfg(feature = "json")]
    #[test]
    fn a_journal_kept_under_a_jsonl_prefix_is_one_line_per_entry() {
        use crate::{Journal, JsonSlots, Seq};
        pollster::block_on(async {
            let (_dir, store) = store();
            let slots = JsonSlots::new(store.clone());
            let mut log: Journal<String> = Journal::new();
            log.append("first".into());
            log.append_entries(&slots, "s/journal.jsonl", Seq(0))
                .await
                .unwrap();
            let saved = log.next_seq();
            log.append_caused_by([Seq(0)], "second".into()).unwrap();
            log.append_entries(&slots, "s/journal.jsonl", saved)
                .await
                .unwrap();

            let text = fs::read_to_string(store.root().join("s").join("journal.jsonl")).unwrap();
            assert_eq!(
                text,
                "{\"entry\":\"first\",\"causes\":[]}\n{\"entry\":\"second\",\"causes\":[0]}\n"
            );
            let back: Journal<String> =
                Journal::load_entries(&slots, "s/journal.jsonl", None, None)
                    .await
                    .unwrap();
            assert_eq!(back, log);
        });
    }
}
