// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

fn args(a: &[&str]) -> Vec<String> {
    a.iter().map(|s| s.to_string()).collect()
}

fn options(a: &[&str]) -> Options {
    let (o, check) = Options::parse(&args(a)).unwrap();
    assert!(!check);
    o
}

/// Lines in `with` that are not in `base`, and the reverse, as a multiset diff.
fn diff(base: &str, with: &str) -> (Vec<String>, Vec<String>) {
    let mut removed: Vec<String> = base.lines().map(String::from).collect();
    let mut added = Vec::new();
    for l in with.lines() {
        match removed.iter().position(|r| r == l) {
            Some(i) => {
                removed.remove(i);
            }
            None => added.push(l.to_string()),
        }
    }
    (added, removed)
}

#[test]
fn default_carries_every_fixed_hook() {
    let out = render(&Options::default());
    assert!(out.starts_with("# house hooks\n"));
    for want in [
        "fail_fast: true",
        "default_install_hook_types: [pre-commit, post-commit, pre-push]",
        "        entry: house line-ceiling",
        "        types: [text]",
        "        entry: house decisions check",
        "        entry: cargo clippy --all-targets",
        r"        files: '(^|/)(Cargo\.toml|[^/]+\.rs)$'",
        "        entry: house resolved record",
        "        stages: [post-commit]",
        "        entry: house resolved push",
        "        stages: [pre-push]",
    ] {
        assert!(out.lines().any(|l| l == want), "missing {want:?}");
    }
    assert!(!out.contains("#{") && !out.contains("#}") && !out.contains("{{"));
    assert!(!out.contains("cargo deny"));
    // `house` runs from the PATH: no hook installs or names a private copy.
    assert!(!out.contains("additional_dependencies") && !out.contains("language: rust"));
    assert_eq!(out.matches("        entry: house ").count(), 4);
}

#[test]
fn only_the_resolved_hooks_leave_the_default_stage() {
    let every = options(&["--deny", "--clippy", "-p a"]);
    for o in [Options::default(), every] {
        let out = render(&o);
        assert!(out.lines().any(|l| l == "default_stages: [pre-commit]"));
        let staged: Vec<(&str, &str)> = out
            .split("      - id: ")
            .skip(1)
            .filter_map(|hook| {
                let id = hook.lines().next().unwrap();
                hook.lines().find_map(|l| l.trim().strip_prefix("stages: ")).map(|s| (id, s))
            })
            .collect();
        assert_eq!(staged, [("record-resolved", "[post-commit]"), ("push-resolved", "[pre-push]")]);
    }
}

#[test]
fn no_decisions_drops_only_that_hook() {
    let base = render(&Options::default());
    let (added, removed) = diff(&base, &render(&options(&["--no-decisions"])));
    assert_eq!(added, ["# house hooks --no-decisions"]);
    assert_eq!(removed[0], "# house hooks");
    assert!(removed.iter().any(|l| l == "        entry: house decisions check"));
    assert!(removed.iter().any(|l| l == "      - id: decisions"));
    assert_eq!(removed.len(), 1 + 6);
}

#[test]
fn deny_adds_only_the_bans_hook() {
    let base = render(&Options::default());
    let (added, removed) = diff(&base, &render(&options(&["--deny"])));
    assert_eq!(removed, ["# house hooks"]);
    assert_eq!(added[0], "# house hooks --deny");
    assert_eq!(
        &added[1..],
        [
            "      - id: dependency-bans",
            "        name: dependency bans",
            "        entry: cargo deny check bans",
            "        language: system",
            "        pass_filenames: false",
            "        always_run: true",
        ]
    );
}

#[test]
fn each_clippy_adds_one_pass_after_the_first() {
    let extra = "--all-targets -p mere-rdf --features mere-rdf/query";
    let out = render(&options(&["--clippy", extra, "--clippy", "-p x"]));
    let first = out.lines().next().unwrap();
    assert_eq!(first, format!("# house hooks --clippy '{extra}' --clippy '-p x'"));
    let base = render(&Options::default());
    let (added, removed) = diff(&base, &out);
    assert_eq!(removed, ["# house hooks"]);
    // The header, then six lines per pass.
    assert_eq!(added.len(), 1 + 2 * 6);
    assert!(out.contains(&format!("      - id: clippy-2\n        name: clippy {extra}\n        entry: cargo clippy {extra}\n")));
    assert!(out.contains("      - id: clippy-3\n        name: clippy -p x\n        entry: cargo clippy -p x\n"));
    let at = |s: &str| out.find(s).unwrap();
    assert!(at("id: clippy\n") < at("id: clippy-2") && at("id: clippy-2") < at("id: clippy-3"));
}

#[test]
fn risky_scalars_are_quoted() {
    assert_eq!(yaml_scalar("cargo clippy -p a"), "cargo clippy -p a");
    assert_eq!(yaml_scalar("cargo clippy -- -D x: y"), "'cargo clippy -- -D x: y'");
    assert_eq!(yaml_scalar("a #b's"), "'a #b''s'");
}

#[test]
fn check_passes_on_its_own_output() {
    for a in [&[][..], &["--no-decisions"], &["--deny", "--clippy", "--all-targets -p a --features a/q"]] {
        let text = render(&options(a));
        assert_eq!(check_text(&text), Ok(()), "{a:?}");
        assert_eq!(check_text(&text.replace('\n', "\r\n")), Ok(()));
    }
}

#[test]
fn check_fails_on_an_edited_file() {
    let text = render(&options(&["--deny"]));
    let edited = text.replace("cargo deny check bans", "cargo deny check");
    let err = check_text(&edited).unwrap_err();
    assert!(err.contains("house hooks --deny") && !err.contains('\n'), "{err}");
    let options_changed = text.replacen("# house hooks --deny", "# house hooks", 1);
    assert!(check_text(&options_changed).is_err());
    assert!(check_text("fail_fast: true\n").unwrap_err().contains("does not start with"));
    assert!(check_text("# house hooks --check\n").is_err());
    assert!(check_text("# house hookss\n").is_err());
}

#[test]
fn bad_options_are_refused() {
    assert!(Options::parse(&args(&["--clippy"])).is_err());
    assert!(Options::parse(&args(&["--clippy", " "])).is_err());
    assert!(Options::parse(&args(&["--frob"])).is_err());
    assert!(Options::parse(&args(&["--check"])).unwrap().1);
}
