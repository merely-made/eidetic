// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use std::collections::BTreeSet;

use super::upstream::{Row, Rule, normalize, select};
use super::*;

const FORK: &str = "https://github.com/mark-ik/oxigraph";

const UPSTREAM: &str = r#"[package]
name = "up"

[patch.crates-io]
# The fork.
oxttl = { git = "https://github.com/mark-ik/oxigraph", branch = "mark-ik/0.5" }
oxrdf = { git = "https://github.com/mark-ik/oxigraph", branch = "mark-ik/0.5" }
sparopt = { git = "https://github.com/mark-ik/oxigraph", branch = "mark-ik/0.5" }
eidetic = { git = "https://github.com/merely-made/eidetic.git", branch = "main" }
local = { path = "support/patches/local" }

[profile.dev]
opt-level = 0
"#;

fn spec() -> Spec {
    Spec { from: "https://github.com/merely-made/mere.git".into(), branch: "main".into(), forks: FORKS.into() }
}

fn rows() -> Vec<Row> {
    select(UPSTREAM, &spec().rule()).unwrap()
}

fn only(names: &[&str]) -> impl FnOnce(&[Row]) -> Result<BTreeSet<String>, String> {
    let set: BTreeSet<String> = names.iter().map(|s| (*s).to_string()).collect();
    move |_| Ok(set)
}

