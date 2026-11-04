// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl Inner {
    pub(super) fn locked(&self) -> Result<std::sync::MutexGuard<'_, State>, StoreError> {
        let state = self.state.lock().expect("directory backend lock poisoned");
        match &state.poisoned {
            Some(why) => Err(StoreError::Backend(why.clone())),
            None => Ok(state),
        }
    }

    pub(super) fn target(&self, key: &str) -> Result<Target, StoreError> {
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

    pub(super) fn recover(&self) -> Result<(), StoreError> {
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

    pub(super) fn get(
        &self,
        logs: &mut LogCache,
        key: &str,
    ) -> Result<Option<Vec<u8>>, StoreError> {
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
            }
            Target::Entry { log, index } => logs.read(&log, index),
        }
    }

    /// Every key beginning with `prefix`, in no particular order.
    pub(super) fn keys(
        &self,
        logs: &mut LogCache,
        prefix: &str,
    ) -> Result<Vec<String>, StoreError> {
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

    pub(super) fn walk(
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

    pub(super) fn push_log_keys(
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
    pub(super) fn check_placeable(&self, path: &Path) -> Result<(), StoreError> {
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
    pub(super) fn effects(
        &self,
        logs: &mut LogCache,
        ops: &[WriteOp],
    ) -> Result<Effects, StoreError> {
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
                }
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
                }
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

    pub(super) fn log_effect(
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

    pub(super) fn perform(&self, logs: &mut LogCache, effects: Effects) -> Result<(), StoreError> {
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
                }
            }
        }
        for (log, effect) in effects.logs {
            self.perform_log(logs, &log, effect)?;
        }
        Ok(())
    }

    pub(super) fn perform_log(
        &self,
        logs: &mut LogCache,
        log: &Path,
        effect: LogEffect,
    ) -> Result<(), StoreError> {
        match effect {
            LogEffect::Unchanged => {}
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
            }
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
            }
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
            }
            LogEffect::Remove => {
                logs.forget(log);
                fs::remove_file(log).map_err(|error| io_error(log, error))?;
                prune_empty(&self.root, log.parent().expect("a log file has a parent"));
            }
        }
        Ok(())
    }

    /// Apply `ops` all together. Called with the write lock held.
    pub(super) fn apply(&self, state: &mut State, ops: &[WriteOp]) -> Result<(), StoreError> {
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
