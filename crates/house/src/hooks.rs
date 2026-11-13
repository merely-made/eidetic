// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! `house hooks [--no-decisions] [--bans DIR]... [--clippy ARGS]... | --check`:
//! the canonical `.pre-commit-config.yaml`, rendered from `template.yaml`.
//! In the template a `#{name` line opens a block kept only when its option
//! is on (`clippy` and `bans` repeat once per pass) and `#}` closes it.

use std::process::ExitCode;

use crate::words;

const TEMPLATE: &str = include_str!("template.yaml");
/// The file a consumer repository carries at its root.
pub const FILE: &str = ".pre-commit-config.yaml";
const HEADER: &str = "# house hooks";
const USAGE: &str = "usage: house hooks [--no-decisions] [--regen] [--bans DIR]... [--deny] [--deny-args ARGS] [--clippy-base ARGS] [--clippy ARGS]... [--local FILE] | house hooks --check";
/// What the first clippy pass runs when a repository names nothing else.
const CLIPPY_BASE: &str = "--all-targets";

/// What a repository's hook file carries beyond the fixed hooks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    /// `house decisions check`; off for repositories with no manifest.
    pub decisions: bool,
    /// One bans pass per workspace directory (`.` is the repository root):
    /// the whole workspace with every feature, read against that directory's
    /// own `deny.toml`, failing on a skip that matches nothing.
    pub bans: Vec<String>,
    /// `cargo deny check bans`: the root with default features. The form
    /// `bans` replaces; it leaves when no repository's file names it.
    pub deny: bool,
    /// Extra `cargo clippy` passes, each its argument string.
    pub clippy: Vec<String>,
    /// Arguments between `cargo deny` and `check bans`; implies `deny`.
    pub deny_args: Option<String>,
    /// `house decisions regen --stage` before the check.
    pub regen: bool,
    /// The first clippy pass's arguments, when not `--all-targets`.
    pub clippy_base: Option<String>,
    /// A repository's own hooks: the file's path and its text, a YAML list
    /// at column 0, appended after the fixed hooks.
    pub local: Option<(String, String)>,
}

impl Default for Options {
    fn default() -> Self {
        Options { decisions: true, deny: false, clippy: Vec::new(), regen: false, clippy_base: None, local: None, deny_args: None, bans: Vec::new() }
    }
}

