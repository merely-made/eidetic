// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! `house resolved record | push | show`: what cargo resolved for a commit,
//! kept as a git note under `refs/notes/resolved` since no Cargo.lock is
//! tracked. A port of the family's `record-resolved.sh`.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

use crate::notes;

/// The notes ref, without `refs/notes/`.
pub const REF: &str = "resolved";
/// Set in `git push`'s environment so the pre-push hook does not recurse.
pub const PUSHING: &str = "RESOLVED_NOTES_PUSH";

const USAGE: &str = "usage: house resolved record | push | show <commit> [--write [--force]]";

/// The `resolved` command line, without `resolved` itself.
pub fn run(args: &[String]) -> ExitCode {
    match args.first().map(String::as_str) {
        Some("record") if args.len() == 1 => {
            record();
            ExitCode::SUCCESS
        }
        Some("push") if args.len() == 1 => {
            push();
            ExitCode::SUCCESS
        }
        Some("show") => match show(&args[1..]) {
            Ok(()) => ExitCode::SUCCESS,
            Err(Failure::Usage) => usage(),
            Err(Failure::Message(m)) => {
                eprintln!("house resolved show: {m}");
                ExitCode::FAILURE
            }
        },
        _ => usage(),
    }
}

fn usage() -> ExitCode {
    eprintln!("{USAGE}");
    ExitCode::from(2)
}

enum Failure {
    Usage,
    Message(String),
}

fn git() -> Command {
    Command::new("git")
}

/// Runs git quietly; its stdout when it succeeded.
pub(crate) fn git_out(args: &[&str], dir: Option<&Path>) -> Option<Vec<u8>> {
    let mut cmd = git();
    cmd.args(args).stdin(Stdio::null()).stderr(Stdio::null());
    if let Some(dir) = dir {
        cmd.current_dir(dir);
    }
    let out = cmd.output().ok()?;
    out.status.success().then_some(out.stdout)
}

fn git_ok(args: &[&str]) -> bool {
    git_out(args, None).is_some()
}

pub(crate) fn toplevel() -> Option<PathBuf> {
    let out = git_out(&["rev-parse", "--show-toplevel"], None)?;
    let text = String::from_utf8(out).ok()?;
    Some(PathBuf::from(text.trim_end_matches(['\n', '\r'])))
}

/// The lock beside every tracked `Cargo.toml` that has one on disk, sorted
/// by path. Build output is never walked.
pub fn collect(root: &Path) -> Vec<(String, Vec<u8>)> {
    let Some(out) = git_out(&["ls-files", "-z", "--", "Cargo.toml", "*/Cargo.toml"], Some(root)) else {
        return Vec::new();
    };
    let mut paths: Vec<String> = out
        .split(|b| *b == 0)
        .filter_map(|p| std::str::from_utf8(p).ok())
        .filter_map(|p| p.strip_suffix("Cargo.toml"))
        .map(|dir| format!("{dir}Cargo.lock"))
        .collect();
    paths.sort();
    paths.into_iter().filter_map(|p| std::fs::read(root.join(&p)).ok().map(|b| (p, b))).collect()
}

/// Attaches every lock to `HEAD`, replacing any note there. Never fails.
pub fn record() {
    let Some(root) = toplevel() else { return };
    let note = notes::assemble(&collect(&root));
    if note.is_empty() {
        return;
    }
    if !attach(&root, &note) {
        eprintln!("record-resolved: could not attach the note");
    }
}

fn attach(root: &Path, note: &[u8]) -> bool {
    let child = git()
        .args(["notes", &format!("--ref={REF}"), "add", "-f", "-F", "-", "HEAD"])
        .current_dir(root)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
    let Ok(mut child) = child else { return false };
    let wrote = child.stdin.take().is_some_and(|mut s| s.write_all(note).is_ok());
    child.wait().is_ok_and(|s| s.success()) && wrote
}

