// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! `house patches` against a synthetic upstream, fork and consumer: local
//! git repositories, path crates and an empty vendored registry, no network.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

struct Tree(PathBuf);

impl Tree {
    fn new(name: &str) -> Tree {
        let dir = std::env::temp_dir().join(format!("house-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Tree(dir)
    }

    fn write(&self, path: &str, text: &str) {
        let full = self.0.join(path);
        std::fs::create_dir_all(full.parent().unwrap()).unwrap();
        std::fs::write(full, text).unwrap();
    }

    fn read(&self, path: &str) -> String {
        std::fs::read_to_string(self.0.join(path)).unwrap()
    }

    fn url(&self, path: &str) -> String {
        let p = self.0.join(path).display().to_string().replace('\\', "/");
        if p.starts_with('/') { format!("file://{p}") } else { format!("file:///{p}") }
    }
}

impl Drop for Tree {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn git(dir: &Path, args: &[&str]) -> Output {
    let out = Command::new("git")
        .args(["-c", "user.name=house test", "-c", "user.email=house@example.invalid", "-c", "commit.gpgsign=false"])
        .args(["-c", "core.autocrlf=false", "-c", "init.defaultBranch=main"])
        .arg("-c")
        .arg(format!("core.hooksPath={}", dir.join(".no-hooks").display()))
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    out
}

fn house(t: &Tree, dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_house"))
        .args(args)
        .current_dir(dir)
        .env("CARGO_HOME", t.0.join("cargo-home"))
        .env_remove("CARGO_TARGET_DIR")
        .output()
        .unwrap()
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

fn package(name: &str, deps: &str) -> String {
    format!("[package]\nname = \"{name}\"\nversion = \"0.5.0\"\nedition = \"2021\"\n\n[dependencies]\n{deps}")
}

fn upstream_manifest(t: &Tree, branch: &str) -> String {
    let fork = t.url("forks/fork");
    let rows: String =
        ["fa", "fb", "fc"].iter().map(|c| format!("{c} = {{ git = \"{fork}\", branch = \"{branch}\" }}\n")).collect();
    format!("[workspace]\n\n[patch.crates-io]\n{rows}family = {{ git = \"https://example.invalid/family.git\" }}\n")
}

/// A fork with three crates on `work` and `next`, an upstream naming them,
/// its checkout under `family/`, and a consumer beside it whose root uses
/// `fa` and whose nested `web` workspace uses `fb`.
fn setup(t: &Tree) -> (String, String, PathBuf) {
    let fork = t.0.join("forks/fork");
    t.write("forks/fork/Cargo.toml", "[workspace]\nmembers = [\"fa\", \"fb\", \"fc\"]\n");
    for c in ["fa", "fb", "fc"] {
        t.write(&format!("forks/fork/{c}/Cargo.toml"), &package(c, ""));
        t.write(&format!("forks/fork/{c}/src/lib.rs"), "");
    }
    git(&fork, &["init", "-q"]);
    git(&fork, &["add", "."]);
    git(&fork, &["commit", "-q", "-m", "fork"]);
    git(&fork, &["branch", "work"]);
    git(&fork, &["branch", "next"]);

    let up = t.0.join("src-up");
    t.write("src-up/Cargo.toml", &upstream_manifest(t, "work"));
    git(&up, &["init", "-q"]);
    git(&up, &["add", "Cargo.toml"]);
    git(&up, &["commit", "-q", "-m", "up"]);
    let from = t.url("src-up");
    std::fs::create_dir_all(t.0.join("family")).unwrap();
    git(&t.0.join("family"), &["clone", "-q", &from, "up"]);

    std::fs::create_dir_all(t.0.join("vendor")).unwrap();
    let vendor = t.0.join("vendor").display().to_string().replace('\\', "/");
    t.write(
        "family/consumer/.cargo/config.toml",
        &format!("[source.crates-io]\nreplace-with = \"none\"\n\n[source.none]\ndirectory = \"{vendor}\"\n"),
    );
    t.write("family/consumer/Cargo.toml", "[workspace]\nmembers = [\"a\"]\nexclude = [\"web\"]\n\n[patch.crates-io]\n# hand\nlocal = { path = \"local\" }\n");
    t.write("family/consumer/a/Cargo.toml", &package("a", "fa = \"0.5\"\n"));
    t.write("family/consumer/a/src/lib.rs", "");
    t.write("family/consumer/web/Cargo.toml", &format!("{}\n[workspace]\n", package("web", "fb = \"0.5\"\n")).replace('\n', "\r\n"));
    t.write("family/consumer/web/src/lib.rs", "");
    t.write("family/consumer/local/Cargo.toml", &package("local", ""));
    t.write("family/consumer/local/src/lib.rs", "");
    let consumer = t.0.join("family/consumer");
    git(&consumer, &["init", "-q"]);
    (from, t.url("forks") + "/", consumer)
}

#[test]
fn write_check_and_every_drift() {
    let t = Tree::new("patches");
    let (from, forks, consumer) = setup(&t);
    let family = t.0.join("family").display().to_string();
    let write = ["patches", "--from", &from, "--forks", &forks, "--family-dir", &family, "--manifest", "Cargo.toml", "--manifest", "web/Cargo.toml"];
    let before_web = t.read("family/consumer/web/Cargo.toml");
    let out = house(&t, &consumer, &write);
    assert!(out.status.success(), "{}", stderr(&out));
    let root = t.read("family/consumer/Cargo.toml");
    let web = t.read("family/consumer/web/Cargo.toml");
    assert!(root.contains(&format!("fa = {{ git = \"{}\", branch = \"work\" }}\n", t.url("forks/fork"))), "{root}");
    assert!(!root.contains("fb =") && !root.contains("fc =") && !root.contains("family ="));
    assert!(root.contains(&format!("# house patches --from {from} --forks {forks}\n")));
    assert!(web.contains("fb = {") && !web.contains("fa = {") && web.contains("# end house patches\r\n"));
    assert!(web.starts_with(&before_web), "the CRLF file keeps its bytes and gains a table");
    assert!(String::from_utf8_lossy(&out.stdout).contains("not in the graph: fb, fc"));

    // Idempotent.
    assert!(house(&t, &consumer, &write).status.success());
    assert_eq!(t.read("family/consumer/Cargo.toml"), root);

    git(&consumer, &["add", "."]);
    let check = ["patches", "--check", "--family-dir", &family];
    let ok = house(&t, &consumer, &check);
    assert!(ok.status.success(), "{}", stderr(&ok));

    // From a linked worktree, with no family directory named.
    git(&consumer, &["commit", "-q", "-m", "c"]);
    let wt = t.0.join("wt");
    git(&consumer, &["worktree", "add", "-q", wt.to_str().unwrap()]);
    let from_wt = house(&t, &wt, &["patches", "--check"]);
    assert!(from_wt.status.success(), "{}", stderr(&from_wt));

    // A row edited by hand.
    t.write("family/consumer/Cargo.toml", &root.replace("branch = \"work\"", "branch = \"wurk\""));
    let bad = house(&t, &consumer, &check);
    assert_eq!(bad.status.code(), Some(1));
    assert_eq!(stderr(&bad).lines().count(), 1, "{}", stderr(&bad));
    assert!(stderr(&bad).contains("Cargo.toml: rows differ") && stderr(&bad).contains("changed fa"));
    t.write("family/consumer/Cargo.toml", &root);

    // The upstream moves its rows to another branch.
    t.write("src-up/Cargo.toml", &upstream_manifest(&t, "next"));
    git(&t.0.join("src-up"), &["commit", "-q", "-am", "next"]);
    git(&t.0.join("family/up"), &["fetch", "-q"]);
    let moved = house(&t, &consumer, &check);
    assert_eq!(moved.status.code(), Some(1));
    assert_eq!(stderr(&moved).lines().count(), 2, "both manifests drift: {}", stderr(&moved));
    assert!(house(&t, &consumer, &write).status.success());
    assert!(t.read("family/consumer/Cargo.toml").contains("branch = \"next\""));
    assert!(house(&t, &consumer, &check).status.success());

    // A crate leaves the graph.
    t.write("family/consumer/a/Cargo.toml", &package("a", ""));
    let left = house(&t, &consumer, &check);
    assert_eq!(left.status.code(), Some(1));
    assert!(stderr(&left).contains("extra fa"), "{}", stderr(&left));

    // A hand-written row for a carried crate is refused.
    t.write("family/consumer/a/Cargo.toml", &package("a", "fa = \"0.5\"\n"));
    let hand = t.read("family/consumer/Cargo.toml").replace("# hand\n", "# hand\nfc = { path = \"x\" }\n");
    t.write("family/consumer/Cargo.toml", &hand);
    let refused = house(&t, &consumer, &write);
    assert_eq!(refused.status.code(), Some(1));
    assert!(stderr(&refused).contains("fc") && stderr(&refused).contains("outside the block"));
    assert_eq!(t.read("family/consumer/Cargo.toml"), hand);
}

#[test]
fn check_takes_no_block_options() {
    let t = Tree::new("patches-usage");
    let out = house(&t, &t.0, &["patches", "--check", "--from", "x"]);
    assert_eq!(out.status.code(), Some(2));
    let out = house(&t, &t.0, &["patches", "--branch", "main"]);
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn the_hook_option_round_trips() {
    let t = Tree::new("patches-hooks");
    let out = house(&t, &t.0, &["hooks", "--no-decisions", "--patches"]);
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.contains("        entry: house patches --check\n"));
    t.write(".pre-commit-config.yaml", &text);
    let check = house(&t, &t.0, &["hooks", "--check"]);
    assert!(check.status.success(), "{}", stderr(&check));
}
