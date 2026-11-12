// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! The manifest scan: every `Cargo.toml` of a checkout, the workspace roots
//! they make (each root is a node, D35), and the edges their git rows draw.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;

use super::tree::{self, Toml};
use crate::text;

pub const ORG: &str = "merely-made";
const SKIP_DIRS: [&str; 6] = [".git", ".claude", "target", "node_modules", "fixtures", "receipts"];
const SKIP_PATHS: [&str; 1] = ["support/patches"];
const DEP_TABLES: [(&str, &str); 5] = [
    ("dependencies", "normal"),
    ("build-dependencies", "build"),
    ("dev-dependencies", "dev"),
    ("build_dependencies", "build"),
    ("dev_dependencies", "dev"),
];

static REPO_URL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)github\.com[/:]merely-made/([A-Za-z0-9._-]+?)(?:\.git)?(?:[/#?]|\n?\z)").expect("pattern")
});

/// A git source URL's family repository name; `None` when it is external.
pub fn repo_of_url(url: &str) -> Option<String> {
    REPO_URL.captures(url).map(|c| c[1].to_string())
}

/// One manifest: its path in the repository, parsed data and text.
pub struct Manifest {
    pub rel: String,
    pub data: Toml,
    pub text: String,
}

/// A directory of a checkout, relative (`""` for the top), and its file names.
pub type Walked = Vec<(String, Vec<String>)>;

/// Every directory outside the skipped ones, top first, subdirectories sorted.
/// Symbolic links are not followed.
pub fn walk(root: &Path) -> Walked {
    let mut out = Vec::new();
    walk_into(root, "", &mut out);
    out
}

fn walk_into(dir: &Path, rel: &str, out: &mut Walked) {
    let Ok(read) = std::fs::read_dir(dir) else { return };
    let (mut dirs, mut files) = (Vec::new(), Vec::new());
    for entry in read.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let is_dir = std::fs::metadata(entry.path()).is_ok_and(|m| m.is_dir());
        if !is_dir {
            files.push(name);
        } else if !entry.file_type().is_ok_and(|t| t.is_symlink()) {
            dirs.push(name);
        }
    }
    out.push((rel.to_string(), files));
    dirs.retain(|d| !skipped(rel, d));
    dirs.sort();
    for d in dirs {
        let child = if rel.is_empty() { d.clone() } else { format!("{rel}/{d}") };
        walk_into(&dir.join(&d), &child, out);
    }
}

fn skipped(rel: &str, name: &str) -> bool {
    let joined = if rel.is_empty() { name.to_string() } else { format!("{rel}/{name}") };
    SKIP_DIRS.contains(&name) || SKIP_PATHS.contains(&joined.as_str())
}

/// Whether a repository-relative file path lies inside a skipped directory.
pub fn path_skipped(path: &str) -> bool {
    let parts: Vec<&str> = path.split('/').collect();
    (1..parts.len()).any(|n| skipped(&parts[..n - 1].join("/"), parts[n - 1]))
}

/// The repository-relative path of `file` in a walked directory.
pub fn join(rel: &str, file: &str) -> String {
    if rel.is_empty() { file.to_string() } else { format!("{rel}/{file}") }
}

/// The directory part of a relative path, as `os.path.dirname`.
pub fn dirname(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(d, _)| d)
}

/// The file joined onto a checkout, one component at a time.
pub fn on_disk(repo: &Path, rel: &str) -> std::path::PathBuf {
    rel.split('/').fold(repo.to_path_buf(), |p, c| p.join(c))
}

/// Reads and parses a TOML file, warning (and giving `None`) on failure.
pub fn load(path: &Path) -> Option<Toml> {
    match text::read_sig(path).and_then(|t| tree::parse(&t)) {
        Ok(data) => Some(data),
        Err(e) => {
            eprintln!("warning: cannot read {}: {e}", text::py_path(path));
            None
        }
    }
}

