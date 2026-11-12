// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! The family's git-hook tooling as one command: the line ceiling and the
//! decisions tool by their library entry points, the resolved-version notes,
//! and the canonical hook file that names them.

pub mod hooks;
pub mod notes;
pub mod resolved;
pub mod words;

use std::process::ExitCode;

const USAGE: &str = "usage: house line-ceiling [--config PATH] FILE... | decisions <check|regen [--stage]|lookup ...|anchors ...> | resolved <record|push|show COMMIT [--write [--force]]> | hooks [--no-decisions] [--deny] [--clippy ARGS]... [--check]";

/// The command line without the program name.
pub fn run(args: &[String]) -> ExitCode {
    let rest = args.get(1..).unwrap_or_default();
    match args.first().map(String::as_str) {
        Some("line-ceiling") => line_ceiling::run(rest.to_vec()),
        Some("decisions") => responsa::cli::run_args(rest),
        Some("resolved") => resolved::run(rest),
        Some("hooks") => hooks::run(rest),
        _ => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
}
