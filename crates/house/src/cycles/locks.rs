// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! The resolved graphs: one per lock root. No `Cargo.lock` is tracked, so a
//! root's lock comes from the `refs/notes/resolved` note on the checkout's
//! `HEAD`, else from a `Cargo.lock` on disk; a root with neither is reported
//! as having no resolved versions recorded.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use super::scan::{self, Manifest, Roots, Walked};
use super::tree::{self, Toml};
use crate::notes;

/// Where a lock's text came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Note,
    Disk,
}

/// The locks found for one repository, and the expected roots with none.
#[derive(Debug, Default, PartialEq)]
pub struct Found {
    /// (repository-relative lock path, text, source), sorted by path.
    pub locks: Vec<(String, String, Source)>,
    /// Expected lock paths with no lock in the note or on disk.
    pub missing: Vec<String>,
}

/// Takes each lock from the note when it holds one for that path, else from
/// disk. Note paths inside skipped directories are ignored, as the walk
/// ignores them on disk.
pub fn choose(note: Option<&str>, disk: Vec<(String, String)>, expected: &[String]) -> Found {
    let mut by_path: BTreeMap<String, (String, Source)> = BTreeMap::new();
    for (path, text) in note.map(notes::split).unwrap_or_default() {
        if notes::safe_path(&path) && !scan::path_skipped(&path) {
            by_path.insert(path, (text, Source::Note));
        }
    }
    for (path, text) in disk {
        by_path.entry(path).or_insert((text, Source::Disk));
    }
    let missing = expected.iter().filter(|p| !by_path.contains_key(*p)).cloned().collect();
    Found { locks: by_path.into_iter().map(|(p, (t, s))| (p, t, s)).collect(), missing }
}

/// The `Cargo.lock` files the walk found, read as text; unreadable ones warn
/// and are left out.
pub fn on_disk(repo: &Path, walked: &Walked) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for (rel, files) in walked {
        if !files.iter().any(|f| f == "Cargo.lock") {
            continue;
        }
        let rel = scan::join(rel, "Cargo.lock");
        let path = scan::on_disk(repo, &rel);
        match crate::text::read_sig(&path) {
            Ok(text) => out.push((rel, text)),
            Err(e) => eprintln!("warning: cannot read {}: {e}", crate::text::py_path(&path)),
        }
    }
    out
}

/// The note on the checkout's `HEAD`, if any.
pub fn head_note(repo: &Path) -> Option<String> {
    let out = crate::resolved::git_out(&["notes", &format!("--ref={}", crate::resolved::REF), "show", "HEAD"], Some(repo))?;
    String::from_utf8(out).ok()
}

/// Where cargo writes a lock for these manifests: each `[workspace]`
/// directory, and each package directory under no workspace.
pub fn expected(manifests: &[Manifest], roots: &Roots, name: &str) -> Vec<String> {
    let workspaces = roots.workspace_dirs(name);
    let covered = |dir: &str| workspaces.iter().any(|w| w.is_empty() || dir == w || dir.starts_with(&format!("{w}/")));
    let mut out: Vec<String> = workspaces.iter().map(|w| scan::join(w, "Cargo.lock")).collect();
    for m in manifests {
        let dir = scan::dirname(&m.rel);
        if m.data.has("package") && !covered(dir) {
            out.push(scan::join(dir, "Cargo.lock"));
        }
    }
    out.sort();
    out.dedup();
    out
}

/// One root-level edge inside a lock: (from, to, package, dependency).
pub type LockEdge = (String, String, String, String);