/// The parsed manifests of a checkout, in walk order.
pub fn manifests(repo: &Path, walked: &Walked) -> Vec<Manifest> {
    let mut out = Vec::new();
    for (rel, files) in walked {
        if !files.iter().any(|f| f == "Cargo.toml") {
            continue;
        }
        let rel = join(rel, "Cargo.toml");
        let path = on_disk(repo, &rel);
        if let Some(data) = load(&path) {
            let bytes = std::fs::read(&path).unwrap_or_default();
            let text = String::from_utf8_lossy(&bytes);
            let text = text::universal(text.strip_prefix('\u{feff}').unwrap_or(&text));
            out.push(Manifest { rel, data, text });
        }
    }
    out
}

/// Workspace roots as nodes: a manifest belongs to the deepest `[workspace]`
/// directory at or above it. The top root's node is the repository name; a
/// nested root is `<repository>/<directory>`.
pub struct Roots {
    dirs: HashMap<String, Vec<String>>,
    packages: HashMap<(String, String), String>,
}

impl Roots {
    pub fn new(scanned: &BTreeMap<String, Vec<Manifest>>) -> Roots {
        let mut roots = Roots { dirs: HashMap::new(), packages: HashMap::new() };
        for (name, manifests) in scanned {
            let mut dirs: Vec<String> =
                manifests.iter().filter(|m| m.data.has("workspace")).map(|m| dirname(&m.rel).to_string()).collect();
            dirs.sort();
            dirs.dedup();
            dirs.sort_by_key(|d| std::cmp::Reverse(d.len()));
            roots.dirs.insert(name.clone(), dirs);
            for m in manifests {
                let package = m.data.get("package").and_then(|p| p.get("name")).and_then(Toml::as_str);
                if let Some(package) = package.filter(|p| !p.is_empty()) {
                    let node = roots.node(name, dirname(&m.rel));
                    roots.packages.entry((name.clone(), package.to_string())).or_insert(node);
                }
            }
        }
        roots
    }

    /// The node of a directory of a repository.
    pub fn node(&self, name: &str, directory: &str) -> String {
        for root in self.dirs.get(name).into_iter().flatten() {
            if !root.is_empty() && (directory == root || directory.starts_with(&format!("{root}/"))) {
                return format!("{name}/{root}");
            }
        }
        name.to_string()
    }

    /// The node holding a package, or the repository when none declares it.
    pub fn of_package(&self, repo: &str, package: &str) -> String {
        self.packages.get(&(repo.to_string(), package.to_string())).cloned().unwrap_or_else(|| repo.to_string())
    }

    /// The `[workspace]` directories of a repository, deepest first.
    pub fn workspace_dirs(&self, name: &str) -> &[String] {
        self.dirs.get(name).map_or(&[], Vec::as_slice)
    }
}

/// The repository of a node.
pub fn repo_of_node(node: &str) -> &str {
    node.split_once('/').map_or(node, |(r, _)| r)
}

/// One cross-root manifest row.
#[derive(Clone, Debug, PartialEq)]
pub struct Edge {
    pub from: String,
    pub to: String,
    pub kind: &'static str,
    pub optional: bool,
    pub manifest: String,
    pub dep: String,
    pub line: usize,
}

/// A dependency table: its kind, its name, and its rows.
type DepTable<'a> = (&'static str, String, &'a [(String, Toml)]);

fn dep_tables(data: &Toml) -> Vec<DepTable<'_>> {
    let mut out = Vec::new();
    for (table, kind) in DEP_TABLES {
        if let Some(deps) = data.get(table).and_then(Toml::as_table) {
            out.push((kind, table.to_string(), deps));
        }
    }
    for (cfg, body) in data.get("target").and_then(Toml::as_table).unwrap_or_default() {
        for (table, kind) in DEP_TABLES {
            if let Some(deps) = body.get(table).and_then(Toml::as_table) {
                out.push((kind, format!("target.{cfg}.{table}"), deps));
            }
        }
    }
    out
}

