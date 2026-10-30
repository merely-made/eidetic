// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Read-only construction of a standard v2 tree and deferred orphan cleanup.

use crate::{
    Config, TreeType, Version,
    bounded_manifest::parse_levels,
    bounded_prepare::{PrepareError, RecoveryBudget, RecoveryResource},
    bounded_segment::prepare_segment,
    coding::Decode,
    file::{LEVELS_MANIFEST_FILE, MANIFEST_FILE, SEGMENTS_FOLDER},
    level_manifest::LevelManifest,
    manifest::Manifest,
    segment::Segment,
    stop_signal::StopSignal,
    tree::{
        Tree,
        inner::{TreeInner, get_next_tree_id},
    },
};
use std::{
    io::{Cursor, Read},
    path::{Path, PathBuf},
    sync::{Arc, RwLock, atomic::AtomicU64},
};

fn corrupt() -> PrepareError {
    PrepareError::Corruption("invalid tree metadata".into())
}
fn charge(b: &mut dyn RecoveryBudget, n: usize) -> Result<(), PrepareError> {
    let n = u64::try_from(n).map_err(|_| corrupt())?;
    b.charge(RecoveryResource::MetadataBytes, n)?;
    b.charge(RecoveryResource::AllocationBytes, n)
}
fn arc_request<T>() -> Result<usize, PrepareError> {
    let header = std::alloc::Layout::new::<(
        std::sync::atomic::AtomicUsize,
        std::sync::atomic::AtomicUsize,
    )>();
    Ok(header
        .extend(std::alloc::Layout::new::<T>())
        .map_err(|_| corrupt())?
        .0
        .pad_to_align()
        .size())
}
fn tree_shell_request() -> Result<usize, PrepareError> {
    use crate::{memtable::Memtable, tree::inner::SealedMemtables};
    let requests = [
        arc_request::<TreeInner>()?,
        arc_request::<AtomicU64>()?,
        arc_request::<RwLock<Arc<Memtable>>>()?,
        arc_request::<Memtable>()?,
        arc_request::<RwLock<SealedMemtables>>()?,
        arc_request::<RwLock<LevelManifest>>()?,
        arc_request::<std::sync::atomic::AtomicBool>()?,
    ];
    requests
        .into_iter()
        .try_fold(0usize, |sum, n| sum.checked_add(n))
        .ok_or_else(corrupt)
}
pub(crate) fn os_path_capacity(path: &Path) -> Result<usize, PrepareError> {
    let bytes = path.as_os_str().as_encoded_bytes();
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        return bytes.len().checked_add(1).ok_or_else(corrupt);
    }
    #[cfg(target_os = "windows")]
    {
        // std's short absolute-path branch only allocates the UTF-16 conversion.
        // Longer or non-absolute paths enter OS-dependent normalization.
        let absolute =
            bytes.len() >= 3 && bytes[1] == b':' && (bytes[2] == b'\\' || bytes[2] == b'/')
                || bytes.starts_with(b"\\\\")
                || bytes.starts_with(b"//");
        if bytes.len() >= 247 || !absolute {
            return Err(PrepareError::Unsupported("Windows long path accounting"));
        }
        return bytes
            .len()
            .checked_add(1)
            .and_then(|n| n.checked_mul(2))
            .ok_or_else(corrupt);
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        let _ = bytes;
        Err(PrepareError::Unsupported(
            "path accounting not audited for this platform",
        ))
    }
}
fn charge_os_path(path: &Path, b: &mut dyn RecoveryBudget) -> Result<(), PrepareError> {
    charge(b, os_path_capacity(path)?)
}
pub(crate) fn precharge_commit_paths(
    missing: Option<&Path>,
    orphans: &[PathBuf],
    b: &mut dyn RecoveryBudget,
) -> Result<(), PrepareError> {
    if let Some(path) = missing {
        charge_os_path(path, b)?;
        charge_os_path(path, b)?;
    }
    for path in orphans {
        charge_os_path(path, b)?;
    }
    Ok(())
}
fn join(path: &Path, name: &str, b: &mut dyn RecoveryBudget) -> Result<PathBuf, PrepareError> {
    let n = path
        .as_os_str()
        .as_encoded_bytes()
        .len()
        .checked_add(name.len())
        .and_then(|n| n.checked_add(1))
        .and_then(|n| n.checked_mul(2))
        .ok_or_else(corrupt)?;
    charge(b, n)?;
    Ok(path.join(name))
}
fn read_file(path: &Path, b: &mut dyn RecoveryBudget) -> Result<Vec<u8>, PrepareError> {
    charge_os_path(path, b)?;
    let mut file = std::fs::File::open(path).map_err(PrepareError::Io)?;
    let len = file.metadata().map_err(PrepareError::Io)?.len();
    b.charge(RecoveryResource::InputBytes, len)?;
    let n = usize::try_from(len).map_err(|_| corrupt())?;
    charge(b, n)?;
    let mut result = Vec::new();
    result
        .try_reserve_exact(n)
        .map_err(|_| PrepareError::Io(std::io::Error::other("manifest allocation failed")))?;
    result.resize(n, 0);
    file.read_exact(&mut result).map_err(PrepareError::Io)?;
    Ok(result)
}
#[cfg(any(target_os = "linux", target_os = "macos"))]
const NAME_RESERVE: usize = 3 * 65_536;
#[cfg(target_os = "windows")]
const NAME_RESERVE: usize = 2 * (260 + 520 + 1040) + 780;
#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
const NAME_RESERVE: usize = 0;

