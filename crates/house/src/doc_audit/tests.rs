// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! The audit on small fixture trees built in a temp directory.

use std::path::{Path, PathBuf};

use super::{Paths, audit, records, text};

struct Tree(PathBuf);

impl Tree {
    fn new(name: &str) -> Tree {
        let dir = std::env::temp_dir().join(format!("house-audit-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Tree(dir)
    }

    fn write(&self, path: &str, body: &str) {
        let full = self.0.join(path);
        std::fs::create_dir_all(full.parent().unwrap()).unwrap();
        std::fs::write(full, body).unwrap();
    }

    fn paths(&self) -> Paths {
        let mut p = Paths::under(&self.0);
        p.workspace = self.0.join("workspace");
        p
    }
}

impl Drop for Tree {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn record(path: &str, claims: (u32, u32, u32, u32)) -> String {
    let (c, h, s, u) = claims;
    format!(
        "## `{path}`\n\n- disposition: `current`\n- status line: \"Active\" — accurate: yes\n- claims checked: {c} — holds: {h}, stale: {s}, unverifiable: {u}\n\n### Stale claims\n\n### Contradictions\n\n### Recommended action\n\n### Notes\n\n"
    )
}

fn fixture(name: &str) -> Tree {
    let t = Tree::new(name);
    t.write("design_docs/a.md", "# a\n");
    t.write("design_docs/plans/b.md", "# b\n");
    t.write("design_docs/archive_docs/old.md", "# old\n");
    t
}

#[test]
fn an_active_document_without_a_record_is_reported() {
    let t = fixture("missing");
    t.write("support/doc-audit/d2/batch_1.md", &record("a.md", (2, 2, 0, 0)));
    let r = audit::audit(&t.paths()).unwrap();
    assert_eq!(r.active_docs, 2);
    assert_eq!(r.covered_active_docs, 1);
    assert_eq!(r.missing_active_records, ["plans/b.md"]);
    assert_eq!(r.errors, ["active document lacks D2 record: plans/b.md"]);
    let shown = text(&r);
    assert!(shown.starts_with("D2 coverage: 1/2 active; 0 legacy + 1 supplemental"), "{shown}");
    assert!(shown.ends_with("error: active document lacks D2 record: plans/b.md\n"));
}

#[test]
fn a_record_for_a_document_no_longer_active_is_reported() {
    let t = fixture("inactive");
    let batch = [record("a.md", (1, 1, 0, 0)), record("design_docs/plans/b.md", (1, 0, 1, 0)), record("gone.md", (0, 0, 0, 0)), record("old.md", (0, 0, 0, 0))];
    t.write("support/doc-audit/d2/batch_2.md", &batch.concat());
    let r = audit::audit(&t.paths()).unwrap();
    assert!(r.missing_active_records.is_empty());
    assert_eq!(r.unknown_supplemental_records, ["gone.md"]);
    // A record whose document moved into archive_docs stays as history.
    assert_eq!(r.inactive_supplemental_records, ["old.md"]);
    assert_eq!(r.errors, ["supplemental record is not active: gone.md"]);
    assert_eq!(r.combined_claim_totals, [2, 1, 1, 0]);
}

#[test]
fn later_batches_supersede_and_bad_records_error() {
    let t = fixture("batches");
    t.write("support/doc-audit/d2/batch_2.md", &(record("a.md", (1, 1, 0, 0)) + &record("plans/b.md", (3, 1, 1, 0))));
    t.write("support/doc-audit/d2/batch_10.md", &(record("a.md", (2, 2, 0, 0)) + "## `plans/b.md`\n\n- disposition: `nope`\n"));
    let r = audit::audit(&t.paths()).unwrap();
    assert_eq!(r.superseded_supplemental_records, 1);
    assert_eq!(r.combined_claim_totals[0], 5);
    assert_eq!(
        r.errors[..2],
        ["batch_2.md: plans/b.md: counts do not sum (3 != 1+1+0)".to_string(), "batch_10.md: plans/b.md: missing or invalid disposition".to_string()]
    );
}

#[test]
fn the_snapshot_digest_and_legacy_records() {
    let t = fixture("legacy");
    let snapshot = r#"{"docs": {"a.md": {"disposition": "current", "claims": 3, "holds": 3, "stale": 0, "unverifiable": 0}, "plans/b.md": {"claims": 2, "holds": 1}}}"#;
    t.write("support/doc-audit/d2/snapshot_281_aggregate.json", snapshot);
    t.write("support/doc-audit/d2/legacy_corrections.json", r#"{"plans/b.md": {"stale": 1}}"#);
    let mut paths = t.paths();
    let r = audit::audit(&paths).unwrap();
    assert_eq!(r.legacy_active_records, 2);
    assert_eq!(r.errors.len(), 1);
    assert!(r.errors[0].starts_with("snapshot aggregate digest changed: "));
    paths.snapshot_sha256 = records::sha256_hex(snapshot.as_bytes());
    let r = audit::audit(&paths).unwrap();
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    assert_eq!(r.combined_dispositions.get("(missing)"), Some(&1));
    t.write("support/doc-audit/d2/legacy_corrections.json", r#"{"nope.md": {"stale": 1}}"#);
    assert!(audit::audit(&paths).unwrap_err().contains("nope.md"));
}

#[test]
fn receipt_relocations_check_digests() {
    let t = fixture("receipts");
    let body = b"receipt\n";
    t.write("workspace/testing/mere/r.md", "receipt\n");
    let entry = |sha: &str| {
        format!(
            r#"{{"schema_version": 1, "receipts": [{{"source": "mere_docs/testing/r.md", "destination": "Code/testing/mere/r.md", "sha256": "{sha}", "git_blob": "{z}", "source_commit": "{z}"}}, {{"source": "mere_docs/testing/q.md", "destination": "Code/testing/mere/q.md", "sha256": "{sha}", "git_blob": "{z}", "source_commit": "{z}"}}]}}"#,
            z = "0".repeat(40)
        )
    };
    t.write("tests/fixtures/doc-audit/receipt-relocations.json", &entry(&records::sha256_hex(body)));
    let paths = t.paths();
    let (accepted, unavailable, errors) = records::receipt_relocations(&paths.receipt_relocations, Path::new(&paths.workspace));
    assert_eq!(accepted.len(), 2);
    assert_eq!(unavailable, ["mere_docs/testing/q.md"]);
    assert!(errors.is_empty());
    t.write("tests/fixtures/doc-audit/receipt-relocations.json", &entry(&"1".repeat(64)));
    let (_, _, errors) = records::receipt_relocations(&paths.receipt_relocations, &paths.workspace);
    assert_eq!(errors, ["receipt relocation registry: relocated receipt digest mismatch: mere_docs/testing/r.md"]);
}
