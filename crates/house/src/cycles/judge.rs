// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! The allowlist and the judgement: which cycles remain once the ruled back
//! edges and isolated roots are left out, and which entries sit on no cycle.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

use super::graph::{self, Pair};
use super::scan::{self, Edge};
use super::tree::Toml;
use super::locks::LockEdge;

const STRICT: [&str; 2] = ["normal", "build"];
const EVERYTHING: [&str; 3] = ["normal", "build", "dev"];

/// A ruled back edge: a directed pair of repositories or of root nodes.
pub struct AllowedEdge {
    pub from: String,
    pub to: String,
    pub decision: String,
}

/// A manifest root whose rows are left out of the family-wide graph.
pub struct Isolated {
    pub root: String,
    pub decision: String,
}

#[derive(Default)]
pub struct Allowlist {
    pub edges: Vec<AllowedEdge>,
    pub isolated: Vec<Isolated>,
}

/// Reads `support/family-cycles/allowed.toml`; an absent file allows nothing.
///
/// # Errors
/// An entry missing a required field: the message the script exits with.
pub fn load_allowlist(path: &Path) -> Result<Allowlist, String> {
    if !path.exists() {
        return Ok(Allowlist::default());
    }
    let data = scan::load(path).unwrap_or(Toml::Table(Vec::new()));
    let shown = crate::text::py_path(path);
    let list = |key: &str| match data.get(key) {
        Some(Toml::Arr(a)) => a.clone(),
        _ => Vec::new(),
    };
    let field = |e: &Toml, k: &str| e.get(k).filter(|v| v.truthy()).map(Toml::py_str);
    let mut out = Allowlist::default();
    for entry in list("edge") {
        match (field(&entry, "from"), field(&entry, "to"), field(&entry, "decision"), field(&entry, "reason")) {
            (Some(from), Some(to), Some(decision), Some(_)) => out.edges.push(AllowedEdge { from, to, decision }),
            _ => return Err(format!("{shown}: each [[edge]] needs from, to, decision and reason: {}", entry.py_repr())),
        }
    }
    for entry in list("isolated") {
        match (field(&entry, "root"), field(&entry, "decision"), field(&entry, "reason")) {
            (Some(root), Some(decision), Some(_)) => out.isolated.push(Isolated { root, decision }),
            _ => return Err(format!("{shown}: each [[isolated]] needs root, decision and reason: {}", entry.py_repr())),
        }
    }
    Ok(out)
}

/// What the check found.
pub struct Findings {
    pub repos: Vec<String>,
    pub edges: Vec<Edge>,
    pub strict: BTreeSet<Pair>,
    pub strict_rows: Vec<Edge>,
    pub strict_witness: Vec<Vec<String>>,
    pub dev: BTreeSet<Pair>,
    pub dev_rows: Vec<Edge>,
    pub dev_witness: Vec<Vec<String>>,
    pub dev_is_warning: bool,
    pub lock_cycles: BTreeMap<String, BTreeSet<Pair>>,
    pub stale: Vec<String>,
    pub lock_roots: usize,
    /// Expected lock roots with no note and no lock on disk.
    pub unresolved: Vec<String>,
    pub strict_locks: bool,
}

fn pairs_of<'a>(rows: impl IntoIterator<Item = &'a Edge>, kinds: &[&str]) -> BTreeSet<Pair> {
    rows.into_iter().filter(|e| kinds.contains(&e.kind)).map(|e| (e.from.clone(), e.to.clone())).collect()
}

fn under(manifest: &str, root: &str) -> bool {
    manifest.starts_with(&format!("{}/", root.trim_end_matches('/')))
}