/// A tree whose stored metadata and deferred cleanup have been admitted.
#[doc(hidden)]
pub struct PreparedTree {
    tree: Option<Tree>,
    orphans: Vec<PathBuf>,
    missing_segment_directory: Option<PathBuf>,
}
impl PreparedTree {
    /// The admitted in-memory tree, available for constructing replay state.
    #[must_use]
    pub fn tree(&self) -> &Tree {
        self.tree.as_ref().expect("prepared tree already consumed")
    }
    /// Install already admitted replay tables before any filesystem cleanup.
    /// Returns handles for the sealed tables in oldest-to-newest order.
    ///
    /// # Errors
    /// Returns a typed limit or allocation refusal without changing the tree.
    pub fn install_replay(
        &mut self,
        active: Option<crate::Memtable>,
        sealed: Vec<crate::Memtable>,
        budget: &mut dyn RecoveryBudget,
    ) -> Result<Vec<(crate::tree::inner::MemtableId, Arc<crate::Memtable>)>, PrepareError> {
        let tree = self.tree();
        let mut active_lock = tree.active_memtable.write().expect("lock is poisoned");
        let mut sealed_lock = tree.sealed_memtables.write().expect("lock is poisoned");
        if !active_lock.is_empty() || sealed_lock.len() != 0 {
            return Err(PrepareError::Unsupported("replay already installed"));
        }
        let count = sealed.len();
        budget.charge(
            RecoveryResource::VisitedItems,
            u64::try_from(count).map_err(|_| corrupt())?,
        )?;
        let arc_count = count
            .checked_add(usize::from(active.is_some()))
            .ok_or_else(corrupt)?;
        let capacity = count
            .checked_mul(2)
            .and_then(|n| {
                n.checked_mul(std::mem::size_of::<(
                    crate::tree::inner::MemtableId,
                    Arc<crate::Memtable>,
                )>())
            })
            .and_then(|n| {
                n.checked_add(arc_count.checked_mul(arc_request::<crate::Memtable>().ok()?)?)
            })
            .ok_or_else(corrupt)?;
        charge(budget, capacity)?;
        let mut installed = Vec::new();
        let mut retained = Vec::new();
        installed
            .try_reserve_exact(count)
            .map_err(|_| PrepareError::Io(std::io::Error::other("replay allocation failed")))?;
        retained
            .try_reserve_exact(count)
            .map_err(|_| PrepareError::Io(std::io::Error::other("replay allocation failed")))?;
        let first = tree
            .segment_id_counter
            .load(std::sync::atomic::Ordering::Relaxed);
        let next = first
            .checked_add(u64::try_from(count).map_err(|_| corrupt())?)
            .ok_or_else(corrupt)?;
        for (offset, memtable) in sealed.into_iter().enumerate() {
            let id = first + offset as u64;
            let table = Arc::new(memtable);
            retained.push((id, table.clone()));
            installed.push((id, table));
        }
        let new_active = active.map(Arc::new);
        tree.segment_id_counter
            .store(next, std::sync::atomic::Ordering::Relaxed);
        *sealed_lock = crate::tree::inner::SealedMemtables::from_replay(installed);
        if let Some(active) = new_active {
            *active_lock = active;
        }
        Ok(retained)
    }