/// Sends the notes beside the branch. Never blocks a push.
pub fn push() {
    if std::env::var_os(PUSHING).is_some_and(|v| !v.is_empty()) {
        return;
    }
    let notes_ref = format!("--ref={REF}");
    // A rebase gives a commit a new id and leaves its note behind.
    if !git_ok(&["notes", &notes_ref, "show", "HEAD"]) {
        record();
    }
    let full = format!("refs/notes/{REF}");
    if !git_ok(&["rev-parse", "-q", "--verify", &full]) {
        return;
    }
    let remote = std::env::var("PRE_COMMIT_REMOTE_NAME").ok().filter(|r| !r.is_empty());
    let remote = remote.as_deref().unwrap_or("origin");
    let local_copy = format!("refs/notes/{REF}-remote");
    match git_quiet(&["fetch", "-q", remote, &format!("+{full}:{local_copy}")], false) {
        Ok(()) => {
            let merge = ["notes", &notes_ref, "merge", "-q", "-s", "cat_sort_uniq", &local_copy];
            if let Err(e) = git_quiet(&merge, false) {
                eprintln!("house resolved push: could not merge {full} from {remote}: {e}");
            }
        }
        // The first push to a remote finds no notes there yet.
        Err(e) if e.contains("couldn't find remote ref") => {}
        Err(e) => eprintln!("house resolved push: could not fetch {full} from {remote}: {e}"),
    }
    if let Err(e) = git_quiet(&["push", "-q", remote, &full], true) {
        eprintln!("house resolved push: could not push {full} to {remote}: {e}");
    }
}

/// Runs git with stdout passed through; on failure, the first line of its
/// stderr. `pushing` sets `RESOLVED_NOTES_PUSH=1` for the child.
fn git_quiet(args: &[&str], pushing: bool) -> Result<(), String> {
    let mut cmd = git();
    cmd.args(args).stdin(Stdio::null()).stdout(Stdio::inherit()).stderr(Stdio::piped());
    if pushing {
        cmd.env(PUSHING, "1");
    }
    let out = cmd.output().map_err(|e| format!("git: {e}"))?;
    if out.status.success() {
        return Ok(());
    }
    let err = String::from_utf8_lossy(&out.stderr);
    let first = err.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("git failed");
    Err(first.to_string())
}

fn show(args: &[String]) -> Result<(), Failure> {
    let (mut commit, mut write, mut force) = (None, false, false);
    for a in args {
        match a.as_str() {
            "--write" => write = true,
            "--force" => force = true,
            s if s.starts_with('-') || commit.is_some() => return Err(Failure::Usage),
            s => commit = Some(s),
        }
    }
    let commit = commit.ok_or(Failure::Usage)?;
    if force && !write {
        return Err(Failure::Usage);
    }
    let note = git_out(&["notes", &format!("--ref={REF}"), "show", commit], None)
        .ok_or_else(|| Failure::Message(format!("{commit} has no note under refs/notes/{REF}")))?;
    let note = String::from_utf8(note).map_err(|_| Failure::Message("the note is not UTF-8".into()))?;
    if !write {
        print!("{note}");
        return Ok(());
    }
    let root = toplevel().ok_or_else(|| Failure::Message("not in a git repository".into()))?;
    write_locks(&root, &notes::split(&note), force).map_err(Failure::Message)
}

/// Writes each lock to its path under `root`. Nothing is written when any
/// path is unsafe, or exists and `force` is off.
///
/// # Errors
/// No locks, an unsafe or present path, or a failed write.
pub fn write_locks(root: &Path, locks: &[(String, String)], force: bool) -> Result<(), String> {
    if locks.is_empty() {
        return Err("the note holds no locks".into());
    }
    let mut refused = Vec::new();
    for (path, _) in locks {
        if !notes::safe_path(path) {
            return Err(format!("refusing to write outside the repository: {path}"));
        }
        if !force && root.join(path).exists() {
            refused.push(path.as_str());
        }
    }
    if !refused.is_empty() {
        return Err(format!("already present, pass --force to overwrite: {}", refused.join(", ")));
    }
    for (path, text) in locks {
        let full = root.join(path);
        if let Some(dir) = full.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        std::fs::write(&full, text).map_err(|e| format!("{path}: {e}"))?;
        eprintln!("wrote {path}");
    }
    Ok(())
}