/// Judges the edges and lock graphs against the allowlist.
pub fn judge(
    repos: Vec<String>,
    edges: Vec<Edge>,
    locks: &BTreeMap<String, Vec<LockEdge>>,
    allowlist: &Allowlist,
    dev_warn: bool,
) -> Findings {
    // A later entry for the same pair takes its place, as a dict would.
    let entries: HashMap<(&str, &str), usize> =
        allowlist.edges.iter().enumerate().map(|(i, e)| ((e.from.as_str(), e.to.as_str()), i)).collect();
    let entry_for = |(a, b): &Pair| -> Option<usize> {
        entries.get(&(a.as_str(), b.as_str())).or_else(|| entries.get(&(scan::repo_of_node(a), scan::repo_of_node(b)))).copied()
    };
    let included: Vec<&Edge> =
        edges.iter().filter(|e| !allowlist.isolated.iter().any(|i| under(&e.manifest, &i.root))).collect();
    let remaining = |kinds: &[&str]| -> BTreeSet<Pair> {
        pairs_of(included.iter().copied(), kinds).into_iter().filter(|p| entry_for(p).is_none()).collect()
    };
    let strict_remaining = remaining(&STRICT);
    let all_remaining = remaining(&EVERYTHING);
    let strict_left = graph::cyclic_pairs(&strict_remaining);
    let all_left = graph::cyclic_pairs(&all_remaining);
    let dev_left: BTreeSet<Pair> = all_left.difference(&strict_left).cloned().collect();

    // Which entries sit on a cycle? Judged with everything present.
    let mut used_edges = BTreeSet::new();
    for pair in graph::cyclic_pairs(&pairs_of(included.iter().copied(), &EVERYTHING)) {
        if let Some(i) = entry_for(&pair) {
            used_edges.insert(i);
        }
    }
    let full_with_isolated = graph::cyclic_pairs(&pairs_of(&edges, &EVERYTHING));
    let mut stale = Vec::new();
    for (i, e) in allowlist.edges.iter().enumerate() {
        if !used_edges.contains(&i) {
            stale.push(format!("edge {} -> {} ({})", e.from, e.to, e.decision));
        }
    }
    for entry in &allowlist.isolated {
        let mine = pairs_of(edges.iter().filter(|e| under(&e.manifest, &entry.root)), &EVERYTHING);
        if mine.is_disjoint(&full_with_isolated) {
            stale.push(format!("isolated {} ({})", entry.root, entry.decision));
        }
    }

    let mut lock_cycles = BTreeMap::new();
    for (root, rows) in locks {
        let pairs: BTreeSet<Pair> =
            rows.iter().map(|(a, b, _, _)| (a.clone(), b.clone())).filter(|p| entry_for(p).is_none()).collect();
        let left = graph::cyclic_pairs(&pairs);
        if !left.is_empty() {
            lock_cycles.insert(root.clone(), left);
        }
    }

    let rows_for = |pairs: &BTreeSet<Pair>, kinds: &[&str]| -> Vec<Edge> {
        let mut rows: Vec<Edge> = included
            .iter()
            .filter(|e| kinds.contains(&e.kind) && pairs.contains(&(e.from.clone(), e.to.clone())))
            .map(|e| (*e).clone())
            .collect();
        rows.sort_by(|x, y| (&x.from, &x.to, &x.manifest, &x.dep).cmp(&(&y.from, &y.to, &y.manifest, &y.dep)));
        rows
    };
    let strict_witness_pairs: BTreeSet<Pair> = strict_remaining.intersection(&strict_left).cloned().collect();
    Findings {
        strict_rows: rows_for(&strict_left, &STRICT),
        strict_witness: graph::witnesses(&strict_witness_pairs, None, 8),
        dev_rows: rows_for(&dev_left, &["dev"]),
        dev_witness: graph::witnesses(&all_remaining, Some(&dev_left), 8),
        strict: strict_left,
        dev: dev_left,
        dev_is_warning: dev_warn,
        lock_roots: locks.len(),
        lock_cycles,
        stale,
        repos,
        edges,
        unresolved: Vec::new(),
        strict_locks: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edge(from: &str, to: &str, kind: &'static str, manifest: &str) -> Edge {
        Edge {
            from: from.into(),
            to: to.into(),
            kind,
            optional: false,
            manifest: manifest.into(),
            dep: format!("{to}-crate"),
            line: 0,
        }
    }

    fn allow(edges: &[(&str, &str)], isolated: &[&str]) -> Allowlist {
        Allowlist {
            edges: edges.iter().map(|(f, t)| AllowedEdge { from: (*f).into(), to: (*t).into(), decision: "D1".into() }).collect(),
            isolated: isolated.iter().map(|r| Isolated { root: (*r).into(), decision: "D2".into() }).collect(),
        }
    }

    fn pair_edges() -> Vec<Edge> {
        vec![edge("a", "b", "normal", "a/Cargo.toml"), edge("b", "a", "normal", "b/tools/Cargo.toml")]
    }

    #[test]
    fn a_cycle_fails_with_its_witness() {
        let f = judge(vec!["a".into(), "b".into()], pair_edges(), &BTreeMap::new(), &Allowlist::default(), false);
        assert_eq!(f.strict.len(), 2);
        assert_eq!(f.strict_witness, vec![vec!["a", "b", "a"]]);
        assert_eq!(f.strict_rows.len(), 2);
    }

    #[test]
    fn an_allowlisted_edge_is_not_reported() {
        let f = judge(vec![], pair_edges(), &BTreeMap::new(), &allow(&[("b", "a")], &[]), false);
        assert!(f.strict.is_empty() && f.dev.is_empty() && f.stale.is_empty());
        // A repository pair covers root nodes inside it.
        let edges = vec![edge("a", "b/x", "normal", "a/Cargo.toml"), edge("b/x", "a", "normal", "b/x/Cargo.toml")];
        let f = judge(vec![], edges, &BTreeMap::new(), &allow(&[("b", "a")], &[]), false);
        assert!(f.strict.is_empty() && f.stale.is_empty());
    }

    #[test]
    fn isolated_roots_and_stale_entries() {
        let f = judge(vec![], pair_edges(), &BTreeMap::new(), &allow(&[], &["b/tools"]), false);
        assert!(f.strict.is_empty() && f.stale.is_empty());
        let clean = vec![edge("a", "b", "normal", "a/Cargo.toml")];
        let f = judge(vec![], clean, &BTreeMap::new(), &allow(&[("a", "b")], &["b/tools"]), false);
        assert_eq!(f.stale, ["edge a -> b (D1)", "isolated b/tools (D2)"]);
    }

    #[test]
    fn a_self_contained_repository_has_no_cycle() {
        let edges = vec![edge("a", "a/sub", "normal", "a/Cargo.toml")];
        let f = judge(vec!["a".into()], edges, &BTreeMap::new(), &Allowlist::default(), false);
        assert!(f.strict.is_empty() && f.dev.is_empty() && f.lock_cycles.is_empty());
    }

    #[test]
    fn dev_only_cycles_and_lock_cycles() {
        let edges = vec![edge("a", "b", "normal", "a/Cargo.toml"), edge("b", "a", "dev", "b/Cargo.toml")];
        let f = judge(vec![], edges, &BTreeMap::new(), &Allowlist::default(), true);
        assert!(f.strict.is_empty());
        assert_eq!(f.dev.len(), 2);
        assert_eq!(f.dev_rows.len(), 1);
        assert_eq!(f.dev_witness, vec![vec!["a", "b", "a"]]);
        let lock: Vec<LockEdge> = vec![("a".into(), "b".into(), "a".into(), "b".into()), ("b".into(), "a".into(), "b".into(), "a".into())];
        let locks = BTreeMap::from([("a/Cargo.lock".to_string(), lock)]);
        let f = judge(vec![], Vec::new(), &locks, &Allowlist::default(), false);
        assert_eq!(f.lock_cycles["a/Cargo.lock"].len(), 2);
    }
}
