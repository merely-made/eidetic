// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Finding the family's checkouts: the directories under a family directory,
//! or shallow clones of every `merely-made` repository.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::scan::{self, ORG};
use crate::json;
use crate::resolved::{REF, elsewhere, git_out};
use crate::text::py_path;

const CLONE_WORKERS: usize = 6;

fn git_text(args: &[&str], dir: &Path) -> Option<String> {
    let out = git_out(args, Some(dir))?;
    Some(String::from_utf8_lossy(&out).trim().to_string())
}

/// A checkout's `HEAD`.
pub fn head(dir: &Path) -> Option<String> {
    git_text(&["rev-parse", "HEAD"], dir)
}

/// The family name of a checkout: its origin's `merely-made` name, else its
/// directory name; `None` for an origin elsewhere.
fn identify(dir: &Path) -> Option<String> {
    match git_text(&["config", "--get", "remote.origin.url"], dir).filter(|o| !o.is_empty()) {
        Some(origin) => scan::repo_of_url(&origin),
        None => dir.file_name().map(|n| n.to_string_lossy().into_owned()),
    }
}

/// The checkouts directly under `family_dir`, by family name.
///
/// # Errors
/// Two checkouts of one repository.
pub fn local(family_dir: &Path) -> Result<BTreeMap<String, PathBuf>, String> {
    let mut children: Vec<PathBuf> =
        std::fs::read_dir(family_dir).map_err(|e| format!("{}: {e}", py_path(family_dir)))?.flatten().map(|e| e.path()).collect();
    children.sort();
    let mut repos: BTreeMap<String, PathBuf> = BTreeMap::new();
    for child in children {
        let hidden = child.file_name().is_some_and(|n| n.to_string_lossy().starts_with('.'));
        if !child.is_dir() || hidden {
            continue;
        }
        let Some(name) = identify(&child) else { continue };
        if let Some(first) = repos.get(&name) {
            return Err(format!(
                "two checkouts of {name}: {} and {}; name one family directory",
                py_path(first),
                py_path(&child)
            ));
        }
        repos.insert(name, child);
    }
    Ok(repos)
}

/// Shallow-clones every unarchived `merely-made` repository (or those in
/// `only`) into `workdir`, with its resolved notes when the remote has them.
///
/// # Errors
/// `gh repo list` failing or printing something other than the listing.
///
/// # Panics
/// A clone worker panicking while it holds the results lock.
pub fn remote(workdir: &Path, only: &BTreeSet<String>) -> Result<BTreeMap<String, PathBuf>, String> {
    let listing = Command::new("gh")
        .args(["repo", "list", ORG, "--limit", "300", "--no-archived", "--json", "name"])
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("gh: {e}"))?;
    if !listing.status.success() {
        return Err(format!("gh repo list failed: {}", String::from_utf8_lossy(&listing.stderr).trim()));
    }
    let parsed = json::parse(&String::from_utf8_lossy(&listing.stdout))?;
    let mut names: Vec<String> = match parsed {
        json::Json::Arr(items) => items.iter().filter_map(|i| i.get("name")?.as_str().map(str::to_string)).collect(),
        _ => return Err("gh repo list did not print a list".into()),
    };
    names.sort();
    if !only.is_empty() {
        names.retain(|n| only.contains(n));
    }
    let results: Mutex<Vec<Option<std::io::Result<Output>>>> = Mutex::new((0..names.len()).map(|_| None).collect());
    let next = AtomicUsize::new(0);
    std::thread::scope(|s| {
        for _ in 0..CLONE_WORKERS {
            s.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::SeqCst);
                    let Some(name) = names.get(i) else { break };
                    let done = clone(workdir, name);
                    results.lock().expect("clone results")[i] = Some(done);
                }
            });
        }
    });
    let mut repos = BTreeMap::new();
    for (name, done) in names.iter().zip(results.into_inner().expect("clone results")) {
        match done {
            Some(Ok(out)) if out.status.success() => {
                repos.insert(name.clone(), workdir.join(name));
            }
            Some(Ok(out)) => eprintln!("warning: cannot clone {name}: {}", String::from_utf8_lossy(&out.stderr).trim()),
            Some(Err(e)) => eprintln!("warning: cannot clone {name}: {e}"),
            None => eprintln!("warning: cannot clone {name}: not attempted"),
        }
    }
    Ok(repos)
}

fn clone(workdir: &Path, name: &str) -> std::io::Result<Output> {
    let target = workdir.join(name);
    let url = format!("https://github.com/{ORG}/{name}.git");
    let mut cmd = Command::new("git");
    let out = elsewhere(&mut cmd)
        .args(["clone", "-q", "--depth", "1", &url])
        .arg(&target)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .output()?;
    if out.status.success() {
        // A clone carries no notes; a remote without any is not an error.
        let notes = format!("refs/notes/{REF}:refs/notes/{REF}");
        let mut cmd = Command::new("git");
        let _ = elsewhere(&mut cmd)
            .args(["fetch", "-q", "--depth", "1", "origin", &notes])
            .current_dir(&target)
            .env("GIT_TERMINAL_PROMPT", "0")
            .stdin(Stdio::null())
            .output();
    }
    Ok(out)
}
