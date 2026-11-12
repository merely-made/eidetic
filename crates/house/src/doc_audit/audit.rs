// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! The coverage judgement: every active document has a record, every
//! supplemental record names an active (or recorded as moved) document, and
//! the sealed legacy snapshot is unchanged.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use super::Paths;
use super::records::{self, Record, basename};
use crate::json::{self, Json};

const FIELDS: [&str; 4] = ["claims", "holds", "stale", "unverifiable"];

/// What the audit found; the script's report dictionary.
#[derive(Debug, Default)]
pub struct Report {
    pub active_docs: usize,
    pub covered_active_docs: usize,
    pub legacy_records: usize,
    pub legacy_snapshot_sha256: Option<String>,
    pub legacy_corrections: usize,
    pub legacy_active_records: usize,
    pub supplemental_records: usize,
    pub superseded_legacy_records: usize,
    pub superseded_supplemental_records: usize,
    pub inactive_legacy_records: usize,
    pub combined_records: usize,
    pub combined_dispositions: BTreeMap<String, usize>,
    pub combined_claim_totals: [i64; 4],
    pub missing_active_records: Vec<String>,
    pub unknown_supplemental_records: Vec<String>,
    pub inactive_supplemental_records: Vec<String>,
    pub registered_receipt_relocations: Vec<String>,
    pub unavailable_relocated_receipts: Vec<String>,
    pub errors: Vec<String>,
}

type LegacyRecord = Vec<(String, Json)>;

fn read_json(path: &std::path::Path) -> Result<Option<Json>, String> {
    if !path.exists() {
        return Ok(None);
    }
    let text = crate::text::read_utf8(path)?;
    json::parse(&text).map(Some).map_err(|e| format!("{}: {e}", path.display()))
}

fn field<'a>(record: &'a LegacyRecord, key: &str) -> Option<&'a Json> {
    record.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

/// Runs the audit. An absent snapshot, corrections file or moved-out list
/// counts as empty, so the audit runs in a repository without them.
///
/// # Errors
/// An unreadable input, or a correction naming no legacy record.
pub fn audit(paths: &Paths) -> Result<Report, String> {
    let (legacy, snapshot_hash, corrections) = load_legacy(paths)?;
    let index: HashMap<&str, usize> = legacy.iter().enumerate().map(|(i, (p, _))| (p.as_str(), i)).collect();
    let in_legacy = |name: &str| index.contains_key(name);
    let active: Vec<String> = records::markdown_under(&paths.docs)
        .into_iter()
        .filter(|p| !p.split('/').any(|part| part == "archive_docs"))
        .collect();
    let (supplements, mut errors, superseded_supplements) = records::supplements(&paths.ledger)?;
    if let Some(hash) = &snapshot_hash
        && *hash != paths.snapshot_sha256
    {
        errors.push(format!("snapshot aggregate digest changed: {hash} != {}", paths.snapshot_sha256));
    }

    let mut basenames: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for path in &active {
        basenames.entry(basename(path)).or_default().push(path);
    }
    // Full paths disambiguate supplemental records; only a legacy
    // basename-only record is ambiguous when active paths share it.
    for (name, paths) in &basenames {
        if paths.len() > 1 && in_legacy(name) {
            errors.push(format!("duplicate active basename {name}: {}", paths.join(", ")));
        }
    }
    let root_names: BTreeSet<&str> =
        legacy.iter().map(|(n, _)| n.as_str()).filter(|n| !n.contains('/') && !n.contains('\\')).collect();
    let legacy_active: BTreeSet<&str> = active
        .iter()
        .map(String::as_str)
        .filter(|p| in_legacy(p) || (!p.contains('/') && root_names.contains(basename(p))))
        .collect();
    let supplemental_active: BTreeSet<&str> =
        active.iter().map(String::as_str).filter(|p| supplements.contains_key(*p)).collect();
    let covered: BTreeSet<&str> = legacy_active.union(&supplemental_active).copied().collect();
    let missing: Vec<String> = active.iter().filter(|p| !covered.contains(p.as_str())).cloned().collect();
    // A record whose document moved into archive_docs, left the repository
    // with its subject, or is a relocated receipt remains as history.
    let archived: BTreeSet<String> =
        records::markdown_under(&paths.docs.join("archive_docs")).iter().map(|p| basename(p).to_string()).collect();
    let moved = match read_json(&paths.moved_out)? {
        Some(data) => data.get("moved").cloned().ok_or_else(|| format!("{}: no \"moved\" key", paths.moved_out.display()))?,
        None => Json::Arr(Vec::new()),
    };
    let moved_out = |path: &str| match &moved {
        Json::Arr(items) => items.iter().any(|i| i.as_str() == Some(path)),
        Json::Obj(entries) => entries.iter().any(|(k, _)| k == path),
        Json::Str(s) => s.contains(path),
        _ => false,
    };
    let (relocated, unavailable, relocation_errors) = records::receipt_relocations(&paths.receipt_relocations, &paths.workspace);
    errors.extend(relocation_errors);
    let active_set: BTreeSet<&str> = active.iter().map(String::as_str).collect();
    let not_active: Vec<&String> = supplements.keys().filter(|p| !active_set.contains(p.as_str())).collect();
    let (inactive, unknown): (Vec<&String>, Vec<&String>) = not_active
        .into_iter()
        .partition(|p| archived.contains(basename(p)) || moved_out(p) || relocated.contains(*p));
    // A batch record for a path the snapshot covers supersedes it (S34).
    let superseded: Vec<&str> = legacy_active.intersection(&supplemental_active).copied().collect();
    let superseded_keys: BTreeSet<&str> =
        superseded.iter().map(|p| if in_legacy(p) { *p } else { basename(p) }).collect();
    errors.extend(missing.iter().map(|p| format!("active document lacks D2 record: {p}")));
    errors.extend(unknown.iter().map(|p| format!("supplemental record is not active: {p}")));

    let inactive_legacy = legacy
        .iter()
        .filter(|(n, _)| {
            let bare = !n.contains('/') && !n.contains('\\');
            !active_set.contains(n.replace('\\', "/").as_str()) && !(bare && basenames.contains_key(n.as_str()))
        })
        .count();
    let kept_legacy: Vec<&LegacyRecord> =
        legacy.iter().filter(|(p, _)| !superseded_keys.contains(p.as_str())).map(|(_, r)| r).collect();
    let (dispositions, totals) = tally(&kept_legacy, &supplements);
    errors.extend(unsummed(&legacy));
    Ok(Report {
        active_docs: active.len(),
        covered_active_docs: covered.len(),
        legacy_records: legacy.len(),
        legacy_snapshot_sha256: snapshot_hash,
        legacy_corrections: corrections,
        legacy_active_records: legacy_active.len(),
        supplemental_records: supplements.len(),
        superseded_legacy_records: superseded.len(),
        superseded_supplemental_records: superseded_supplements,
        inactive_legacy_records: inactive_legacy,
        combined_records: kept_legacy.len() + supplements.len(),
        combined_dispositions: dispositions,
        combined_claim_totals: totals,
        missing_active_records: missing,
        unknown_supplemental_records: unknown.into_iter().cloned().collect(),
        inactive_supplemental_records: inactive.into_iter().cloned().collect(),
        registered_receipt_relocations: relocated.into_iter().collect(),
        unavailable_relocated_receipts: unavailable,
        errors,
    })
}

