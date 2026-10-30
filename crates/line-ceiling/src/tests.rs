// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::*;

/// A fresh directory under the system temp dir, removed on drop.
struct Tree(PathBuf);

impl Tree {
    fn new() -> Tree {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("line-ceiling-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Tree(dir)
    }

    fn file(&self, path: &str, lines: usize) -> String {
        let full = self.0.join(path);
        std::fs::create_dir_all(full.parent().unwrap()).unwrap();
        std::fs::write(full, "x\n".repeat(lines)).unwrap();
        path.to_string()
    }
}

impl Drop for Tree {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn config(text: &str) -> Config {
    Config::parse(text).unwrap()
}

#[test]
fn six_hundred_pass_and_six_hundred_one_fail() {
    let tree = Tree::new();
    let files = vec![tree.file("a.rs", 600), tree.file("b.rs", 601)];
    let report = check(&tree.0, &files, &Config::default()).unwrap();
    assert_eq!(report.over, vec![(601, "b.rs".to_string())]);
    assert!(report.stale.is_empty());
    assert!(report.failed());
}

#[test]
fn counts_lines_as_read_all_lines_does() {
    assert_eq!(count_lines(b""), 0);
    assert_eq!(count_lines(b"a"), 1);
    assert_eq!(count_lines(b"a\n"), 1);
    assert_eq!(count_lines(b"a\nb"), 2);
    assert_eq!(count_lines(b"a\r\nb\r\n"), 2);
    assert_eq!(count_lines(b"a\rb"), 2);
    assert_eq!(count_lines(b"\n"), 1);
    assert_eq!(count_lines(b"\n\n"), 2);
    assert_eq!(count_lines(b"a\r\n\r\nb"), 3);
}

#[test]
fn entries_without_a_reason_are_refused() {
    let e = Config::parse("[[exception]]\npath = \"a.rs\"\nreason = \" \"\n").unwrap_err();
    assert!(e.contains("no reason"), "{e}");
    let e = Config::parse("[[exclude]]\npath = \"vendor/\"\nreason = \"\"\n").unwrap_err();
    assert!(e.contains("no reason"), "{e}");
    assert!(Config::parse("[[exception]]\npath = \"a.rs\"\n").is_err());
}

#[test]
fn malformed_configurations_are_refused() {
    assert!(Config::parse("ceiling = 0").is_err());
    assert!(Config::parse("ceilling = 600").is_err());
    assert!(Config::parse("[[exception]]\npath = \"gen/\"\nreason = \"r\"\n").is_err());
    let twice = "[[exception]]\npath = \"a.rs\"\nreason = \"r\"\n[[exception]]\npath = \"./a.rs\"\nreason = \"r\"\n";
    assert!(Config::parse(twice).is_err());
}

#[test]
fn an_excepted_file_passes() {
    let tree = Tree::new();
    let files = vec![tree.file("src/table.rs", 900)];
    let c = config("[[exception]]\npath = \"src/table.rs\"\nreason = \"generated table\"\n");
    let report = check(&tree.0, &files, &c).unwrap();
    assert!(!report.failed(), "{report}");
}

#[test]
fn excluded_paths_and_other_extensions_are_not_counted() {
    let tree = Tree::new();
    let files = vec![
        tree.file("vendor/big.rs", 900),
        tree.file("vendored.rs", 900),
        tree.file("notes.md", 900),
        tree.file("Makefile", 900),
    ];
    let c = config("[[exclude]]\npath = \"vendor/\"\nreason = \"upstream\"\n");
    let report = check(&tree.0, &files, &c).unwrap();
    assert_eq!(report.over, vec![(900, "vendored.rs".to_string())]);
}

#[test]
fn a_stale_exception_fails() {
    let tree = Tree::new();
    tree.file("small.rs", 400);
    let c = config(
        "[[exception]]\npath = \"small.rs\"\nreason = \"was big\"\n\
         [[exception]]\npath = \"gone.rs\"\nreason = \"was big\"\n",
    );
    let report = check(&tree.0, &[], &c).unwrap();
    assert_eq!(
        report.stale,
        vec![
            Stale::Under { path: "small.rs".into(), lines: 400 },
            Stale::Gone { path: "gone.rs".into() },
        ]
    );
    assert!(report.failed());
}

#[test]
fn paths_are_normalized_and_deduplicated() {
    let tree = Tree::new();
    tree.file("src/big.rs", 700);
    let files = vec!["src\\big.rs".to_string(), "./src/big.rs".to_string()];
    let report = check(&tree.0, &files, &Config::default()).unwrap();
    assert_eq!(report.over, vec![(700, "src/big.rs".to_string())]);
}

#[test]
fn a_named_file_that_is_missing_is_skipped() {
    let tree = Tree::new();
    let report = check(&tree.0, &["deleted.rs".to_string()], &Config::default()).unwrap();
    assert!(!report.failed());
}

#[test]
fn a_missing_configuration_means_the_defaults() {
    let tree = Tree::new();
    assert_eq!(Config::load(&tree.0.join(CONFIG)).unwrap(), Config::default());
}

#[test]
fn ceiling_and_extensions_are_configurable() {
    let tree = Tree::new();
    let files = vec![tree.file("a.rs", 11), tree.file("b.wgsl", 11)];
    let c = config("ceiling = 10\nextensions = [\".wgsl\"]\n");
    let report = check(&tree.0, &files, &c).unwrap();
    assert_eq!(report.over, vec![(11, "b.wgsl".to_string())]);
}
