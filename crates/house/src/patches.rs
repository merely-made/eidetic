// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! `house patches --from URL [--branch B] [--forks PREFIX] [--manifest PATH]...
//! | --check`: a consumer's `[patch.crates-io]` rows for forks, written from
//! the upstream's own table into a generated block, keeping only the crates
//! the consumer's graph holds.

pub mod block;
pub mod graph;
pub mod upstream;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use crate::args::Options;
use crate::words;
use upstream::{Row, Rule};

const USAGE: &str = "usage: house patches --from URL [--branch BRANCH] [--forks PREFIX] [--manifest PATH]... [--family-dir DIR | --remote] | house patches --check [--family-dir DIR | --remote]";
const FLAGS: [&str; 2] = ["check", "remote"];
const VALUED: [&str; 5] = ["from", "branch", "forks", "manifest", "family-dir"];
/// Where the owner's patch-only forks live (one repository per crate).
pub const FORKS: &str = "https://github.com/mark-ik/";
const BRANCH: &str = "main";

/// What a block's first line records: the upstream and the rule.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Spec {
    pub from: String,
    pub branch: String,
    pub forks: String,
}

impl Spec {
    /// The options as the first line writes them, each with a leading space.
    pub fn command_line(&self) -> String {
        let mut line = format!(" --from {}", words::quote(&self.from));
        if self.branch != BRANCH {
            line.push_str(&format!(" --branch {}", words::quote(&self.branch)));
        }
        if self.forks != FORKS {
            line.push_str(&format!(" --forks {}", words::quote(&self.forks)));
        }
        line
    }

    fn from_options(o: &Options) -> Result<Spec, String> {
        let from = o.value("from").ok_or("--from names the upstream's git URL")?.to_string();
        let branch = o.value("branch").unwrap_or(BRANCH).to_string();
        let forks = o.value("forks").unwrap_or(FORKS).to_string();
        if [&from, &branch, &forks].iter().any(|v| v.trim().is_empty() || v.contains(['\n', '\r'])) {
            return Err("--from, --branch and --forks need one-line values".into());
        }
        Ok(Spec { from, branch, forks })
    }

    /// Reads a block's first-line options.
    ///
    /// # Errors
    /// Words that do not split, or options other than the three.
    pub fn parse_header(options: &str) -> Result<Spec, String> {
        let words = words::split(options)?;
        let o = Options::parse(&words, &[], &["from", "branch", "forks"])?;
        Spec::from_options(&o)
    }

    fn rule(&self) -> Rule {
        Rule::ForkPrefix(self.forks.clone())
    }
}

/// Where the upstream's manifest is read from.
pub enum Locate {
    /// The checkout under this family directory, at `origin/<branch>`.
    Family(PathBuf),
    /// A shallow clone of the URL.
    Remote,
}

/// The rows `spec` selects from the upstream's current table.
///
/// # Errors
/// The upstream cannot be found or read, or a carried row is refused.
pub fn candidates(spec: &Spec, locate: &Locate) -> Result<Vec<Row>, String> {
    let text = match locate {
        Locate::Family(dir) => {
            let checkout = upstream::find(dir, &spec.from)?;
            upstream::manifest_at(&checkout, &format!("origin/{}", spec.branch))?
        }
        Locate::Remote => {
            let dir = std::env::temp_dir().join(format!("house-patches-{}-{}", std::process::id(), nanos()));
            let text = upstream::remote(&spec.from, &spec.branch, &dir);
            let _ = std::fs::remove_dir_all(&dir);
            text?
        }
    };
    upstream::select(&text, &spec.rule())
}

fn nanos() -> u128 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos())
}

/// The manifest's new text and the candidates it dropped, with graph
/// membership given as `used`.
///
/// # Errors
/// A hand-written row for a carried crate, or what splicing refuses.
pub fn rewrite(
    text: &str,
    spec: &Spec,
    rows: &[Row],
    used: impl FnOnce(&[Row]) -> Result<std::collections::BTreeSet<String>, String>,
) -> Result<(String, Vec<String>), String> {
    let names: Vec<&str> = rows.iter().map(|r| r.name.as_str()).collect();
    let dup = block::hand_written(text, &names)?;
    if !dup.is_empty() {
        return Err(format!("hand-written [patch.crates-io] rows for {} sit outside the block; remove them", dup.join(", ")));
    }
    let used = used(rows)?;
    let (kept, dropped): (Vec<Row>, Vec<Row>) = rows.iter().cloned().partition(|r| used.contains(&r.name));
    let options = spec.command_line();
    let out = block::splice(text, |eol| block::render(&options, &kept, eol))?;
    crate::cycles::tree::parse(&out).map_err(|e| format!("the written manifest is not TOML: {e}"))?;
    Ok((out, dropped.into_iter().map(|r| r.name).collect()))
}