/// The edges of one repository's git rows: a row naming a family repository
/// runs from the manifest's root to the root holding the named package.
/// Path rows are not edges.
pub fn manifest_edges(name: &str, manifests: &[Manifest], roots: &Roots) -> Vec<Edge> {
    let mut workspace_deps: HashMap<&str, &Toml> = HashMap::new();
    for m in manifests {
        if let Some(table) = m.data.get("workspace").and_then(|w| w.get("dependencies")).filter(|t| t.as_table().is_some()) {
            workspace_deps.insert(dirname(&m.rel), table);
        }
    }
    let inherited = |rel: &str, dep: &str| -> Option<&Toml> {
        let mut directory = dirname(rel);
        loop {
            if let Some(found) = workspace_deps.get(directory).and_then(|t| t.get(dep)) {
                return Some(found);
            }
            if directory.is_empty() {
                return None;
            }
            directory = dirname(directory);
        }
    };
    let empty = Toml::Table(Vec::new());
    let mut edges = Vec::new();
    for m in manifests {
        let source = roots.node(name, dirname(&m.rel));
        for (kind, table, deps) in dep_tables(&m.data) {
            for (dep, spec) in deps {
                if spec.as_table().is_none() {
                    continue;
                }
                let optional = spec.get("optional").is_some_and(Toml::truthy);
                let spec = if spec.get("workspace").is_some_and(Toml::truthy) {
                    inherited(&m.rel, dep).filter(|b| b.as_table().is_some()).unwrap_or(&empty)
                } else {
                    spec
                };
                let Some(target_repo) = spec.get("git").and_then(Toml::as_str).and_then(repo_of_url) else {
                    continue;
                };
                let package = spec.get("package").and_then(Toml::as_str).unwrap_or(dep);
                let target = roots.of_package(&target_repo, package);
                if target == source {
                    continue;
                }
                let hint = table.rsplit('.').next().unwrap_or(&table);
                edges.push(Edge {
                    from: source.clone(),
                    to: target,
                    kind,
                    optional,
                    manifest: format!("{name}/{}", m.rel),
                    dep: dep.clone(),
                    line: line_of(&m.text, dep, hint),
                });
            }
        }
    }
    edges
}

/// Best-effort 1-based line of a dependency row, for the report; 0 if none.
pub fn line_of(text: &str, name: &str, table_hint: &str) -> usize {
    let (n, t) = (regex::escape(name), regex::escape(table_hint));
    let patterns = [
        Regex::new(&format!(r#"^\s*"?{n}"?\s*[.=]"#)),
        Regex::new(&format!(r#"^\s*\[[^\]]*{t}[^\]]*\.\s*"?{n}"?\s*\]"#)),
    ];
    let patterns: Vec<Regex> = patterns.into_iter().flatten().collect();
    for (index, line) in text::splitlines(text).into_iter().enumerate() {
        if patterns.iter().any(|p| p.is_match(line)) {
            return index + 1;
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_name_family_repositories() {
        assert_eq!(repo_of_url("https://github.com/merely-made/mere.git").as_deref(), Some("mere"));
        assert_eq!(repo_of_url("git@github.com:merely-made/mere.git").as_deref(), Some("mere"));
        assert_eq!(repo_of_url("https://www.GitHub.com/Merely-Made/knot-editor").as_deref(), Some("knot-editor"));
        assert_eq!(repo_of_url("git+https://github.com/merely-made/genet?branch=main#abc").as_deref(), Some("genet"));
        assert_eq!(repo_of_url("https://github.com/other/mere.git"), None);
    }

    #[test]
    fn line_of_finds_rows_and_headers() {
        let text = "[package]\nname = \"a\"\n\n[dependencies]\nb-crate = { git = \"x\" }\n[dev-dependencies.\"c\"]\ngit = \"y\"\n";
        assert_eq!(line_of(text, "b-crate", "dependencies"), 5);
        assert_eq!(line_of(text, "c", "dev-dependencies"), 6);
        assert_eq!(line_of(text, "zzz", "dependencies"), 0);
    }

    #[test]
    fn skipped_paths() {
        assert!(path_skipped("target/Cargo.lock"));
        assert!(path_skipped("a/fixtures/x/Cargo.lock"));
        assert!(path_skipped("support/patches/p/Cargo.lock"));
        assert!(!path_skipped("Cargo.lock"));
        assert!(!path_skipped("crates/a/Cargo.lock"));
    }
}
