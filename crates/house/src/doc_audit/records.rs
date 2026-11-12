// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! The audit's inputs: supplemental judgment records in the ledger's
//! `batch_*.md` files, the receipt relocation registry, and the file walks.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;
use sha2::{Digest, Sha256};

use crate::json::{self, Json};
use crate::text;

fn re(pattern: &str) -> Regex {
    Regex::new(pattern).expect("pattern")
}

static DOC_HEADER: LazyLock<Regex> = LazyLock::new(|| re(r"(?m)^##\s+`?(.+?\.md)`?\s*$"));
static DISPOSITION: LazyLock<Regex> = LazyLock::new(|| {
    re(r"(?mi)^- disposition:\s*`?(current|historical-marked|historical-unmarked|superseded|dead)`?\s*$")
});
static STATUS: LazyLock<Regex> =
    LazyLock::new(|| re(r#"(?mi)^- status line:\s*".*?"\s*[—–-]+\s*accurate:\s*`?(yes|no|n/a)`?\s*$"#));
static COUNTS: LazyLock<Regex> = LazyLock::new(|| {
    re(r"(?mi)^- claims checked:\s*(\d+)\s*[—–-]+\s*holds:\s*(\d+),\s*stale:\s*(\d+),\s*unverifiable:\s*(\d+)\s*$")
});
static BATCH: LazyLock<Regex> = LazyLock::new(|| re(r"\Abatch_(\d+)"));
static SOURCE: LazyLock<Regex> = LazyLock::new(|| re(r"\A[a-z0-9_]+_docs/testing/(?:receipts/)?[^\\]+\.md\z"));
static SHA256: LazyLock<Regex> = LazyLock::new(|| re(r"\A[0-9a-f]{64}\z"));
static SHA1: LazyLock<Regex> = LazyLock::new(|| re(r"\A[0-9a-f]{40}\z"));

const REQUIRED_SECTIONS: [&str; 4] = ["### Stale claims", "### Contradictions", "### Recommended action", "### Notes"];

/// One judgment record's verdicts.
#[derive(Clone, Debug, PartialEq)]
pub struct Record {
    pub disposition: String,
    pub status_accurate: String,
    pub claims: i64,
    pub holds: i64,
    pub stale: i64,
    pub unverifiable: i64,
}

/// Whether a file name matches the glob `<prefix>*.md`; case-insensitive on
/// Windows, as pathlib globs there.
fn matches(name: &str, prefix: &str) -> bool {
    let name = if cfg!(windows) { name.to_lowercase() } else { name.to_string() };
    name.len() >= prefix.len() + 3 && name.starts_with(prefix) && name.ends_with(".md")
}

/// The last component of a slash path, as `Path(path).name`.
pub fn basename(path: &str) -> &str {
    path.rsplit('/').find(|c| !c.is_empty() && *c != ".").unwrap_or("")
}

/// Every `*.md` file under `dir`, relative and slash-separated, sorted.
/// Symbolic links to directories are not followed.
pub fn markdown_under(dir: &Path) -> Vec<String> {
    fn walk(dir: &Path, rel: &str, out: &mut Vec<String>) {
        let Ok(read) = std::fs::read_dir(dir) else { return };
        for entry in read.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let child = if rel.is_empty() { name.clone() } else { format!("{rel}/{name}") };
            let Ok(kind) = entry.file_type() else { continue };
            if kind.is_dir() {
                walk(&entry.path(), &child, out);
            } else if matches(&name, "") && entry.path().is_file() {
                out.push(child);
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, "", &mut out);
    out.sort();
    out
}

/// Records by path, the errors found, and the count superseding earlier batches.
pub type Supplements = (BTreeMap<String, Record>, Vec<String>, usize);

/// The supplemental records, the errors in them, and how many superseded an
/// earlier batch's record. A later batch's record for a path supersedes an
/// earlier one (ruling S34); one batch judging a path twice is an error.
///
/// # Errors
/// A batch file that cannot be read as UTF-8.
pub fn supplements(ledger: &Path) -> Result<Supplements, String> {
    let mut batches: Vec<(u64, String)> = std::fs::read_dir(ledger)
        .map(|r| r.flatten().collect::<Vec<_>>())
        .unwrap_or_default()
        .into_iter()
        .filter(|e| e.path().is_file())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| matches(n, "batch_"))
        .map(|n| (BATCH.captures(&n).and_then(|c| c[1].parse().ok()).unwrap_or(0), n))
        .collect();
    batches.sort();
    let (mut records, mut origin, mut errors, mut superseded) = (BTreeMap::new(), BTreeMap::new(), Vec::new(), 0);
    for (_, batch) in batches {
        let text = text::read_utf8(&ledger.join(&batch))?;
        let headers: Vec<regex::Captures> = DOC_HEADER.captures_iter(&text).collect();
        for (i, header) in headers.iter().enumerate() {
            let mut path = header[1].replace('\\', "/");
            if let Some(rest) = path.strip_prefix("design_docs/") {
                path = rest.to_string();
            }
            let end = headers.get(i + 1).map_or(text.len(), |h| h.get(0).map_or(text.len(), |m| m.start()));
            let body = &text[header.get(0).map_or(0, |m| m.end())..end];
            if origin.get(&path) == Some(&batch) {
                errors.push(format!("duplicate supplemental record: {path}"));
                continue;
            }
            let disposition = DISPOSITION.captures(body);
            let status = STATUS.captures(body);
            let counts = COUNTS.captures(body);
            let missing: Vec<&str> = REQUIRED_SECTIONS.iter().copied().filter(|s| !body.contains(s)).collect();
            if disposition.is_none() {
                errors.push(format!("{batch}: {path}: missing or invalid disposition"));
            }
            if status.is_none() {
                errors.push(format!("{batch}: {path}: missing or invalid status verdict"));
            }
            if counts.is_none() {
                errors.push(format!("{batch}: {path}: missing or invalid claim counts"));
            }
            if !missing.is_empty() {
                errors.push(format!("{batch}: {path}: missing sections: {}", missing.join(", ")));
            }
            let (Some(disposition), Some(status), Some(counts)) = (disposition, status, counts) else { continue };
            if !missing.is_empty() {
                continue;
            }
            let n = |g: usize| counts[g].parse::<i64>().unwrap_or(0);
            let (checked, holds, stale, unverifiable) = (n(1), n(2), n(3), n(4));
            if checked != holds + stale + unverifiable {
                errors.push(format!("{batch}: {path}: counts do not sum ({checked} != {holds}+{stale}+{unverifiable})"));
            }
            if records.contains_key(&path) {
                superseded += 1;
            }
            origin.insert(path.clone(), batch.clone());
            records.insert(
                path,
                Record {
                    disposition: disposition[1].to_lowercase(),
                    status_accurate: status[1].to_lowercase(),
                    claims: checked,
                    holds,
                    stale,
                    unverifiable,
                },
            );
        }
    }
    Ok((records, errors, superseded))
}

/// SHA-256 of bytes, lowercase hex.
pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

/// Recorded receipt moves, so historical judgments survive them: (accepted
/// sources, sources whose receipt is not present locally, errors). The
/// first invalid entry stops the walk and becomes the one error.
pub fn receipt_relocations(registry: &Path, workspace: &Path) -> (BTreeSet<String>, Vec<String>, Vec<String>) {
    let (mut accepted, mut unavailable) = (BTreeSet::new(), Vec::new());
    if !registry.exists() {
        return (accepted, unavailable, Vec::new());
    }
    let result = relocations_into(registry, workspace, &mut accepted, &mut unavailable);
    let errors = result.err().map(|e| vec![format!("receipt relocation registry: {e}")]).unwrap_or_default();
    (accepted, unavailable, errors)
}

fn relocations_into(
    registry: &Path,
    workspace: &Path,
    accepted: &mut BTreeSet<String>,
    unavailable: &mut Vec<String>,
) -> Result<(), String> {
    let data = json::parse(&text::read_utf8(registry)?)?;
    let version_one = matches!(data.get("schema_version"), Some(Json::Int(1) | Json::Bool(true)))
        || matches!(data.get("schema_version"), Some(Json::Float(f)) if *f == 1.0);
    let Some(Json::Arr(receipts)) = data.get("receipts").filter(|_| version_one) else {
        return Err("expected schema_version 1 and receipts list".into());
    };
    for item in receipts {
        let field = |k: &str| item.get(k).map(Json::py_str).unwrap_or_default();
        let (source, destination) = (field("source"), field("destination"));
        let parts = source.split('/').chain(destination.split('/'));
        let valid = SOURCE.is_match(&source)
            && destination.starts_with("Code/testing/mere/")
            && basename(&source) == basename(&destination)
            && !parts.into_iter().any(|p| p == "." || p == "..")
            && !source.contains('\\')
            && !destination.contains('\\')
            && SHA256.is_match(&field("sha256"))
            && SHA1.is_match(&field("git_blob"))
            && SHA1.is_match(&field("source_commit"));
        if !valid || accepted.contains(&source) {
            return Err(format!("invalid or duplicate receipt relocation: {source}"));
        }
        let rest = destination.strip_prefix("Code/").unwrap_or(&destination);
        let target = rest.split('/').fold(workspace.to_path_buf(), |p, c| p.join(c));
        if target.is_file() {
            let bytes = std::fs::read(&target).map_err(|e| format!("{}: {e}", target.display()))?;
            if sha256_hex(&bytes) != field("sha256") {
                return Err(format!("relocated receipt digest mismatch: {source}"));
            }
        } else {
            unavailable.push(source.clone());
        }
        accepted.insert(source);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digest_and_names() {
        assert_eq!(sha256_hex(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        assert_eq!(basename("a/b/c.md"), "c.md");
        assert_eq!(basename("c.md"), "c.md");
        assert!(matches("batch_12_x.md", "batch_"));
        assert!(!matches("batch_12_x.txt", "batch_"));
        assert!(!matches("notes.md", "batch_"));
    }
}
