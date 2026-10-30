// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! `line-ceiling [--config PATH] FILE...`, run from the repository root.
//! Exits 1 when a file is over the ceiling or an exception is stale, 2 on
//! a configuration or usage error.

use std::path::PathBuf;
use std::process::ExitCode;

use line_ceiling::{CONFIG, Config, check};

fn main() -> ExitCode {
    match run(std::env::args().skip(1).collect()) {
        Ok(failed) => ExitCode::from(u8::from(failed)),
        Err(e) => {
            eprintln!("line-ceiling: {e}");
            ExitCode::from(2)
        }
    }
}

fn run(args: Vec<String>) -> Result<bool, String> {
    let mut config_path = PathBuf::from(CONFIG);
    let mut files = Vec::new();
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--config" => config_path = args.next().ok_or("--config needs a path")?.into(),
            "-h" | "--help" => return Err("usage: line-ceiling [--config PATH] FILE...".into()),
            _ => files.push(arg),
        }
    }
    let root = std::env::current_dir().map_err(|e| e.to_string())?;
    let config = Config::load(&config_path)?;
    let report = check(&root, &files, &config)?;
    eprint!("{report}");
    Ok(report.failed())
}
