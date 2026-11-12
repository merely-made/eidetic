// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! `house cycles` and `house doc-audit` against fixture trees: manifest
//! cycles, the allowlist, and each lock taken from the `HEAD` note, else
//! from disk, else reported as missing.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

struct Tree(PathBuf);

impl Tree {
    fn new(name: &str) -> Tree {
        let dir = std::env::temp_dir().join(format!("house-cycles-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("family")).unwrap();
        Tree(dir)
    }

    fn write(&self, path: &str, text: &str) {
        let full = self.0.join(path);
        std::fs::create_dir_all(full.parent().unwrap()).unwrap();
        std::fs::write(full, text).unwrap();
    }

    fn family(&self) -> String {
        self.0.join("family").display().to_string()
    }

    fn manifest(&self, repo: &str, body: &str) {
        self.write(&format!("family/{repo}/Cargo.toml"), &format!("[package]\nname = \"{repo}-crate\"\nversion = \"0.1.0\"\n{body}"));
    }
}

impl Drop for Tree {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn row(repo: &str) -> String {
    format!("{{ git = \"https://github.com/merely-made/{repo}.git\", branch = \"main\" }}")
}

fn git(dir: &Path, args: &[&str]) {
    let hooks = dir.join(".no-hooks");
    let out = Command::new("git")
        .arg("-c")
        .arg(format!("core.hooksPath={}", hooks.display()))
        .args(["-c", "user.name=house test", "-c", "user.email=house@example.invalid", "-c", "commit.gpgsign=false"])
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
}

fn house(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_house")).args(args).current_dir(dir).output().unwrap()
}

fn cycles(t: &Tree, extra: &[&str]) -> (i32, String) {
    let allow = t.0.join("allowed.toml").display().to_string();
    let mut args = vec!["cycles".to_string(), "--family-dir".into(), t.family(), "--allowlist".into(), allow];
    args.extend(extra.iter().map(|s| (*s).to_string()));
    let out = Command::new(env!("CARGO_BIN_EXE_house")).args(&args).current_dir(&t.0).output().unwrap();
    (out.status.code().unwrap(), String::from_utf8(out.stdout).unwrap())
}

#[test]
fn a_two_repository_cycle_fails_and_the_allowlist_clears_it() {
    let t = Tree::new("pair");
    t.manifest("a", &format!("[dependencies]\nb-crate = {}\n", row("b")));
    t.manifest("b", &format!("[dev-dependencies]\na-crate = {}\n", row("a")));
    let (code, out) = cycles(&t, &[]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("FAIL dev-only cycle(s) (D6: dev edges count) (2 repositories: a, b)"), "{out}");
    assert!(out.contains("    a -> b -> a"), "{out}");
    assert!(out.contains("    b -> a [dev] a-crate (b/Cargo.toml:5)"), "{out}");
    let (code, out) = cycles(&t, &["--dev-warn"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("warning dev-only cycle(s) (--dev-warn)"), "{out}");

    t.write("allowed.toml", "[[edge]]\nfrom = \"b\"\nto = \"a\"\ndecision = \"D9\"\nreason = \"ruled\"\n");
    let (code, out) = cycles(&t, &[]);
    assert_eq!(code, 0, "{out}");
    assert!(out.ends_with("ok: no repository cycle beyond the allowlist\n"), "{out}");
    t.write("allowed.toml", "[[edge]]\nfrom = \"b\"\n");
    let out = house(&t.0, &["cycles", "--family-dir", &t.family(), "--allowlist", &t.0.join("allowed.toml").display().to_string()]);
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("each [[edge]] needs from, to, decision and reason: {'from': 'b'}"));
}

#[test]
fn locks_come_from_the_note_then_disk_and_a_missing_one_is_named() {
    let t = Tree::new("locks");
    let lock_cycle = "version = 4\n\n[[package]]\nname = \"a-crate\"\nversion = \"0.1.0\"\ndependencies = [\"b-crate\"]\n\n[[package]]\nname = \"b-crate\"\nversion = \"0.1.0\"\nsource = \"git+https://github.com/merely-made/b?branch=main#1\"\ndependencies = [\"a-crate 0.1.0 (git+https://github.com/merely-made/a?branch=main#2)\"]\n\n[[package]]\nname = \"a-crate\"\nversion = \"0.1.0\"\nsource = \"git+https://github.com/merely-made/a?branch=main#2\"\n";
    // a: a committed note on HEAD, and no lock on disk.
    t.manifest("a", &format!("[dependencies]\nb-crate = {}\n", row("b")));
    let a = t.0.join("family/a");
    git(&a, &["init", "-q"]);
    git(&a, &["add", "Cargo.toml"]);
    git(&a, &["commit", "-q", "-m", "a"]);
    t.write("family/a/Cargo.lock", lock_cycle);
    let rec = house(&a, &["resolved", "record"]);
    assert!(rec.status.success());
    std::fs::remove_file(a.join("Cargo.lock")).unwrap();
    // b: a lock on disk only. c: neither.
    t.manifest("b", "");
    t.write("family/b/Cargo.lock", "version = 4\n");
    t.manifest("c", "[workspace]\n");
    let (code, out) = cycles(&t, &[]);
    assert_eq!(code, 0, "{out}");
    assert!(out.starts_with("family repositories scanned: 3; lock roots read: 2; cross-repository manifest edges: 1\n"), "{out}");
    assert!(out.contains("\nno resolved versions recorded: c/Cargo.lock\n"), "{out}");
    assert!(out.contains("\nwarning resolved cycle inside a/Cargo.lock: a, b\n"), "{out}");
    let (code, _) = cycles(&t, &["--strict-locks"]);
    assert_eq!(code, 1);
    let (_, json) = cycles(&t, &["--json"]);
    assert!(json.contains("\"unresolved_roots\": [\n    \"c/Cargo.lock\"\n  ],"), "{json}");
}

#[test]
fn doc_audit_reports_both_findings() {
    let t = Tree::new("audit");
    t.write("repo/design_docs/a.md", "# a\n");
    t.write("repo/design_docs/b.md", "# b\n");
    let rec = "## `a.md`\n\n- disposition: `current`\n- status line: \"x\" — accurate: yes\n- claims checked: 0 — holds: 0, stale: 0, unverifiable: 0\n\n### Stale claims\n### Contradictions\n### Recommended action\n### Notes\n";
    t.write("repo/support/doc-audit/d2/batch_1.md", &format!("{rec}{}", rec.replace("a.md", "gone.md")));
    let out = house(&t.0, &["doc-audit", "--root", "repo"]);
    assert_eq!(out.status.code(), Some(1));
    let text = String::from_utf8(out.stdout).unwrap();
    assert_eq!(
        text,
        "D2 coverage: 1/2 active; 0 legacy + 2 supplemental (0 superseding legacy, 0 superseding earlier batches); 0 inactive legacy, 0 inactive supplemental\nRelocated receipts unavailable locally: 0. Coverage is not artifact verification.\nerror: active document lacks D2 record: b.md\nerror: supplemental record is not active: gone.md\n"
    );
}
