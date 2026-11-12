// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! `house cycles`: refuse repository-level dependency cycles in the family.
//! A port of Mere's `scripts/check_family_cycles.py` (cycle breaks plan P0,
//! D6, D35), with the same options, report and exit codes.
//!
//! Family-wide, from manifests: each workspace root is a node, and every
//! `git = "https://github.com/merely-made/<repo>"` row is an edge tagged
//! normal, build or dev. Per lock root, from the resolved graph: a cycle
//! inside one lock warns (fails under `--strict-locks`). Locks come from the
//! `refs/notes/resolved` note on each checkout's `HEAD`, else from disk.

pub mod discover;
pub mod graph;
pub mod judge;
pub mod locks;
pub mod report;
pub mod scan;
pub mod tree;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use crate::args::Options;

const USAGE: &str = "usage: house cycles [--family-dir DIR] [--remote] [--repo NAME]... [--allowlist PATH] [--dev-warn] [--strict-locks] [--verbose] [--json]";
const FLAGS: [&str; 5] = ["remote", "dev-warn", "strict-locks", "verbose", "json"];
const VALUED: [&str; 3] = ["family-dir", "repo", "allowlist"];

/// The `cycles` command line, without `cycles` itself.
pub fn run(args: &[String]) -> ExitCode {
    let opts = match Options::parse(args, &FLAGS, &VALUED) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("{USAGE}\nhouse cycles: error: {e}");
            return ExitCode::from(2);
        }
    };
    // The script's checkout; here, the repository holding the current directory.
    let base = crate::resolved::toplevel().or_else(|| std::env::current_dir().ok()).unwrap_or_default();
    let allowlist_path =
        opts.value("allowlist").map_or_else(|| base.join("support/family-cycles/allowed.toml"), PathBuf::from);
    let allowlist = match judge::load_allowlist(&allowlist_path) {
        Ok(a) => a,
        Err(e) => return fail(&e),
    };
    let only: BTreeSet<String> = opts.all("repo").iter().cloned().collect();
    let mut temp = None;
    let repos = if opts.flag("remote") {
        let dir = std::env::temp_dir().join(format!("family-cycles-{}-{}", std::process::id(), nanos()));
        if let Err(e) = std::fs::create_dir_all(&dir) {
            return fail(&format!("{}: {e}", dir.display()));
        }
        temp = Some(dir.clone());
        match discover::remote(&dir, &only) {
            Ok(r) => r,
            Err(e) => {
                let _ = std::fs::remove_dir_all(&dir);
                return fail(&e);
            }
        }
    } else {
        let given = opts.value("family-dir").map(PathBuf::from);
        let family = given.unwrap_or_else(|| base.parent().map(Path::to_path_buf).unwrap_or_default());
        let family = std::path::absolute(&family).unwrap_or(family);
        let mut repos = match discover::local(&family) {
            Ok(r) => r,
            Err(e) => return fail(&e),
        };
        if !only.is_empty() {
            repos.retain(|n, _| only.contains(n));
        }
        if repos.is_empty() {
            eprintln!("no family checkouts under {}; skipping the family-wide scope", crate::text::py_path(&family));
            return ExitCode::SUCCESS;
        }
        repos
    };
    let heads: BTreeMap<String, Option<String>> = repos.iter().map(|(n, p)| (n.clone(), discover::head(p))).collect();
    let mut findings = analyse(&repos, &allowlist, opts.flag("dev-warn"));
    findings.strict_locks = opts.flag("strict-locks");
    if opts.flag("json") {
        println!("{}", report::json(&findings));
    } else {
        println!("{}", report::text(&findings, &heads, opts.flag("verbose")));
    }
    if let Some(dir) = temp {
        let _ = std::fs::remove_dir_all(dir);
    }
    ExitCode::from(report::exit_code(&findings))
}

fn fail(message: &str) -> ExitCode {
    eprintln!("{message}");
    ExitCode::FAILURE
}

fn nanos() -> u128 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos())
}

/// Scans the checkouts and judges their graphs against the allowlist.
pub fn analyse(repos: &BTreeMap<String, PathBuf>, allowlist: &judge::Allowlist, dev_warn: bool) -> judge::Findings {
    let mut walked = BTreeMap::new();
    let mut scanned = BTreeMap::new();
    for (name, path) in repos {
        let w = scan::walk(path);
        scanned.insert(name.clone(), scan::manifests(path, &w));
        walked.insert(name.clone(), w);
    }
    let roots = scan::Roots::new(&scanned);
    let (mut edges, mut lock_graphs, mut unresolved) = (Vec::new(), BTreeMap::new(), Vec::new());
    for (name, path) in repos {
        edges.extend(scan::manifest_edges(name, &scanned[name], &roots));
        let expected = locks::expected(&scanned[name], &roots, name);
        let found = locks::choose(locks::head_note(path).as_deref(), locks::on_disk(path, &walked[name]), &expected);
        for (rel, text, source) in found.locks {
            let shown = match source {
                locks::Source::Disk => crate::text::py_path(&scan::on_disk(path, &rel)),
                locks::Source::Note => format!("{name}/{rel} (refs/notes/{} on HEAD)", crate::resolved::REF),
            };
            if let Some(found) = locks::lock_edges(name, &shown, &text, &roots) {
                lock_graphs.insert(format!("{name}/{rel}"), found);
            }
        }
        unresolved.extend(found.missing.into_iter().map(|m| format!("{name}/{m}")));
    }
    let mut findings = judge::judge(repos.keys().cloned().collect(), edges, &lock_graphs, allowlist, dev_warn);
    findings.unresolved = unresolved;
    findings
}