#[test]
fn the_rule_takes_fork_rows_sorted() {
    let r = rows();
    let names: Vec<&str> = r.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, ["oxrdf", "oxttl", "sparopt"]);
    assert_eq!(r[0].line(), format!(r#"oxrdf = {{ git = "{FORK}", branch = "mark-ik/0.5" }}"#));
    let family = select(UPSTREAM, &Rule::ForkPrefix("https://github.com/merely-made/".into())).unwrap();
    assert_eq!(family.len(), 1);
    assert!(select("[package]\nname = \"x\"\n", &spec().rule()).unwrap().is_empty());
}

#[test]
fn pinned_fork_rows_are_refused() {
    let pinned = format!("[patch.crates-io]\noxrdf = {{ git = \"{FORK}\", rev = \"abc\" }}\n");
    assert!(select(&pinned, &spec().rule()).unwrap_err().contains("rev"));
}

#[test]
fn urls_compare_by_place() {
    let want = normalize("https://github.com/merely-made/mere.git");
    for u in ["https://github.com/Merely-Made/mere", "git@github.com:merely-made/mere.git", "ssh://git@github.com/merely-made/mere/", "git+https://github.com/merely-made/mere?branch=main#abc"] {
        assert_eq!(normalize(u), want, "{u}");
    }
    assert_ne!(normalize("https://github.com/merely-made/mere-rdf"), want);
}

#[test]
fn header_round_trips() {
    let s = Spec { branch: "next".into(), forks: "https://example.com/f/".into(), ..spec() };
    assert_eq!(Spec::parse_header(&s.command_line()).unwrap(), s);
    assert_eq!(spec().command_line(), " --from https://github.com/merely-made/mere.git");
    assert!(Spec::parse_header(" --from x --check").is_err());
    assert!(Spec::parse_header(" --branch main").is_err());
}

#[test]
fn rows_outside_the_graph_are_dropped() {
    let text = "[package]\nname = \"c\"\n";
    let (out, dropped) = rewrite(text, &spec(), &rows(), only(&["oxrdf", "oxttl"])).unwrap();
    assert_eq!(dropped, ["sparopt"]);
    assert!(out.contains("\noxrdf = ") && out.contains("\noxttl = ") && !out.contains("sparopt"));
    assert!(out.starts_with(text));
    assert!(out.contains("\n[patch.crates-io]\n# house patches --from https://github.com/merely-made/mere.git\n"));
    assert!(out.ends_with("# end house patches\n"));
}

#[test]
fn writing_is_idempotent_and_keeps_every_other_byte() {
    for eol in ["\n", "\r\n"] {
        let text = ["[package]", "name = \"c\"", "", "[patch.crates-io]", "# hand", "eidetic = { git = \"e\" }", "", "# next table", "[profile.dev]", "opt-level = 1", ""]
            .join(eol);
        let (once, _) = rewrite(&text, &spec(), &rows(), only(&["oxrdf"])).unwrap();
        let (twice, _) = rewrite(&once, &spec(), &rows(), only(&["oxrdf"])).unwrap();
        assert_eq!(once, twice);
        let found = block::find(&once).unwrap().unwrap();
        let outside = format!("{}{}", &once[..found.range.start], &once[found.range.end..]);
        assert_eq!(outside, text);
        let block_text = &once[found.range.clone()];
        assert!(block_text.lines().all(|l| !l.ends_with('\r') || eol == "\r\n"));
        assert_eq!(block_text.matches(eol).count(), 4);
        // After the hand rows, before the next table's comment.
        assert!(once.find("eidetic =").unwrap() < found.range.start && found.range.end < once.find("# next table").unwrap());
        // A different graph rewrites the block in place.
        let (wider, _) = rewrite(&once, &spec(), &rows(), only(&["oxrdf", "sparopt"])).unwrap();
        assert!(wider.contains("sparopt = ") && wider.starts_with(&once[..found.range.start]));
    }
}

#[test]
fn a_hand_written_duplicate_is_refused() {
    let text = "[patch.crates-io]\noxrdf = { git = \"elsewhere\" }\n";
    let err = rewrite(text, &spec(), &rows(), only(&["oxrdf"])).unwrap_err();
    assert!(err.contains("oxrdf") && !err.contains('\n'), "{err}");
    let dotted = "[patch.crates-io.oxttl]\ngit = \"elsewhere\"\n";
    assert!(rewrite(dotted, &spec(), &rows(), only(&[])).unwrap_err().contains("oxttl"));
}

#[test]
fn malformed_blocks_are_refused() {
    let opened = "[patch.crates-io]\n# house patches --from x\noxrdf = { git = \"y\" }\n";
    assert!(block::find(opened).unwrap_err().contains("no end line"));
    let outside = "[dependencies]\n# house patches --from x\n# end house patches\n";
    assert!(block::find(outside).unwrap_err().contains("not inside"));
    let inline = "patch.crates-io.a = { git = \"y\" }\n";
    assert!(block::splice(inline, |_| String::new()).unwrap_err().contains("[patch.crates-io]"));
}

#[test]
fn compare_names_what_drifted() {
    let s = spec();
    let (written, _) = rewrite("", &s, &rows(), only(&["oxrdf", "oxttl"])).unwrap();
    let found = block::find(&written).unwrap().unwrap();
    assert_eq!(compare(&written, &found, &s, &written), Ok(()));
    let crlf = written.replace('\n', "\r\n");
    assert_eq!(compare(&crlf, &block::find(&crlf).unwrap().unwrap(), &s, &written), Ok(()));
    // The upstream renamed its branch.
    let renamed: Vec<Row> = rows()
        .into_iter()
        .map(|mut r| {
            r.fields[1].1 = "mark-ik/0.6".into();
            r
        })
        .collect();
    let (now, _) = rewrite(&written, &s, &renamed, only(&["oxrdf", "oxttl"])).unwrap();
    let err = compare(&written, &found, &s, &now).unwrap_err();
    assert!(err.contains("changed oxrdf, oxttl") && !err.contains('\n'), "{err}");
    // A crate left the graph; another joined it.
    let (now, _) = rewrite(&written, &s, &rows(), only(&["oxrdf", "sparopt"])).unwrap();
    let err = compare(&written, &found, &s, &now).unwrap_err();
    assert!(err.contains("missing sparopt") && err.contains("extra oxttl"), "{err}");
    // Spacing edited by hand.
    let edited = written.replacen("oxrdf = { git", "oxrdf = {git", 1);
    let err = compare(&edited, &block::find(&edited).unwrap().unwrap(), &s, &written).unwrap_err();
    assert!(err.contains("(edited)"), "{err}");
}

#[test]
fn config_args_apply_each_field() {
    let args = graph::config_args(&rows()[..1]);
    assert_eq!(args, ["--config", &format!("patch.crates-io.oxrdf.git=\"{FORK}\""), "--config", "patch.crates-io.oxrdf.branch=\"mark-ik/0.5\""]);
}

#[test]
fn membership_is_by_name_and_fork_source() {
    let meta = format!(
        r#"{{"packages":[
        {{"name":"oxrdf","source":"git+{FORK}?branch=mark-ik%2F0.5#abc"}},
        {{"name":"oxttl","source":"registry+https://github.com/rust-lang/crates.io-index"}},
        {{"name":"sparopt","source":null}},
        {{"name":"other","source":"git+{FORK}#abc"}}]}}"#
    );
    let used = graph::used_in(&meta, &rows()).unwrap();
    assert_eq!(used.into_iter().collect::<Vec<_>>(), ["oxrdf"]);
}
