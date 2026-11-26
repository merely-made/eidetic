// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! A fixed OpenSSH command for one locally configured updater.

use std::{error::Error, io, path::PathBuf};

fn run() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args_os().skip(1);
    let policy = PathBuf::from(
        args.next()
            .ok_or("usage: luggage-ssh-update /path/to/policy.json")?,
    );
    if !policy.is_absolute() || args.next().is_some() {
        return Err("usage: luggage-ssh-update /path/to/policy.json".into());
    }
    println!("{}", luggage::ssh::serve(&policy, io::stdin().lock())?);
    Ok(())
}

fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("luggage-ssh-update: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}
