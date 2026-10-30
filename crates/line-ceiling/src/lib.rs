// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! The source-size ceiling (mere:stack.7). A source file over the ceiling
//! fails unless its path is excluded (generated or upstream) or excepted,
//! each entry with its reason. An exception whose file is now under the
//! ceiling or gone fails too, so the inventory shrinks only on purpose.

use std::fmt;
use std::path::Path;

use serde::Deserialize;

/// The configuration file a repository keeps at its root.
pub const CONFIG: &str = "line-ceiling.toml";
/// The ceiling when the configuration names none.
pub const CEILING: usize = 600;
/// Counted extensions when the configuration names none: isocosm's set.
const EXTENSIONS: &[&str] = &[
    "rs", "ps1", "py", "js", "mjs", "ts", "wgsl", "wesl", "lua", "glsl", "sh", "html", "css",
];

/// A declared path and the reason it is declared.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub path: String,
    pub reason: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Raw {
    ceiling: Option<usize>,
    extensions: Option<Vec<String>>,
    #[serde(default)]
    exclude: Vec<Entry>,
    #[serde(default)]
    exception: Vec<Entry>,
}

/// A repository's ceiling, counted extensions, exclusions and exceptions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub ceiling: usize,
    pub extensions: Vec<String>,
    /// Generated or upstream sources: a directory (`vendor/`) or one file.
    pub exclude: Vec<Entry>,
    /// Single files allowed over the ceiling.
    pub exception: Vec<Entry>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            ceiling: CEILING,
            extensions: EXTENSIONS.iter().map(|e| e.to_string()).collect(),
            exclude: Vec::new(),
            exception: Vec::new(),
        }
    }
}

impl Config {
    /// Parses and validates a configuration.
    ///
    /// # Errors
    /// Malformed TOML, an unknown field, a zero ceiling, an entry without a
    /// reason, a directory listed as an exception, or a path excepted twice.
    pub fn parse(text: &str) -> Result<Config, String> {
        let raw: Raw = toml::from_str(text).map_err(|e| e.to_string())?;
        let mut config = Config::default();
        if let Some(ceiling) = raw.ceiling {
            config.ceiling = ceiling;
        }
        if let Some(extensions) = raw.extensions {
            config.extensions = extensions.iter().map(|e| e.trim_start_matches('.').to_string()).collect();
        }
        config.exclude = raw.exclude.into_iter().map(normalized_entry).collect();
        config.exception = raw.exception.into_iter().map(normalized_entry).collect();
        config.validate()?;
        Ok(config)
    }

    /// Reads the configuration at `path`; a missing file means the defaults.
    ///
    /// # Errors
    /// An unreadable file, or any error [`Config::parse`] reports.
    pub fn load(path: &Path) -> Result<Config, String> {
        match std::fs::read_to_string(path) {
            Ok(text) => Config::parse(&text).map_err(|e| format!("{}: {e}", path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
            Err(e) => Err(format!("{}: {e}", path.display())),
        }
    }

    fn validate(&self) -> Result<(), String> {
        if self.ceiling == 0 {
            return Err("ceiling must be above zero".into());
        }
        for (kind, entries) in [("exclude", &self.exclude), ("exception", &self.exception)] {
            if let Some(e) = entries.iter().find(|e| e.reason.trim().is_empty()) {
                return Err(format!("{kind} {} has no reason", e.path));
            }
        }
        if let Some(e) = self.exception.iter().find(|e| e.path.ends_with('/')) {
            return Err(format!("exception {} is a directory; exclude it or list its files", e.path));
        }
        for (i, e) in self.exception.iter().enumerate() {
            if self.exception[..i].iter().any(|f| f.path == e.path) {
                return Err(format!("exception {} is listed twice", e.path));
            }
        }
        Ok(())
    }

    fn counted(&self, path: &str) -> bool {
        let name = path.rsplit('/').next().unwrap_or(path);
        name.rsplit_once('.').is_some_and(|(_, ext)| self.extensions.iter().any(|e| e == ext))
    }

    fn excluded(&self, path: &str) -> bool {
        self.exclude.iter().any(|e| covers(&e.path, path))
    }

    fn excepted(&self, path: &str) -> bool {
        self.exception.iter().any(|e| e.path == path)
    }
}

fn normalized_entry(e: Entry) -> Entry {
    Entry { path: normalize(&e.path), reason: e.reason }
}

/// A repository-relative path with forward slashes and no leading `./`.
fn normalize(path: &str) -> String {
    let path = path.replace('\\', "/");
    path.trim_start_matches("./").to_string()
}

/// Whether a declared path covers `path`: itself, or anything beneath it.
fn covers(declared: &str, path: &str) -> bool {
    let dir = declared.trim_end_matches('/');
    path == dir || path.strip_prefix(dir).is_some_and(|rest| rest.starts_with('/'))
}

/// Physical lines as .NET's `ReadAllLines` counts them: `\n`, `\r\n` and a
/// lone `\r` each end a line, and a final unterminated line counts.
pub fn count_lines(bytes: &[u8]) -> usize {
    let mut lines = 0;
    let mut open = false;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\r' => {
                lines += 1;
                open = false;
                if bytes.get(i + 1) == Some(&b'\n') {
                    i += 1;
                }
            }
            b'\n' => {
                lines += 1;
                open = false;
            }
            _ => open = true,
        }
        i += 1;
    }
    lines + usize::from(open)
}