/// Compares a manifest's block with what `spec` writes now.
///
/// # Errors
/// One line naming the rows missing, extra or changed.
pub fn compare(text: &str, found: &block::Found, spec: &Spec, expected_text: &str) -> Result<(), String> {
    let expected = block::find(expected_text)?.ok_or("internal: no block written")?;
    let (old, new) = (&text[found.range.clone()], &expected_text[expected.range.clone()]);
    if old.replace("\r\n", "\n") == new.replace("\r\n", "\n") {
        return Ok(());
    }
    let (old_rows, new_rows) = (block::rows_of(old)?, block::rows_of(new)?);
    let get = |rows: &[(String, crate::cycles::tree::Toml)], k: &str| rows.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone());
    let mut parts = Vec::new();
    let missing: Vec<&str> = new_rows.iter().filter(|(n, _)| get(&old_rows, n).is_none()).map(|(n, _)| n.as_str()).collect();
    let extra: Vec<&str> = old_rows.iter().filter(|(n, _)| get(&new_rows, n).is_none()).map(|(n, _)| n.as_str()).collect();
    let changed: Vec<&str> =
        new_rows.iter().filter(|(n, v)| get(&old_rows, n).is_some_and(|o| &o != v)).map(|(n, _)| n.as_str()).collect();
    for (label, names) in [("missing", missing), ("extra", extra), ("changed", changed)] {
        if !names.is_empty() {
            parts.push(format!("{label} {}", names.join(", ")));
        }
    }
    if parts.is_empty() {
        parts.push("edited".into());
    }
    Err(format!("rows differ from {}'s current table ({}); rerun `house patches{}`", spec.from, parts.join("; "), spec.command_line()))
}

fn read(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    String::from_utf8(bytes).map_err(|e| format!("{}: {e}", path.display()))
}

/// The `patches` command line, without `patches` itself.
pub fn run(args: &[String]) -> ExitCode {
    let opts = match Options::parse(args, &FLAGS, &VALUED) {
        Ok(o) => o,
        Err(e) => return usage(&e),
    };
    if opts.flag("remote") && opts.value("family-dir").is_some() {
        return usage("--remote and --family-dir name two places");
    }
    let checking = opts.flag("check");
    if checking && ["from", "branch", "forks", "manifest"].iter().any(|k| opts.value(k).is_some()) {
        return usage("--check reads its options from each block's first line; name only --family-dir or --remote");
    }
    let spec = if checking { None } else { Some(Spec::from_options(&opts)) };
    let spec = match spec.transpose() {
        Ok(s) => s,
        Err(e) => return usage(&e),
    };
    let locate = if opts.flag("remote") {
        Locate::Remote
    } else {
        let dir = opts.value("family-dir").map(PathBuf::from).or_else(upstream::default_family_dir);
        let Some(dir) = dir else { return fail("not in a git checkout; pass --family-dir or --remote") };
        Locate::Family(std::path::absolute(&dir).unwrap_or(dir))
    };
    let Some(spec) = spec else { return check(&locate) };
    let manifests: Vec<String> = if opts.all("manifest").is_empty() { vec!["Cargo.toml".into()] } else { opts.all("manifest").to_vec() };
    let rows = match candidates(&spec, &locate) {
        Ok(r) => r,
        Err(e) => return fail(&e),
    };
    for m in &manifests {
        let path = Path::new(m);
        let result = read(path)
            .and_then(|text| rewrite(&text, &spec, &rows, |rows| graph::used(path, rows)).map(|done| (text, done)));
        match result {
            Ok((text, (out, dropped))) => {
                if out != text
                    && let Err(e) = std::fs::write(path, &out)
                {
                    return fail(&format!("{m}: {e}"));
                }
                let kept = rows.len() - dropped.len();
                let note = if dropped.is_empty() { String::new() } else { format!("; not in the graph: {}", dropped.join(", ")) };
                println!("{m}: {kept} rows from {}{note}", spec.from);
            }
            Err(e) => return fail(&format!("{m}: {e}")),
        }
    }
    ExitCode::SUCCESS
}

/// `--check`: every tracked manifest carrying a block, from the top level.
fn check(locate: &Locate) -> ExitCode {
    let Some(root) = crate::resolved::toplevel() else { return fail("--check runs inside a git checkout") };
    let listed = crate::resolved::git_out(&["ls-files", "-z", "--", "Cargo.toml", "*/Cargo.toml"], Some(&root)).unwrap_or_default();
    let mut paths: Vec<String> =
        listed.split(|b| *b == 0).filter_map(|p| std::str::from_utf8(p).ok()).filter(|p| !p.is_empty()).map(String::from).collect();
    paths.sort();
    let mut cache: BTreeMap<Spec, Result<Vec<Row>, String>> = BTreeMap::new();
    let (mut seen, mut failed) = (0, false);
    for rel in paths {
        let path = root.join(&rel);
        let outcome = (|| -> Result<bool, String> {
            let text = read(&path)?;
            let Some(found) = block::find(&text)? else { return Ok(false) };
            let spec = Spec::parse_header(&found.options).map_err(|e| format!("first line: {e}"))?;
            let rows = cache.entry(spec.clone()).or_insert_with(|| candidates(&spec, locate)).clone()?;
            let (expected, _) = rewrite(&text, &spec, &rows, |rows| graph::used(&path, rows))?;
            compare(&text, &found, &spec, &expected)?;
            Ok(true)
        })();
        match outcome {
            Ok(had) => seen += usize::from(had),
            Err(e) => {
                eprintln!("house patches: {rel}: {e}");
                failed = true;
            }
        }
    }
    if seen == 0 && !failed {
        return fail("no tracked Cargo.toml carries a `# house patches` block; write one with `house patches --from URL`");
    }
    if failed { ExitCode::FAILURE } else { ExitCode::SUCCESS }
}

fn usage(e: &str) -> ExitCode {
    eprintln!("{USAGE}\nhouse patches: {e}");
    ExitCode::from(2)
}

fn fail(e: &str) -> ExitCode {
    eprintln!("house patches: {e}");
    ExitCode::FAILURE
}

#[cfg(test)]
mod tests;