/// The cross-root edges inside one lock of repository `name`; `None` (with a
/// warning) when the text is not TOML.
pub fn lock_edges(name: &str, shown: &str, text: &str, roots: &Roots) -> Option<Vec<LockEdge>> {
    let data = match tree::parse(text) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("warning: cannot read {shown}: {e}");
            return None;
        }
    };
    let packages: Vec<&Toml> = match data.get("package") {
        Some(Toml::Arr(a)) => a.iter().filter(|p| p.get("name").and_then(Toml::as_str).is_some()).collect(),
        _ => Vec::new(),
    };
    let field = |p: &Toml, k: &str| p.get(k).and_then(Toml::as_str).map(str::to_string);
    let owner = |p: &Toml| -> Option<String> {
        let pname = field(p, "name")?;
        match field(p, "source") {
            None => Some(roots.of_package(name, &pname)),
            Some(s) if s.starts_with("git+") => scan::repo_of_url(&s).map(|r| roots.of_package(&r, &pname)),
            Some(_) => None,
        }
    };
    let mut by_name: HashMap<String, Vec<&Toml>> = HashMap::new();
    for p in &packages {
        by_name.entry(field(p, "name").unwrap_or_default()).or_default().push(p);
    }
    let mut edges = Vec::new();
    for p in &packages {
        let Some(src) = owner(p) else { continue };
        let deps = match p.get("dependencies") {
            Some(Toml::Arr(a)) => a.iter().filter_map(Toml::as_str).collect(),
            _ => Vec::new(),
        };
        for dep in deps {
            let parts: Vec<&str> = dep.split(' ').collect();
            let mut candidates: Vec<&Toml> = by_name.get(parts[0]).cloned().unwrap_or_default();
            if let Some(version) = parts.get(1) {
                candidates.retain(|c| field(c, "version").as_deref() == Some(*version));
            }
            if let Some(source) = parts.get(2) {
                let source = source.trim_matches(['(', ')']);
                candidates.retain(|c| field(c, "source").as_deref() == Some(source));
            }
            if let Some(candidate) = candidates.first()
                && let Some(dst) = owner(candidate)
                && dst != src
            {
                edges.push((src.clone(), dst, field(p, "name").unwrap_or_default(), field(candidate, "name").unwrap_or_default()));
            }
        }
    }
    Some(edges)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lock(name: &str) -> String {
        format!("version = 4\n\n[[package]]\nname = \"{name}\"\nversion = \"0.1.0\"\n")
    }

    #[test]
    fn the_note_wins_and_disk_fills_in() {
        let note = format!("# ---- Cargo.lock\n{}# ---- target/x/Cargo.lock\n{}", lock("from-note"), lock("skipped"));
        let disk = vec![("Cargo.lock".to_string(), lock("from-disk")), ("crates/b/Cargo.lock".to_string(), lock("b"))];
        let expected = vec!["Cargo.lock".to_string(), "crates/b/Cargo.lock".to_string(), "tools/Cargo.lock".to_string()];
        let found = choose(Some(&note), disk, &expected);
        assert_eq!(
            found.locks,
            vec![
                ("Cargo.lock".to_string(), lock("from-note"), Source::Note),
                ("crates/b/Cargo.lock".to_string(), lock("b"), Source::Disk),
            ]
        );
        assert_eq!(found.missing, ["tools/Cargo.lock"]);
    }

    #[test]
    fn no_note_and_no_disk_is_reported_missing() {
        let found = choose(None, Vec::new(), &["Cargo.lock".to_string()]);
        assert!(found.locks.is_empty());
        assert_eq!(found.missing, ["Cargo.lock"]);
        let found = choose(None, vec![("Cargo.lock".into(), lock("d"))], &["Cargo.lock".to_string()]);
        assert_eq!(found.locks[0].2, Source::Disk);
        assert!(found.missing.is_empty());
    }

    #[test]
    fn lock_edges_follow_git_sources() {
        let manifests = BTreeMap::new();
        let roots = Roots::new(&manifests);
        let text = "version = 4\n\n[[package]]\nname = \"a\"\nversion = \"0.1.0\"\ndependencies = [\"b 0.1.0 (git+https://github.com/merely-made/b?branch=main#1)\", \"serde\"]\n\n[[package]]\nname = \"b\"\nversion = \"0.1.0\"\nsource = \"git+https://github.com/merely-made/b?branch=main#1\"\n\n[[package]]\nname = \"serde\"\nversion = \"1.0.0\"\nsource = \"registry+https://github.com/rust-lang/crates.io-index\"\n";
        let edges = lock_edges("a", "a/Cargo.lock", text, &roots).unwrap();
        assert_eq!(edges, vec![("a".into(), "b".into(), "a".into(), "b".into())]);
        assert!(lock_edges("a", "x", "not = = toml", &roots).is_none());
    }
}
