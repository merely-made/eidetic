// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! The family's repository checks as one command: the line ceiling and the
//! decisions tool by their library entry points, the resolved-version notes,
//! the canonical hook file that names them, the family cycle check and the
//! documentation judgment audit.

pub mod args;
pub mod cycles;
pub mod doc_audit;
pub mod hooks;
pub mod json;
pub mod notes;
pub mod patches;
pub mod resolved;
pub mod text;
pub mod words;

use std::process::ExitCode;

const USAGE: &str = "usage: house line-ceiling [--config PATH] FILE... | decisions <check|regen [--stage]|lookup ...|anchors ...> | resolved <record|push|show COMMIT [--write [--force]]> | hooks [--no-decisions] [--bans DIR]... [--patches] [--clippy ARGS]... [--check] | cycles [--family-dir DIR | --remote] [--repo NAME]... [--allowlist PATH] [--dev-warn] [--strict-locks] [--verbose] [--json] | doc-audit [--json] [--root DIR] [path options] | patches --from URL [--branch B] [--forks PREFIX] [--manifest PATH]... [--family-dir DIR | --remote] | patches --check";

/// The command line without the program name.
pub fn run(args: &[String]) -> ExitCode {
    let rest = args.get(1..).unwrap_or_default();
    match args.first().map(String::as_str) {
        Some("line-ceiling") => line_ceiling::run(rest.to_vec()),
        Some("decisions") => responsa::cli::run_args(rest),
        Some("resolved") => resolved::run(rest),
        Some("hooks") => hooks::run(rest),
        Some("cycles") => cycles::run(rest),
        Some("doc-audit") => doc_audit::run(rest),
        Some("patches") => patches::run(rest),
        _ => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
}
