// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! The upstream's `[patch.crates-io]` table: finding its checkout by git URL,
//! reading its root manifest at `origin/<branch>`, and choosing the rows.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::cycles::tree::{self, Toml};
use crate::resolved::{elsewhere, git_out};
use crate::text::py_path;

/// One patch row: the crate and its string fields, in the upstream's order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub name: String,
    pub fields: Vec<(String, String)>,
}

impl Row {
    /// The row's `git` URL.
    pub fn git(&self) -> &str {
        self.fields.iter().find(|(k, _)| k == "git").map_or("", |(_, v)| v.as_str())
    }

    /// The row as one TOML line, without a line ending.
    pub fn line(&self) -> String {
        let body: Vec<String> = self.fields.iter().map(|(k, v)| format!("{k} = {}", basic(v))).collect();
        format!("{} = {{ {} }}", self.name, body.join(", "))
    }
}

/// A TOML basic string.
pub fn basic(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if c.is_control() => out.push_str(&format!("\\u{:04X}", u32::from(c))),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Which upstream rows a consumer carries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Rule {
    /// Rows whose `git` URL starts with this prefix: the fork owner's.
    ForkPrefix(String),
}

impl Rule {
    fn carries(&self, row: &Row) -> bool {
        match self {
            Rule::ForkPrefix(prefix) => row.git().starts_with(prefix.as_str()),
        }
    }
}

/// The rows of a manifest's `[patch.crates-io]` that `rule` carries.
///
/// # Errors
/// Text that is not TOML, or a carried row that is not a table of strings
/// with a `git` URL, or that names `rev` or `tag`: rows follow branches.
pub fn select(manifest: &str, rule: &Rule) -> Result<Vec<Row>, String> {
    let data = tree::parse(manifest)?;
    let Some(table) = data.get("patch").and_then(|p| p.get("crates-io")).and_then(Toml::as_table) else {
        return Ok(Vec::new());
    };
    let mut rows = Vec::new();
    for (name, spec) in table {
        let Some(spec) = spec.as_table() else { continue };
        let mut fields = Vec::new();
        let mut plain = true;
        for (k, v) in spec {
            match v.as_str() {
                Some(s) => fields.push((k.clone(), s.to_string())),
                None => plain = false,
            }
        }
        let row = Row { name: name.clone(), fields };
        if !rule.carries(&row) {
            continue;
        }
        if !plain {
            return Err(format!("upstream row {name}: only string fields are carried"));
        }
        if let Some((k, _)) = row.fields.iter().find(|(k, _)| k == "rev" || k == "tag") {
            return Err(format!("upstream row {name} names `{k}`; carried rows follow branches"));
        }
        rows.push(row);
    }
    rows.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(rows)
}

/// A git URL reduced to `host/owner/name`, lowercased, so the https, ssh and
/// scp forms and a trailing `.git` or `/` compare equal.
pub fn normalize(url: &str) -> String {
    let mut u = url.trim().to_lowercase();
    if let Some(rest) = u.strip_prefix("git+") {
        u = rest.to_string();
    }
    let scp = !u.contains("://");
    if let Some((_, rest)) = u.split_once("://") {
        u = rest.to_string();
    }
    if let Some((_, rest)) = u.split_once('@').filter(|(user, _)| !user.contains('/')) {
        u = rest.to_string();
    }
    if scp && let Some(at) = u.find(':').filter(|&i| !u[..i].contains('/')) {
        u.replace_range(at..=at, "/");
    }
    let u = u.split(['?', '#']).next().unwrap_or("").trim_end_matches('/');
    u.strip_suffix(".git").unwrap_or(u).trim_end_matches('/').to_string()
}

/// The family directory when none is named: the parent of the main working
/// tree, so a linked worktree finds the same siblings as its main checkout.
pub fn default_family_dir() -> Option<PathBuf> {
    let common = git_out(&["rev-parse", "--path-format=absolute", "--git-common-dir"], None)?;
    let common = PathBuf::from(String::from_utf8(common).ok()?.trim_end_matches(['\n', '\r']));
    let main = if common.file_name().is_some_and(|n| n == ".git") {
        common.parent()?.to_path_buf()
    } else {
        crate::resolved::toplevel()?
    };
    main.parent().map(Path::to_path_buf)
}

/// The one checkout directly under `family_dir` whose origin is `url`.
///
/// # Errors
/// An unreadable directory, no such checkout, or two.
pub fn find(family_dir: &Path, url: &str) -> Result<PathBuf, String> {
    let want = normalize(url);
    let mut found: Vec<PathBuf> = Vec::new();
    let entries = std::fs::read_dir(family_dir).map_err(|e| format!("{}: {e}", py_path(family_dir)))?;
    let mut children: Vec<PathBuf> = entries.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
    children.sort();
    for child in children {
        let origin = git_out(&["config", "--get", "remote.origin.url"], Some(&child));
        let origin = origin.map(|o| String::from_utf8_lossy(&o).trim().to_string()).unwrap_or_default();
        if !origin.is_empty() && normalize(&origin) == want {
            found.push(child);
        }
    }
    match found.as_slice() {
        [one] => Ok(one.clone()),
        [] => Err(format!("no checkout of {url} under {}; clone it there or pass --family-dir or --remote", py_path(family_dir))),
        [a, b, ..] => Err(format!("two checkouts of {url}: {} and {}; name one family directory", py_path(a), py_path(b))),
    }
}

/// The root `Cargo.toml` of `checkout` at `rev`.
///
/// # Errors
/// The revision or the file is missing, or the text is not UTF-8.
pub fn manifest_at(checkout: &Path, rev: &str) -> Result<String, String> {
    let out = git_out(&["show", &format!("{rev}:Cargo.toml")], Some(checkout))
        .ok_or_else(|| format!("{}: no Cargo.toml at {rev}; fetch it", py_path(checkout)))?;
    String::from_utf8(out).map_err(|e| format!("{}: Cargo.toml at {rev}: {e}", py_path(checkout)))
}

/// The upstream's root manifest from a shallow clone of `branch`, in `dir`.
///
/// # Errors
/// The clone failing.
pub fn remote(url: &str, branch: &str, dir: &Path) -> Result<String, String> {
    let mut cmd = Command::new("git");
    let out = elsewhere(&mut cmd)
        .args(["clone", "-q", "--depth", "1", "--no-checkout", "--branch", branch, url])
        .arg(dir)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("git clone: {e}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(format!("cannot clone {url} {branch}: {}", err.lines().next().unwrap_or("").trim()));
    }
    manifest_at(dir, "HEAD")
}
