// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Long options for the ported scripts: `--flag`, `--opt VALUE`, `--opt=VALUE`.

use std::collections::BTreeMap;

/// Parsed options: flags seen, and every value given per option, in order.
#[derive(Debug, Default)]
pub struct Options {
    flags: Vec<String>,
    values: BTreeMap<String, Vec<String>>,
}

impl Options {
    /// Parses `args` against the known `flags` and value-taking `valued`
    /// options (names without the leading `--`).
    ///
    /// # Errors
    /// An unknown option, a positional argument, or a missing value.
    pub fn parse(args: &[String], flags: &[&str], valued: &[&str]) -> Result<Options, String> {
        let mut out = Options::default();
        let mut it = args.iter();
        while let Some(arg) = it.next() {
            let Some(body) = arg.strip_prefix("--") else {
                return Err(format!("unrecognized argument: {arg}"));
            };
            let (name, inline) = match body.split_once('=') {
                Some((n, v)) => (n, Some(v.to_string())),
                None => (body, None),
            };
            if flags.contains(&name) && inline.is_none() {
                out.flags.push(name.to_string());
            } else if valued.contains(&name) {
                let value = match inline {
                    Some(v) => v,
                    None => it.next().cloned().ok_or_else(|| format!("--{name} expects a value"))?,
                };
                out.values.entry(name.to_string()).or_default().push(value);
            } else {
                return Err(format!("unrecognized argument: {arg}"));
            }
        }
        Ok(out)
    }

    /// Whether the flag was given.
    pub fn flag(&self, name: &str) -> bool {
        self.flags.iter().any(|f| f == name)
    }

    /// The last value given for the option.
    pub fn value(&self, name: &str) -> Option<&str> {
        self.values.get(name).and_then(|v| v.last()).map(String::as_str)
    }

    /// Every value given for a repeatable option.
    pub fn all(&self, name: &str) -> &[String] {
        self.values.get(name).map_or(&[], Vec::as_slice)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn flags_values_and_repeats() {
        let o = Options::parse(&strings(&["--json", "--repo", "a", "--repo=b", "--root", "x"]), &["json"], &["repo", "root"])
            .unwrap();
        assert!(o.flag("json"));
        assert_eq!(o.all("repo"), ["a", "b"]);
        assert_eq!(o.value("root"), Some("x"));
        assert!(Options::parse(&strings(&["--nope"]), &["json"], &[]).is_err());
        assert!(Options::parse(&strings(&["--root"]), &[], &["root"]).is_err());
        assert!(Options::parse(&strings(&["pos"]), &[], &[]).is_err());
    }
}
