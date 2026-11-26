// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Write a signed release upload frame for an OpenSSH connection.

use std::{io, path::PathBuf};

fn main() -> std::process::ExitCode {
    let mut args = std::env::args_os().skip(1).map(PathBuf::from);
    match (args.next(), args.next(), args.next()) {
        (Some(manifest), Some(artifact), None) => {
            match luggage::ssh::write_publication(&manifest, &artifact, io::stdout().lock()) {
                Ok(()) => return std::process::ExitCode::SUCCESS,
                Err(error) => eprintln!("luggage-ssh-publish: {error}"),
            }
        }
        _ => eprintln!("usage: luggage-ssh-publish MANIFEST ARTIFACT"),
    }
    std::process::ExitCode::FAILURE
}