/// Why an exception no longer earns its entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stale {
    Under { path: String, lines: usize },
    Gone { path: String },
}

/// What a check found.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Report {
    pub ceiling: usize,
    /// Files over the ceiling, as (lines, path), sorted by path.
    pub over: Vec<(usize, String)>,
    pub stale: Vec<Stale>,
}

impl Report {
    pub fn failed(&self) -> bool {
        !self.over.is_empty() || !self.stale.is_empty()
    }
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if !self.over.is_empty() {
            writeln!(
                f,
                "line-ceiling: {} file(s) over {} lines; split along responsibilities the code already has, or list each in {CONFIG} with its technical reason:",
                self.over.len(),
                self.ceiling
            )?;
            for (lines, path) in &self.over {
                writeln!(f, "{lines:>7} {path}")?;
            }
        }
        for stale in &self.stale {
            match stale {
                Stale::Under { path, lines } => {
                    writeln!(f, "line-ceiling: stale exception {path}: now {lines} lines; remove its entry")?
                }
                Stale::Gone { path } => writeln!(f, "line-ceiling: stale exception {path}: gone; remove its entry")?,
            }
        }
        Ok(())
    }
}

/// Checks `files` (relative to `root`) against `config`, and every
/// exception for staleness whether or not its file was named.
///
/// # Errors
/// A named file that exists but cannot be read.
pub fn check(root: &Path, files: &[String], config: &Config) -> Result<Report, String> {
    let mut report = Report { ceiling: config.ceiling, ..Report::default() };
    let mut paths: Vec<String> = files.iter().map(|f| normalize(f)).collect();
    paths.sort();
    paths.dedup();
    for path in paths {
        if !config.counted(&path) || config.excluded(&path) || config.excepted(&path) {
            continue;
        }
        let Some(lines) = lines_of(root, &path)? else { continue };
        if lines > config.ceiling {
            report.over.push((lines, path));
        }
    }
    for e in &config.exception {
        match lines_of(root, &e.path)? {
            None => report.stale.push(Stale::Gone { path: e.path.clone() }),
            Some(lines) if lines <= config.ceiling => {
                report.stale.push(Stale::Under { path: e.path.clone(), lines })
            }
            Some(_) => {}
        }
    }
    Ok(report)
}

fn lines_of(root: &Path, path: &str) -> Result<Option<usize>, String> {
    match std::fs::read(root.join(path)) {
        Ok(bytes) => Ok(Some(count_lines(&bytes))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("{path}: {e}")),
    }
}

#[cfg(test)]
mod tests;
