// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! `line-ceiling [--config PATH] FILE...`, run from the repository root.
//! Exits 1 when a file is over the ceiling or an exception is stale, 2 on
//! a configuration or usage error. The body is `line_ceiling::run`.

use std::process::ExitCode;

fn main() -> ExitCode {
    line_ceiling::run(std::env::args().skip(1).collect())
}
