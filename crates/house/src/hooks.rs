// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! `house hooks [--no-decisions] [--deny] [--clippy ARGS]... | --check`:
//! the canonical `.pre-commit-config.yaml`, rendered from `template.yaml`.
//! In the template a `#{name` line opens a block kept only when its option
//! is on (`clippy` repeats once per extra pass) and `#}` closes it.

use std::process::ExitCode;

use crate::words;

const TEMPLATE: &str = include_str!("template.yaml");
/// The file a consumer repository carries at its root.
pub const FILE: &str = ".pre-commit-config.yaml";
/// The prek install spec every `house` hook names.
pub const INSTALL: &str =
    concat!("cli:https://github.com/merely-made/eidetic:house-v", env!("CARGO_PKG_VERSION"), ":house");
const HEADER: &str = "# house hooks";
const USAGE: &str = "usage: house hooks [--no-decisions] [--deny] [--clippy ARGS]... | house hooks --check";

/// What a repository's hook file carries beyond the fixed hooks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    /// `house decisions check`; off for repositories with no manifest.
    pub decisions: bool,
    /// `cargo deny check bans`.
    pub deny: bool,
    /// Extra `cargo clippy` passes, each its argument string.
    pub clippy: Vec<String>,
}

impl Default for Options {
    fn default() -> Self {
        Options { decisions: true, deny: false, clippy: Vec::new() }
    }
}

impl Options {
    /// Parses options; `Ok((options, check))`.
    ///
    /// # Errors
    /// An unknown option, or `--clippy` with no usable argument.
    pub fn parse(args: &[String]) -> Result<(Options, bool), String> {
        let mut o = Options::default();
        let mut check = false;
        let mut args = args.iter();
        while let Some(a) = args.next() {
            match a.as_str() {
                "--no-decisions" => o.decisions = false,
                "--deny" => o.deny = true,
                "--check" => check = true,
                "--clippy" => {
                    let extra = args.next().ok_or("--clippy needs its cargo clippy arguments")?;
                    if extra.trim().is_empty() || extra.contains(['\n', '\r']) {
                        return Err("--clippy needs arguments on one line".into());
                    }
                    o.clippy.push(extra.clone());
                }
                _ => return Err(USAGE.into()),
            }
        }
        Ok((o, check))
    }

    /// The options as the first line writes them, each with a leading space.
    pub fn command_line(&self) -> String {
        let mut line = String::new();
        if !self.decisions {
            line.push_str(" --no-decisions");
        }
        if self.deny {
            line.push_str(" --deny");
        }
        for c in &self.clippy {
            line.push_str(" --clippy ");
            line.push_str(&words::quote(c));
        }
        line
    }

    fn block_on(&self, name: &str) -> Result<usize, String> {
        match name {
            "decisions" => Ok(usize::from(self.decisions)),
            "deny" => Ok(usize::from(self.deny)),
            "clippy" => Ok(self.clippy.len()),
            _ => Err(format!("template: unknown block {name}")),
        }
    }
}

/// Replaces each `{{key}}` in `line` with its value, in one left-to-right pass.
fn fill(line: &str, vars: &[(&str, &str)]) -> String {
    let mut out = String::new();
    let mut rest = line;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let found = after.find("}}").and_then(|end| {
            let key = &after[..end];
            vars.iter().find(|(k, _)| *k == key).map(|(_, v)| (end, *v))
        });
        match found {
            Some((end, value)) => {
                out.push_str(value);
                rest = &after[end + 2..];
            }
            None => {
                out.push_str("{{");
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// A YAML plain scalar when one reads back unchanged, else single-quoted.
fn yaml_scalar(s: &str) -> String {
    let risky = s.contains(": ")
        || s.contains(" #")
        || s.ends_with(':')
        || s.contains('\t')
        || s.starts_with(|c: char| "-?:,[]{}#&*!|>'\"%@`".contains(c) || c.is_whitespace())
        || s.ends_with(char::is_whitespace);
    if risky { format!("'{}'", s.replace('\'', "''")) } else { s.to_string() }
}

/// The hook file for `options`.
///
/// # Panics
/// Never for the template the crate ships; a malformed one is a build bug
/// the tests catch.
pub fn render(options: &Options) -> String {
    let opts = options.command_line();
    let globals = [("options", opts.as_str()), ("install", INSTALL)];
    let mut out = String::new();
    let mut lines = TEMPLATE.lines();
    while let Some(line) = lines.next() {
        if let Some(block) = line.strip_prefix("#{") {
            let body: Vec<&str> = lines.by_ref().take_while(|l| *l != "#}").collect();
            let times = options.block_on(block).expect("template block");
            for i in 0..times {
                let pass = (block == "clippy").then(|| clippy_pass(i, &options.clippy[i]));
                let mut vars = globals.to_vec();
                if let Some([n, name, entry]) = &pass {
                    vars.extend([("n", n.as_str()), ("name", name.as_str()), ("entry", entry.as_str())]);
                }
                for l in &body {
                    out.push_str(&fill(l, &vars));
                    out.push('\n');
                }
            }
            continue;
        }
        out.push_str(&fill(line, &globals));
        out.push('\n');
    }
    out
}

/// The `n`, `name` and `entry` of the `i`th extra clippy pass; ids start
/// at `clippy-2`, the fixed pass being the first.
fn clippy_pass(i: usize, args: &str) -> [String; 3] {
    [(i + 2).to_string(), yaml_scalar(&format!("clippy {args}")), yaml_scalar(&format!("cargo clippy {args}"))]
}

/// Checks a hook file against what its own first line says would render.
///
/// # Errors
/// A one-line reason: no `# house hooks` first line, unreadable options, or
/// a body that differs from the rendering.
pub fn check_text(text: &str) -> Result<(), String> {
    let text = text.replace("\r\n", "\n");
    let first = text.lines().next().unwrap_or("");
    let rest = first
        .strip_prefix(HEADER)
        .filter(|r| r.is_empty() || r.starts_with(char::is_whitespace))
        .ok_or_else(|| format!("{FILE} does not start with `{HEADER}`; write it with `house hooks`"))?;
    let words = words::split(rest).map_err(|e| format!("{FILE}: first line: {e}"))?;
    let (options, check) = Options::parse(&words).map_err(|e| format!("{FILE}: first line: {e}"))?;
    if check {
        return Err(format!("{FILE}: first line names --check"));
    }
    if text == render(&options) {
        Ok(())
    } else {
        Err(format!(
            "{FILE} differs from what `house hooks{}` writes; rerun it into the file",
            options.command_line()
        ))
    }
}

/// The `hooks` command line, without `hooks` itself.
pub fn run(args: &[String]) -> ExitCode {
    let (options, check) = match Options::parse(args) {
        Ok(parsed) => parsed,
        Err(e) => {
            eprintln!("house hooks: {e}");
            return ExitCode::from(2);
        }
    };
    if !check {
        print!("{}", render(&options));
        return ExitCode::SUCCESS;
    }
    if options != Options::default() {
        eprintln!("house hooks: --check reads its options from the file's first line; pass none");
        return ExitCode::from(2);
    }
    let result = std::fs::read_to_string(FILE).map_err(|e| format!("{FILE}: {e}")).and_then(|t| check_text(&t));
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("house hooks: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests;
