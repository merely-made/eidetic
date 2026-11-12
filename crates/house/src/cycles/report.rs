// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! The findings as the script printed them: the text report, `--json`, and
//! the exit code.

use std::collections::{BTreeMap, BTreeSet};

use super::graph::Pair;
use super::judge::Findings;
use super::scan::Edge;
use crate::json::{self, Json};

fn describe(edge: &Edge) -> String {
    let place = if edge.line > 0 { format!("{}:{}", edge.manifest, edge.line) } else { edge.manifest.clone() };
    let optional = if edge.optional { ", optional" } else { "" };
    format!("{} -> {} [{}{optional}] {} ({place})", edge.from, edge.to, edge.kind, edge.dep)
}

fn nodes(pairs: &BTreeSet<Pair>) -> BTreeSet<&str> {
    pairs.iter().flat_map(|(a, b)| [a.as_str(), b.as_str()]).collect()
}

fn section(out: &mut Vec<String>, title: &str, pairs: &BTreeSet<Pair>, rows: &[Edge], witness: &[Vec<String>], verbose: bool) {
    if pairs.is_empty() {
        return;
    }
    let names = nodes(pairs);
    out.push(format!("{title} ({} repositories: {})", names.len(), names.into_iter().collect::<Vec<_>>().join(", ")));
    out.push("  shortest cycles:".into());
    out.extend(witness.iter().map(|c| format!("    {}", c.join(" -> "))));
    out.push("  closing rows (first per repository pair; --verbose for all):".into());
    let mut by_pair: BTreeMap<(&str, &str), Vec<String>> = BTreeMap::new();
    for edge in rows {
        by_pair.entry((&edge.from, &edge.to)).or_default().push(describe(edge));
    }
    for ((from, to), lines) in by_pair {
        let shown = if verbose { lines.len() } else { 1 };
        out.extend(lines.iter().take(shown).map(|l| format!("    {l}")));
        if lines.len() > shown {
            out.push(format!("    ... {} more {from} -> {to} rows", lines.len() - shown));
        }
    }
}

/// The text report; `heads` maps each repository to its `HEAD`, if known.
pub fn text(f: &Findings, heads: &BTreeMap<String, Option<String>>, verbose: bool) -> String {
    let mut out = vec![format!(
        "family repositories scanned: {}; lock roots read: {}; cross-repository manifest edges: {}",
        f.repos.len(),
        f.lock_roots,
        f.edges.len()
    )];
    if !heads.is_empty() {
        let shown: Vec<String> = heads
            .iter()
            .filter_map(|(n, s)| s.as_ref().filter(|s| !s.is_empty()).map(|s| format!("{n}@{}", &s[..s.len().min(9)])))
            .collect();
        out.push(format!("heads: {}", shown.join(", ")));
    }
    out.extend(f.unresolved.iter().map(|r| format!("no resolved versions recorded: {r}")));
    section(&mut out, "FAIL normal/build cycle(s) after the allowlist", &f.strict, &f.strict_rows, &f.strict_witness, verbose);
    if !f.dev.is_empty() {
        let title = if f.dev_is_warning {
            "warning dev-only cycle(s) (--dev-warn)"
        } else {
            "FAIL dev-only cycle(s) (D6: dev edges count)"
        };
        section(&mut out, title, &f.dev, &f.dev_rows, &f.dev_witness, verbose);
    }
    for (root, pairs) in &f.lock_cycles {
        let names: Vec<&str> = nodes(pairs).into_iter().collect();
        let level = if f.strict_locks { "FAIL" } else { "warning" };
        out.push(format!("{level} resolved cycle inside {root}: {}", names.join(", ")));
    }
    if !f.stale.is_empty() {
        out.push("FAIL allowlist entries on no cycle (delete them):".into());
        out.extend(f.stale.iter().map(|s| format!("  {s}")));
    }
    if f.strict.is_empty() && f.dev.is_empty() && f.lock_cycles.is_empty() && f.stale.is_empty() {
        out.push("ok: no repository cycle beyond the allowlist".into());
    }
    out.join("\n")
}

fn pair_list(pairs: &BTreeSet<Pair>) -> Json {
    Json::Arr(pairs.iter().map(|(a, b)| json::strings([a.as_str(), b.as_str()])).collect())
}

fn edge_json(e: &Edge) -> Json {
    #[allow(clippy::cast_possible_wrap)]
    let line = Json::Int(e.line as i64);
    Json::Obj(vec![
        ("from".into(), Json::Str(e.from.clone())),
        ("to".into(), Json::Str(e.to.clone())),
        ("kind".into(), Json::Str(e.kind.into())),
        ("optional".into(), Json::Bool(e.optional)),
        ("manifest".into(), Json::Str(e.manifest.clone())),
        ("dep".into(), Json::Str(e.dep.clone())),
        ("line".into(), line),
    ])
}

fn edges_json(rows: &[Edge]) -> Json {
    Json::Arr(rows.iter().map(edge_json).collect())
}

fn cycles_json(cycles: &[Vec<String>]) -> Json {
    Json::Arr(cycles.iter().map(|c| json::strings(c.iter().map(String::as_str))).collect())
}

/// `--json`: the script's result dictionary, plus `unresolved_roots`.
pub fn json(f: &Findings) -> String {
    #[allow(clippy::cast_possible_wrap)]
    let lock_roots = Json::Int(f.lock_roots as i64);
    let lock_cycles = Json::Obj(f.lock_cycles.iter().map(|(r, p)| (r.clone(), pair_list(p))).collect());
    Json::Obj(vec![
        ("repos".into(), json::strings(f.repos.iter().map(String::as_str))),
        ("edges".into(), edges_json(&f.edges)),
        ("strict".into(), pair_list(&f.strict)),
        ("strict_rows".into(), edges_json(&f.strict_rows)),
        ("strict_witness".into(), cycles_json(&f.strict_witness)),
        ("dev".into(), pair_list(&f.dev)),
        ("dev_rows".into(), edges_json(&f.dev_rows)),
        ("dev_witness".into(), cycles_json(&f.dev_witness)),
        ("dev_is_warning".into(), Json::Bool(f.dev_is_warning)),
        ("lock_cycles".into(), lock_cycles),
        ("stale".into(), json::strings(f.stale.iter().map(String::as_str))),
        ("lock_roots".into(), lock_roots),
        ("unresolved_roots".into(), json::strings(f.unresolved.iter().map(String::as_str))),
        ("strict_locks".into(), Json::Bool(f.strict_locks)),
    ])
    .pretty()
}

/// 1 on a normal/build cycle, a stale entry, a dev cycle unless warned, or a
/// lock cycle under `--strict-locks`.
pub fn exit_code(f: &Findings) -> u8 {
    let failed = !f.strict.is_empty()
        || !f.stale.is_empty()
        || (!f.dev.is_empty() && !f.dev_is_warning)
        || (f.strict_locks && !f.lock_cycles.is_empty());
    u8::from(failed)
}