    /// Apply deferred cleanup and return the retained tree without further limit-bearing reads.
    /// # Errors
    /// Returns an I/O error if cleanup fails.
    pub fn commit(mut self) -> Result<Tree, PrepareError> {
        if let Some(path) = &self.missing_segment_directory {
            std::fs::create_dir(path).map_err(PrepareError::Io)?;
            crate::file::fsync_directory(path).map_err(PrepareError::Io)?;
        }
        for path in &self.orphans {
            std::fs::remove_file(path).map_err(PrepareError::Io)?;
        }
        Ok(self.tree.take().expect("prepared tree already consumed"))
    }
}

pub(crate) fn prepare(
    mut config: Config,
    budget: &mut dyn RecoveryBudget,
) -> Result<PreparedTree, PrepareError> {
    #[cfg(feature = "bytes")]
    return Err(PrepareError::Unsupported("bytes-backed bounded recovery"));
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    return Err(PrepareError::Unsupported(
        "directory accounting not audited for this platform",
    ));
    let manifest_path = join(&config.path, MANIFEST_FILE, budget)?;
    let bytes = read_file(&manifest_path, budget)?;
    if bytes.len() != 7 {
        return Err(corrupt());
    }
    let manifest = Manifest::decode_from(&mut Cursor::new(bytes)).map_err(|_| corrupt())?;
    if manifest.version != Version::V2 || manifest.level_count == 0 {
        return Err(corrupt());
    }
    if manifest.tree_type != TreeType::Standard {
        return Err(PrepareError::Unsupported("blob tree"));
    }
    config.level_count = manifest.level_count;
    config.tree_type = manifest.tree_type;
    config.table_type = manifest.table_type;
    let levels_path = join(&config.path, LEVELS_MANIFEST_FILE, budget)?;
    let levels = parse_levels(&read_file(&levels_path, budget)?, budget)?;
    if levels.len() != config.level_count as usize {
        return Err(corrupt());
    }
    let count = levels
        .iter()
        .try_fold(0usize, |n, group| n.checked_add(group.len()))
        .ok_or_else(corrupt)?;
    charge(
        budget,
        count
            .checked_mul(std::mem::size_of::<(u64, usize)>())
            .ok_or_else(corrupt)?,
    )?;
    let mut ids = Vec::new();
    ids.try_reserve_exact(count)
        .map_err(|_| PrepareError::Io(std::io::Error::other("segment list allocation failed")))?;
    for (level, group) in levels.iter().enumerate() {
        ids.extend(group.iter().map(|id| (*id, level)));
    }
    ids.sort_unstable();
    if ids.windows(2).any(|p| p[0].0 == p[1].0) {
        return Err(corrupt());
    }
    let segment_dir = join(&config.path, SEGMENTS_FOLDER, budget)?;
    charge_os_path(&segment_dir, budget)?;
    let (mut dir, missing_segment_directory) = match std::fs::read_dir(&segment_dir) {
        Ok(dir) => (Some(dir), None),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && count == 0 => {
            charge(budget, segment_dir.as_os_str().as_encoded_bytes().len())?;
            (None, Some(segment_dir.clone()))
        }
        Err(e) => return Err(PrepareError::Io(e)),
    };
    let mut present = Vec::<(u64, PathBuf)>::new();
    let mut orphans = Vec::<PathBuf>::new();
    while let Some(entries) = dir.as_mut() {
        let reserve = segment_dir
            .as_os_str()
            .as_encoded_bytes()
            .len()
            .checked_mul(3)
            .and_then(|n| n.checked_add(NAME_RESERVE + 16))
            .ok_or_else(corrupt)?;
        charge(budget, reserve)?;
        let Some(entry) = entries.next() else { break };
        let entry = entry.map_err(PrepareError::Io)?;
        budget.charge(RecoveryResource::VisitedItems, 1)?;
        let name = entry.file_name();
        let name = name
            .to_str()
            .ok_or(PrepareError::Unsupported("non-UTF-8 segment name"))?;
        if name == ".DS_Store" || name.starts_with("._") {
            continue;
        }
        let id = name.parse::<u64>().map_err(|_| corrupt())?;
        let path = entry.path();
        // DirEntry::file_type can secretly allocate another path when d_type is
        // unknown. An explicitly admitted path and lstat avoid that fallback.
        charge_os_path(&path, budget)?;
        if !std::fs::symlink_metadata(&path)
            .map_err(PrepareError::Io)?
            .file_type()
            .is_file()
        {
            return Err(corrupt());
        }
        if ids.binary_search_by_key(&id, |x| x.0).is_ok() {
            reserve_one(&mut present, budget)?;
            present.push((id, path));
        } else {
            reserve_one(&mut orphans, budget)?;
            orphans.push(path);
        }
    }
    present.sort_unstable_by_key(|v| v.0);
    if present.len() != ids.len() || present.iter().zip(&ids).any(|(p, i)| p.0 != i.0) {
        return Err(corrupt());
    }
    // Level::update_metadata forms one pointer per segment and sorts the
    // segment slice. LevelManifest::set_disjoint_flag also builds key-range
    // and pointer vectors for nonempty levels. Charge the requested capacities
    // before invoking those upstream helpers.
    let per_level = std::mem::size_of::<Vec<Segment>>()
        .checked_add(std::mem::size_of::<Arc<crate::level_manifest::level::Level>>())
        .and_then(|n| n.checked_add(arc_request::<crate::level_manifest::level::Level>().ok()?))
        .and_then(|n| n.checked_add(std::mem::size_of::<crate::KeyRange>()))
        .and_then(|n| n.checked_add(std::mem::size_of::<&crate::KeyRange>()))
        .ok_or_else(corrupt)?;
    let group_bytes = levels
        .len()
        .checked_mul(per_level)
        .and_then(|n| {
            n.checked_add(count.checked_mul(
                2 * std::mem::size_of::<Segment>() + std::mem::size_of::<&crate::KeyRange>(),
            )?)
        })
        .ok_or_else(corrupt)?;
    // The pinned KeyRange::is_disjoint compares each pair. Conservatively
    // charge a square number of visits before the potentially quadratic work.
    let visits = levels.iter().try_fold(
        levels.len().checked_mul(levels.len()).ok_or_else(corrupt)?,
        |sum, level| {
            sum.checked_add(level.len().checked_mul(level.len()).ok_or_else(corrupt)?)
                .ok_or_else(corrupt)
        },
    )?;
    budget.charge(
        RecoveryResource::VisitedItems,
        u64::try_from(visits).map_err(|_| corrupt())?,
    )?;
    charge(budget, group_bytes)?;
    let mut groups: Vec<Vec<Segment>> = Vec::with_capacity(levels.len());
    for level in &levels {
        groups.push(Vec::with_capacity(level.len()));
    }
    let tree_id = get_next_tree_id();
    for ((id, path), (_, level)) in present.into_iter().zip(ids) {
        let segment = prepare_segment(
            &path,
            tree_id,
            config.cache.clone(),
            config.descriptor_table.clone(),
            budget,
        )?;
        if segment.id() != id {
            return Err(corrupt());
        }
        groups[level].push(segment);
    }
    precharge_commit_paths(missing_segment_directory.as_deref(), &orphans, budget)?;
    let levels = LevelManifest::from_prepared(levels_path, groups);
    let highest = levels.iter().map(Segment::id).max().unwrap_or_default();
    let next = highest.checked_add(1).ok_or_else(corrupt)?;
    // Fixed tree shell and retained lock/memtable handles are charged before construction.
    charge(budget, tree_shell_request()?)?;
    config
        .descriptor_table
        .insert_bounded_recovery(&levels, budget)?;
    let inner = TreeInner {
        id: tree_id,
        segment_id_counter: Arc::new(AtomicU64::new(next)),
        active_memtable: Arc::default(),
        sealed_memtables: Arc::default(),
        levels: Arc::new(RwLock::new(levels)),
        stop_signal: StopSignal::default(),
        config,
        major_compaction_lock: RwLock::default(),
    };
    Ok(PreparedTree {
        tree: Some(Tree(Arc::new(inner))),
        orphans,
        missing_segment_directory,
    })
}
fn reserve_one<T>(v: &mut Vec<T>, b: &mut dyn RecoveryBudget) -> Result<(), PrepareError> {
    if v.len() == v.capacity() {
        let capacity = v.capacity().max(2).checked_mul(2).ok_or_else(corrupt)?;
        charge(
            b,
            capacity
                .checked_mul(std::mem::size_of::<T>())
                .ok_or_else(corrupt)?,
        )?;
        v.try_reserve_exact(capacity - v.len()).map_err(|_| {
            PrepareError::Io(std::io::Error::other("directory list allocation failed"))
        })?;
    }
    Ok(())
}
