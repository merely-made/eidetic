// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! `house <line-ceiling | decisions | resolved | hooks | cycles | doc-audit> ...`; `house::run` holds it.

fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    house::run(&args)
}
