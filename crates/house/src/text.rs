// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Text as the ported Python scripts read and wrote it.

use std::path::Path;

/// Python's universal newlines: `\r\n` and `\r` read as `\n`.
pub fn universal(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

/// `read_text(encoding="utf-8-sig")`: strict UTF-8, BOM dropped, universal newlines.
///
/// # Errors
/// The file cannot be read or is not UTF-8.
pub fn read_sig(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let text = String::from_utf8(bytes).map_err(|e| e.to_string())?;
    Ok(universal(text.strip_prefix('\u{feff}').unwrap_or(&text)))
}

/// `read_text(encoding="utf-8")`: strict UTF-8, universal newlines, BOM kept.
///
/// # Errors
/// The file cannot be read or is not UTF-8.
pub fn read_utf8(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let text = String::from_utf8(bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(universal(&text))
}

/// `str.splitlines()`: Python's line boundaries, no trailing empty line.
pub fn splitlines(text: &str) -> Vec<&str> {
    let mut lines = Vec::new();
    let mut start = 0;
    let mut chars = text.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        let boundary = matches!(
            c,
            '\n' | '\r' | '\u{b}' | '\u{c}' | '\u{1c}' | '\u{1d}' | '\u{1e}' | '\u{85}' | '\u{2028}' | '\u{2029}'
        );
        if !boundary {
            continue;
        }
        lines.push(&text[start..i]);
        start = i + c.len_utf8();
        if c == '\r' && chars.peek().is_some_and(|(_, n)| *n == '\n') {
            chars.next();
            start += 1;
        }
    }
    if start < text.len() {
        lines.push(&text[start..]);
    }
    lines
}

/// A path as Python's `str(WindowsPath)` prints it: backslashes on Windows.
pub fn py_path(path: &Path) -> String {
    let shown = path.display().to_string();
    if cfg!(windows) { shown.replace('/', "\\") } else { shown }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splitlines_matches_python() {
        assert_eq!(splitlines("a\nb\r\nc\rd\u{c}e\n"), ["a", "b", "c", "d", "e"]);
        assert_eq!(splitlines(""), Vec::<&str>::new());
        assert_eq!(splitlines("\n\nx"), ["", "", "x"]);
        assert_eq!(universal("a\r\nb\rc"), "a\nb\nc");
    }
}