type Legacy = (Vec<(String, LegacyRecord)>, Option<String>, usize);

/// The snapshot's records with the corrections applied, the snapshot's
/// digest, and how many corrections there were.
fn load_legacy(paths: &Paths) -> Result<Legacy, String> {
    let mut legacy: Vec<(String, LegacyRecord)> = Vec::new();
    let mut snapshot_hash = None;
    if paths.snapshot.exists() {
        let bytes = std::fs::read(&paths.snapshot).map_err(|e| format!("{}: {e}", paths.snapshot.display()))?;
        snapshot_hash = Some(records::sha256_hex(&bytes));
        let text = String::from_utf8(bytes).map_err(|e| format!("{}: {e}", paths.snapshot.display()))?;
        let snapshot = json::parse(&text).map_err(|e| format!("{}: {e}", paths.snapshot.display()))?;
        for (path, record) in snapshot.get("docs").and_then(Json::as_obj).unwrap_or_default() {
            legacy.push((path.clone(), record.as_obj().unwrap_or_default().to_vec()));
        }
    }
    let index: HashMap<String, usize> = legacy.iter().enumerate().map(|(i, (p, _))| (p.clone(), i)).collect();
    let corrections = read_json(&paths.corrections)?.unwrap_or(Json::Obj(Vec::new()));
    let corrections = corrections.as_obj().unwrap_or_default();
    for (path, correction) in corrections {
        let Some(&i) = index.get(path) else {
            return Err(format!("legacy correction names an unknown record: {path}"));
        };
        for f in FIELDS {
            if let Some(value) = correction.get(f) {
                let record = &mut legacy[i].1;
                match record.iter_mut().find(|(k, _)| k == f) {
                    Some(slot) => slot.1 = value.clone(),
                    None => record.push((f.to_string(), value.clone())),
                }
            }
        }
    }
    Ok((legacy, snapshot_hash, corrections.len()))
}

/// The combined records' dispositions and claim totals.
fn tally(legacy: &[&LegacyRecord], supplements: &BTreeMap<String, Record>) -> (BTreeMap<String, usize>, [i64; 4]) {
    let mut dispositions: BTreeMap<String, usize> = BTreeMap::new();
    let mut totals = [0i64; 4];
    for record in legacy {
        let d = field(record, "disposition").map_or_else(|| "(missing)".to_string(), Json::py_str);
        *dispositions.entry(d).or_default() += 1;
        for (t, f) in totals.iter_mut().zip(FIELDS) {
            *t += field(record, f).map_or(0, Json::py_int);
        }
    }
    for record in supplements.values() {
        *dispositions.entry(record.disposition.clone()).or_default() += 1;
        for (t, v) in totals.iter_mut().zip([record.claims, record.holds, record.stale, record.unverifiable]) {
            *t += v;
        }
    }
    (dispositions, totals)
}

/// Legacy records whose counts do not sum once corrected.
fn unsummed(legacy: &[(String, LegacyRecord)]) -> Vec<String> {
    let mut errors = Vec::new();
    for (path, record) in legacy {
        let classified: i64 = FIELDS[1..].iter().map(|f| field(record, f).map_or(0, Json::py_int)).sum();
        let claims = field(record, "claims");
        if claims.map_or(0, Json::py_int) != classified {
            let shown = claims.map_or_else(|| "0".to_string(), Json::py_str);
            errors.push(format!("legacy record counts do not sum after corrections: {path} ({shown} != {classified})"));
        }
    }
    errors
}