impl Options {
    /// Parses options; `Ok((options, check))`.
    ///
    /// # Errors
    /// An unknown option, an option with no usable argument, a `--bans`
    /// directory that is not a plain relative path or is named twice, or
    /// `--bans` beside the `--deny` form it replaces.
    pub fn parse(args: &[String]) -> Result<(Options, bool), String> {
        let mut o = Options::default();
        let mut check = false;
        let mut args = args.iter();
        while let Some(a) = args.next() {
            match a.as_str() {
                "--no-decisions" => o.decisions = false,
                "--deny" => o.deny = true,
                "--check" => check = true,
                "--bans" => {
                    let dir = args.next().ok_or("--bans needs a workspace directory")?;
                    if !plain_dir(dir) {
                        return Err(format!("--bans {dir}: name `.` or a relative directory with `/` separators"));
                    }
                    if o.bans.contains(dir) {
                        return Err(format!("--bans {dir} is named twice"));
                    }
                    o.bans.push(dir.clone());
                }
                "--regen" => o.regen = true,
                "--deny-args" => {
                    let extra = args.next().ok_or("--deny-args needs its cargo deny arguments")?;
                    if extra.trim().is_empty() || extra.contains(['\n', '\r']) {
                        return Err("--deny-args needs arguments on one line".into());
                    }
                    o.deny = true;
                    o.deny_args = Some(extra.clone());
                }
                "--clippy-base" => {
                    let base = args.next().ok_or("--clippy-base needs its cargo clippy arguments")?;
                    if base.trim().is_empty() || base.contains(['\n', '\r']) {
                        return Err("--clippy-base needs arguments on one line".into());
                    }
                    o.clippy_base = Some(base.clone());
                }
                "--local" => {
                    let path = args.next().ok_or("--local needs a file")?;
                    let text = std::fs::read_to_string(path).map_err(|e| format!("--local {path}: {e}"))?;
                    o.local = Some((path.clone(), text.replace("\r\n", "\n")));
                }
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
        if o.deny && !o.bans.is_empty() {
            return Err("--bans replaces --deny and --deny-args; name one form".into());
        }
        Ok((o, check))
    }

    /// The options as the first line writes them, each with a leading space.
    pub fn command_line(&self) -> String {
        let mut line = String::new();
        if !self.decisions {
            line.push_str(" --no-decisions");
        }
        if self.regen {
            line.push_str(" --regen");
        }
        for dir in &self.bans {
            line.push_str(" --bans ");
            line.push_str(&words::quote(dir));
        }
        match &self.deny_args {
            Some(extra) => {
                line.push_str(" --deny-args ");
                line.push_str(&words::quote(extra));
            }
            None if self.deny => line.push_str(" --deny"),
            None => {}
        }
        if let Some(base) = &self.clippy_base {
            line.push_str(" --clippy-base ");
            line.push_str(&words::quote(base));
        }
        for c in &self.clippy {
            line.push_str(" --clippy ");
            line.push_str(&words::quote(c));
        }
        if let Some((path, _)) = &self.local {
            line.push_str(" --local ");
            line.push_str(&words::quote(path));
        }
        line
    }

    fn block_on(&self, name: &str) -> Result<usize, String> {
        match name {
            "decisions" => Ok(usize::from(self.decisions)),
            "deny" => Ok(usize::from(self.deny)),
            "regen" => Ok(usize::from(self.regen && self.decisions)),
            "local" => Ok(usize::from(self.local.is_some())),
            "clippy" => Ok(self.clippy.len()),
            "bans" => Ok(self.bans.len()),
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
    let base = self_base(options);
    let deny = options.deny_args.as_ref().map_or_else(|| "check bans".to_string(), |a| format!("{a} check bans"));
    let globals = [("options", opts.as_str()), ("base", base.as_str()), ("deny", deny.as_str())];
    let mut out = String::new();
    let mut lines = TEMPLATE.lines();
    while let Some(line) = lines.next() {
        if let Some(block) = line.strip_prefix("#{") {
            let body: Vec<&str> = lines.by_ref().take_while(|l| *l != "#}").collect();
            let times = options.block_on(block).expect("template block");
            if let (true, Some((path, text))) = (block == "local", &options.local) {
                out.push_str(&format!("      # From {path}:\n"));
                for l in text.lines() {
                    if !l.trim().is_empty() {
                        out.push_str("      ");
                        out.push_str(l);
                    }
                    out.push('\n');
                }
                continue;
            }
            for i in 0..times {
                let pass = match block {
                    "clippy" => options.clippy.get(i).map(|args| clippy_pass(i, args)),
                    "bans" => options.bans.get(i).map(|dir| bans_pass(dir)),
                    _ => None,
                };
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

/// The first clippy pass's arguments, as the entry line writes them.
fn self_base(options: &Options) -> String {
    options.clippy_base.clone().unwrap_or_else(|| CLIPPY_BASE.to_string())
}

/// The `n`, `name` and `entry` of the `i`th extra clippy pass; ids start
/// at `clippy-2`, the fixed pass being the first.
fn clippy_pass(i: usize, args: &str) -> [String; 3] {
    [(i + 2).to_string(), yaml_scalar(&format!("clippy {args}")), yaml_scalar(&format!("cargo clippy {args}"))]
}

/// Whether `dir` is `.` or a relative directory of plain `/`-separated names.
fn plain_dir(dir: &str) -> bool {
    let bad = |c: char| c.is_whitespace() || c == ':' || c == char::from(92);
    let name = |part: &str| !part.is_empty() && part != "." && part != ".." && !part.contains(bad);
    dir == "." || dir.split('/').all(name)
}

/// The `n`, `name` and `entry` of the bans pass over the workspace at `dir`;
/// `n` is the id's suffix, empty for the root.
fn bans_pass(dir: &str) -> [String; 3] {
    const CHECK: &str = "check -D unmatched-skip bans";
    if dir == "." {
        return [String::new(), "dependency bans".into(), format!("cargo deny --workspace --all-features {CHECK}")];
    }
    let entry =
        format!("cargo deny --manifest-path {dir}/Cargo.toml --workspace --all-features --config {dir}/deny.toml {CHECK}");
    [format!("-{}", dir.replace('/', "-")), yaml_scalar(&format!("dependency bans {dir}")), yaml_scalar(&entry)]
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
