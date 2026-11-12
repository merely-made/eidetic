// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! `house doc-audit`: D2 judgment coverage of a repository's active design
//! documents. A port of Mere's `scripts/mere_doc_judgment_audit.py`, with the
//! same report, `--json` and exit codes, and its paths as options.

pub mod audit;
pub mod records;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use crate::args::Options;
use crate::json::{self, Json};

/// The sealed legacy snapshot's digest in Mere.
pub const SNAPSHOT_SHA256: &str = "f0b64eef4cd5514157bb1b49d389b84db29a1e823d6f3e2cebf827337fe56abf";

const USAGE: &str = "usage: house doc-audit [--json] [--root DIR] [--docs DIR] [--ledger DIR] [--snapshot PATH] [--corrections PATH] [--moved-out PATH] [--receipt-relocations PATH] [--workspace DIR] [--snapshot-sha256 HEX]";
const VALUED: [&str; 9] =
    ["root", "docs", "ledger", "snapshot", "corrections", "moved-out", "receipt-relocations", "workspace", "snapshot-sha256"];

/// Where the audit reads.
#[derive(Clone, Debug)]
pub struct Paths {
    pub docs: PathBuf,
    pub ledger: PathBuf,
    pub snapshot: PathBuf,
    pub corrections: PathBuf,
    pub moved_out: PathBuf,
    pub receipt_relocations: PathBuf,
    /// The directory a relocated receipt's `Code/...` destination is under.
    pub workspace: PathBuf,
    pub snapshot_sha256: String,
}

impl Paths {
    /// The script's layout under a repository root.
    pub fn under(root: &Path) -> Paths {
        let ledger = root.join("support").join("doc-audit").join("d2");
        Paths {
            docs: root.join("design_docs"),
            snapshot: ledger.join("snapshot_281_aggregate.json"),
            corrections: ledger.join("legacy_corrections.json"),
            moved_out: ledger.join("moved_out.json"),
            ledger,
            receipt_relocations: root.join("tests").join("fixtures").join("doc-audit").join("receipt-relocations.json"),
            workspace: root.parent().and_then(Path::parent).map(Path::to_path_buf).unwrap_or_default(),
            snapshot_sha256: SNAPSHOT_SHA256.to_string(),
        }
    }

    /// The layout with options applied; relative paths are under the root,
    /// and the ledger's files follow `--ledger` unless named.
    fn from_options(opts: &Options) -> Paths {
        let base = crate::resolved::toplevel().or_else(|| std::env::current_dir().ok()).unwrap_or_default();
        let root = opts.value("root").map_or(base, PathBuf::from);
        let root = std::path::absolute(&root).unwrap_or(root);
        let mut p = Paths::under(&root);
        let at = |v: &str| root.join(v);
        if let Some(l) = opts.value("ledger") {
            p.ledger = at(l);
            p.snapshot = p.ledger.join("snapshot_281_aggregate.json");
            p.corrections = p.ledger.join("legacy_corrections.json");
            p.moved_out = p.ledger.join("moved_out.json");
        }
        let set = |slot: &mut PathBuf, key: &str| {
            if let Some(v) = opts.value(key) {
                *slot = at(v);
            }
        };
        set(&mut p.docs, "docs");
        set(&mut p.snapshot, "snapshot");
        set(&mut p.corrections, "corrections");
        set(&mut p.moved_out, "moved-out");
        set(&mut p.receipt_relocations, "receipt-relocations");
        set(&mut p.workspace, "workspace");
        if let Some(h) = opts.value("snapshot-sha256") {
            p.snapshot_sha256 = h.to_string();
        }
        p
    }
}

/// The `doc-audit` command line, without `doc-audit` itself.
pub fn run(args: &[String]) -> ExitCode {
    let opts = match Options::parse(args, &["json"], &VALUED) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("{USAGE}\nhouse doc-audit: error: {e}");
            return ExitCode::from(2);
        }
    };
    let report = match audit::audit(&Paths::from_options(&opts)) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("house doc-audit: {e}");
            return ExitCode::FAILURE;
        }
    };
    if opts.flag("json") {
        println!("{}", to_json(&report).pretty());
    } else {
        print!("{}", text(&report));
    }
    if report.errors.is_empty() { ExitCode::SUCCESS } else { ExitCode::FAILURE }
}

/// The text report, one line each.
pub fn text(r: &audit::Report) -> String {
    let mut out = format!(
        "D2 coverage: {}/{} active; {} legacy + {} supplemental ({} superseding legacy, {} superseding earlier batches); {} inactive legacy, {} inactive supplemental\n",
        r.covered_active_docs,
        r.active_docs,
        r.legacy_active_records,
        r.supplemental_records,
        r.superseded_legacy_records,
        r.superseded_supplemental_records,
        r.inactive_legacy_records,
        r.inactive_supplemental_records.len()
    );
    out.push_str(&format!(
        "Relocated receipts unavailable locally: {}. Coverage is not artifact verification.\n",
        r.unavailable_relocated_receipts.len()
    ));
    for path in &r.unavailable_relocated_receipts {
        out.push_str(&format!("unavailable receipt: {path}\n"));
    }
    for error in &r.errors {
        out.push_str(&format!("error: {error}\n"));
    }
    out
}

#[allow(clippy::cast_possible_wrap)]
fn count(n: usize) -> Json {
    Json::Int(n as i64)
}

/// `--json`: the script's report dictionary, keys in its order.
pub fn to_json(r: &audit::Report) -> Json {
    let strs = |v: &[String]| json::strings(v.iter().map(String::as_str));
    let totals = ["claims", "holds", "stale", "unverifiable"]
        .iter()
        .zip(r.combined_claim_totals)
        .map(|(k, v)| ((*k).to_string(), Json::Int(v)))
        .collect();
    Json::Obj(vec![
        ("active_docs".into(), count(r.active_docs)),
        ("covered_active_docs".into(), count(r.covered_active_docs)),
        ("legacy_records".into(), count(r.legacy_records)),
        ("legacy_snapshot_sha256".into(), r.legacy_snapshot_sha256.clone().map_or(Json::Null, Json::Str)),
        ("legacy_corrections".into(), count(r.legacy_corrections)),
        ("legacy_active_records".into(), count(r.legacy_active_records)),
        ("supplemental_records".into(), count(r.supplemental_records)),
        ("superseded_legacy_records".into(), count(r.superseded_legacy_records)),
        ("superseded_supplemental_records".into(), count(r.superseded_supplemental_records)),
        ("inactive_legacy_records".into(), count(r.inactive_legacy_records)),
        ("combined_records".into(), count(r.combined_records)),
        (
            "combined_dispositions".into(),
            Json::Obj(r.combined_dispositions.iter().map(|(k, v)| (k.clone(), count(*v))).collect()),
        ),
        ("combined_claim_totals".into(), Json::Obj(totals)),
        ("missing_active_records".into(), strs(&r.missing_active_records)),
        ("unknown_supplemental_records".into(), strs(&r.unknown_supplemental_records)),
        ("inactive_supplemental_records".into(), strs(&r.inactive_supplemental_records)),
        ("registered_receipt_relocations".into(), strs(&r.registered_receipt_relocations)),
        ("unavailable_relocated_receipts".into(), strs(&r.unavailable_relocated_receipts)),
        ("errors".into(), strs(&r.errors)),
    ])
}

#[cfg(test)]
mod tests;
