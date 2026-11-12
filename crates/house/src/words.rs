// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Shell-style quoting for the hook file's first line, and the reverse.

fn plain(c: char) -> bool {
    c.is_ascii_alphanumeric() || "-_./=:,+@%".contains(c)
}

/// One argument as a POSIX shell would read it back.
pub fn quote(arg: &str) -> String {
    if !arg.is_empty() && arg.chars().all(plain) {
        return arg.to_string();
    }
    format!("'{}'", arg.replace('\'', r"'\''"))
}

/// Splits a line into words: whitespace separates, `'...'` is literal,
/// `"..."` honours `\"` and `\\`, and a bare `\` escapes the next character.
///
/// # Errors
/// An unclosed quote or a trailing backslash.
pub fn split(line: &str) -> Result<Vec<String>, String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut started = false;
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        match c {
            c if c.is_whitespace() => {
                if started {
                    words.push(std::mem::take(&mut word));
                    started = false;
                }
            }
            '\'' => {
                started = true;
                loop {
                    match chars.next().ok_or("unclosed single quote")? {
                        '\'' => break,
                        c => word.push(c),
                    }
                }
            }
            '"' => {
                started = true;
                loop {
                    match chars.next().ok_or("unclosed double quote")? {
                        '"' => break,
                        '\\' => match chars.next().ok_or("unclosed double quote")? {
                            c @ ('"' | '\\' | '$' | '`') => word.push(c),
                            c => {
                                word.push('\\');
                                word.push(c);
                            }
                        },
                        c => word.push(c),
                    }
                }
            }
            '\\' => {
                started = true;
                word.push(chars.next().ok_or("trailing backslash")?);
            }
            c => {
                started = true;
                word.push(c);
            }
        }
    }
    if started {
        words.push(word);
    }
    Ok(words)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quote_and_split_round_trip() {
        let args = ["--clippy", "--all-targets -p mere-rdf --features mere-rdf/query", "it's", "", "a\"b\\c"];
        let line: Vec<String> = args.iter().map(|a| quote(a)).collect();
        assert_eq!(split(&line.join(" ")).unwrap(), args);
    }

    #[test]
    fn plain_words_stay_bare() {
        assert_eq!(quote("--deny"), "--deny");
        assert_eq!(quote("a b"), "'a b'");
        assert_eq!(split(r#"a "b c" d\ e"#).unwrap(), ["a", "b c", "d e"]);
        assert!(split("'open").is_err());
    }
}
