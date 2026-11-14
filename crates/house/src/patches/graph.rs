// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Which candidate rows a workspace uses: resolve it with every candidate
//! applied (`--config` patches win over the manifest's) and keep the crates
//! that came from the row's git URL.

use std::collections::BTreeSet;
use std::path::Path;
use std::process::{Command, Stdio};

use super::upstream::{Row, basic, normalize};
use crate::json::{self, Json};

/// The `--config` arguments that apply `rows` as `[patch.crates-io]`.
pub fn config_args(rows: &[Row]) -> Vec<String> {
    let mut args = Vec::new();
    for row in rows {
        for (k, v) in &row.fields {
            args.push("--config".to_string());
            args.push(format!("patch.crates-io.{}.{k}={}", row.name, basic(v)));
        }
    }
    args
}

/// The names of `rows` whose crate resolved from the row's git URL, read
/// from `cargo metadata` output.
///
/// # Errors
/// Output that is not metadata JSON.
pub fn used_in(metadata: &str, rows: &[Row]) -> Result<BTreeSet<String>, String> {
    let parsed = json::parse(metadata)?;
    let Some(Json::Arr(packages)) = parsed.get("packages") else {
        return Err("cargo metadata printed no packages".into());
    };
    let mut used = BTreeSet::new();
    for p in packages {
        let (Some(name), Some(source)) = (p.get("name").and_then(Json::as_str), p.get("source").and_then(Json::as_str)) else {
            continue;
        };
        let Some(git) = source.strip_prefix("git+") else { continue };
        if rows.iter().any(|r| r.name == name && normalize(git) == normalize(r.git())) {
            used.insert(name.to_string());
        }
    }
    Ok(used)
}

/// Resolves the workspace at `manifest` with `rows` applied, from the
/// manifest's directory so its `.cargo/config.toml` holds; every target and
/// feature, as the lock's resolve.
///
/// # Errors
/// Cargo failing, with its first error line.
pub fn used(manifest: &Path, rows: &[Row]) -> Result<BTreeSet<String>, String> {
    let manifest = std::path::absolute(manifest).map_err(|e| format!("{}: {e}", manifest.display()))?;
    let dir = manifest.parent().unwrap_or(Path::new("."));
    let out = Command::new("cargo")
        .args(["metadata", "--format-version", "1", "--all-features", "--manifest-path"])
        .arg(&manifest)
        .args(config_args(rows))
        .current_dir(dir)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("cargo metadata: {e}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let first = err.lines().find(|l| l.starts_with("error")).unwrap_or("failed").trim().to_string();
        return Err(format!("cargo metadata for {}: {first}", manifest.display()));
    }
    used_in(&String::from_utf8_lossy(&out.stdout), rows)
}
