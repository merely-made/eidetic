// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! The generated block inside a manifest's `[patch.crates-io]`: finding it,
//! rendering it and splicing it in, leaving every other byte alone.

use std::ops::Range;

use super::upstream::Row;
use crate::cycles::tree::{self, Toml};

/// The block's first line, before its options.
pub const HEADER: &str = "# house patches";
/// The block's last line.
pub const END: &str = "# end house patches";
const NOTE: &str =
    "# Written by `house patches` (eidetic, crates/house) from the upstream's [patch.crates-io]; `house patches --check` fails when they drift.";

/// The block's lines for `options` (a command line with a leading space),
/// joined with `eol` and ending with one.
pub fn render(options: &str, rows: &[Row], eol: &str) -> String {
    let mut lines = vec![format!("{HEADER}{options}"), NOTE.to_string()];
    lines.extend(rows.iter().map(Row::line));
    lines.push(END.to_string());
    lines.iter().map(|l| format!("{l}{eol}")).collect()
}

/// The line ending a file already uses: `\r\n` if any line has one.
pub fn eol_of(text: &str) -> &'static str {
    if text.contains("\r\n") { "\r\n" } else { "\n" }
}

/// Each line's byte range, its ending included.
fn lines(text: &str) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let mut start = 0;
    for piece in text.split_inclusive('\n') {
        out.push(start..start + piece.len());
        start += piece.len();
    }
    out
}

fn body<'a>(text: &'a str, r: &Range<usize>) -> &'a str {
    text[r.clone()].trim_end_matches(['\n', '\r'])
}

fn is_header(line: &str) -> bool {
    line.strip_prefix(HEADER).is_some_and(|r| r.is_empty() || r.starts_with(char::is_whitespace))
}

fn is_patch_table(line: &str) -> bool {
    let t = line.trim();
    let t = t.split_once('#').map_or(t, |(head, _)| head.trim_end());
    t == "[patch.crates-io]" || t == "[patch.\"crates-io\"]"
}

/// A block found in a manifest: the options on its first line, its byte
/// range, and its text.
#[derive(Debug, PartialEq, Eq)]
pub struct Found {
    pub options: String,
    pub range: Range<usize>,
}

/// The manifest's block, if it has one.
///
/// # Errors
/// Two blocks, a block with no end line, or one outside `[patch.crates-io]`.
pub fn find(text: &str) -> Result<Option<Found>, String> {
    let ls = lines(text);
    let mut found = None;
    let mut table_ok = false;
    let mut i = 0;
    while i < ls.len() {
        let l = body(text, &ls[i]);
        if l.trim_start().starts_with('[') {
            table_ok = is_patch_table(l);
        }
        if is_header(l) {
            if found.is_some() {
                return Err("two `# house patches` blocks".into());
            }
            if !table_ok {
                return Err("the `# house patches` block is not inside [patch.crates-io]".into());
            }
            let end = (i + 1..ls.len()).find(|&j| body(text, &ls[j]) == END).ok_or("the `# house patches` block has no end line")?;
            found = Some(Found { options: l[HEADER.len()..].to_string(), range: ls[i].start..ls[end].end });
            i = end;
        }
        i += 1;
    }
    Ok(found)
}

/// `text` with the block (rendered by `block`, which takes the line ending)
/// in place of the old one, or at the end of `[patch.crates-io]`, or in a
/// new `[patch.crates-io]` at the end of the file.
///
/// # Errors
/// What [`find`] refuses, or a `patch.crates-io` table with no
/// `[patch.crates-io]` header line to put the block under.
pub fn splice(text: &str, block: impl Fn(&str) -> String) -> Result<String, String> {
    let eol = eol_of(text);
    let new = block(eol);
    if let Some(old) = find(text)? {
        return Ok(format!("{}{new}{}", &text[..old.range.start], &text[old.range.end..]));
    }
    let ls = lines(text);
    if let Some(h) = ls.iter().position(|r| is_patch_table(body(text, r))) {
        let next = (h + 1..ls.len()).find(|&j| body(text, &ls[j]).trim_start().starts_with('[')).unwrap_or(ls.len());
        let last = (h..next).rev().find(|&j| {
            let l = body(text, &ls[j]).trim();
            j == h || !(l.is_empty() || l.starts_with('#'))
        });
        let at = ls[last.unwrap_or(h)].end;
        let sep = if text[..at].ends_with('\n') { "" } else { eol };
        return Ok(format!("{}{sep}{new}{}", &text[..at], &text[at..]));
    }
    let data = tree::parse(text)?;
    if data.get("patch").is_some_and(|p| p.has("crates-io")) {
        return Err("patch.crates-io is not written as a [patch.crates-io] table; give it one".into());
    }
    let sep = if text.is_empty() || text.ends_with('\n') { "" } else { eol };
    let gap = if text.is_empty() { "" } else { eol };
    Ok(format!("{text}{sep}{gap}[patch.crates-io]{eol}{new}"))
}

/// The crates the manifest patches outside the block that are also among
/// `names`: hand-written rows the block would duplicate.
///
/// # Errors
/// Text that is not TOML, with or without its block.
pub fn hand_written(text: &str, names: &[&str]) -> Result<Vec<String>, String> {
    let outside = match find(text)? {
        Some(f) => format!("{}{}", &text[..f.range.start], &text[f.range.end..]),
        None => text.to_string(),
    };
    let data = tree::parse(&outside)?;
    let table = data.get("patch").and_then(|p| p.get("crates-io")).and_then(Toml::as_table).unwrap_or_default();
    Ok(table.iter().map(|(k, _)| k.clone()).filter(|k| names.contains(&k.as_str())).collect())
}

/// The rows a block's text holds, parsed.
///
/// # Errors
/// Text that is not TOML.
pub fn rows_of(block: &str) -> Result<Vec<(String, Toml)>, String> {
    match tree::parse(block)? {
        Toml::Table(t) => Ok(t),
        _ => Ok(Vec::new()),
    }
}
