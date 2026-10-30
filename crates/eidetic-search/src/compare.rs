// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. SPDX-License-Identifier: MPL-2.0

//! Bounded, deterministic comparison of two already-authorized exact texts.
//! The caller owns admission, availability, and observations of current heads.
use eidetic::Hash;
use eidetic::models::recall::{PassageSelector, RecallSourceId, RetainedSourceText};

/// Identifies the algorithm and its tie-breaking behavior for replay.
pub const COMPARISON_OPERATION: &str = "eidetic.line-comparison/v1";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComparisonRelationship {
    /// The user explicitly chose two sources or versions.
    Explicit,
    /// The source owner declared a sequence of versions of the same source.
    OwnerDeclaredVersions,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComparisonSettings {
    /// Combined UTF-8 input bytes.
    pub max_input_bytes: usize,
    /// Combined bytes of source and version identifiers.
    pub max_identity_bytes: usize,
    /// Budget for table cells and bytes inspected by line comparisons.
    /// Line comparisons of equal byte length are charged their full length,
    /// including traceback, even if the first differing byte ends the scan.
    pub max_diff_work: usize,
    /// Maximum number of emitted spans.
    pub max_output_spans: usize,
}
impl Default for ComparisonSettings {
    fn default() -> Self {
        Self {
            max_input_bytes: 4 * 1024 * 1024,
            max_identity_bytes: 64 * 1024,
            max_diff_work: 1_000_000,
            max_output_spans: 100_000,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComparisonSource {
    pub source: RecallSourceId,
    pub version: String,
    pub text_hash: Hash,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpanKind {
    Equal,
    Delete,
    Insert,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComparisonSpan {
    pub kind: SpanKind,
    pub old: Option<PassageSelector>,
    pub new: Option<PassageSelector>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Comparison {
    pub operation: &'static str,
    pub relationship: ComparisonRelationship,
    pub settings: ComparisonSettings,
    pub old: ComparisonSource,
    pub new: ComparisonSource,
    pub spans: Vec<ComparisonSpan>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComparisonError {
    DifferentSources,
    InputLimit,
    WorkLimit,
    OutputLimit,
}

struct Line<'a> {
    text: &'a str,
    start: usize,
    end: usize,
}
fn lines(text: &str) -> Vec<Line<'_>> {
    let mut pos = 0;
    text.split_inclusive('\n')
        .map(|line| {
            let start = pos;
            pos += line.chars().count();
            Line {
                text: line,
                start,
                end: pos,
            }
        })
        .collect()
}
fn equal_lines(a: &Line<'_>, b: &Line<'_>, remaining: &mut usize) -> Result<bool, ComparisonError> {
    if a.text.len() != b.text.len() {
        return Ok(false);
    }
    *remaining = remaining
        .checked_sub(a.text.len())
        .ok_or(ComparisonError::WorkLimit)?;
    Ok(a.text == b.text)
}
fn selector(line: &Line<'_>) -> PassageSelector {
    PassageSelector {
        start: line.start,
        end: line.end,
    }
}
fn identity(source: &RetainedSourceText, hash: Hash) -> ComparisonSource {
    ComparisonSource {
        source: source.source.clone(),
        version: source.version.clone(),
        text_hash: hash,
    }
}

/// Compare retained canonical text without fetching or inferring a version chain.
/// Line endings belong to their lines; equal-length alternatives prefer deletion.
pub fn compare(
    old: &RetainedSourceText,
    new: &RetainedSourceText,
    relationship: ComparisonRelationship,
    settings: &ComparisonSettings,
) -> Result<Comparison, ComparisonError> {
    let identity_bytes = [old, new]
        .iter()
        .flat_map(|s| {
            [
                s.source.source.len(),
                s.source.owner.as_ref().map_or(0, String::len),
                s.source.node.as_ref().map_or(0, String::len),
                s.version.len(),
            ]
        })
        .try_fold(0usize, usize::checked_add);
    if identity_bytes
        .filter(|n| *n <= settings.max_identity_bytes)
        .is_none()
    {
        return Err(ComparisonError::InputLimit);
    }
    if relationship == ComparisonRelationship::OwnerDeclaredVersions && old.source != new.source {
        return Err(ComparisonError::DifferentSources);
    }
    if old
        .text
        .len()
        .checked_add(new.text.len())
        .filter(|n| *n <= settings.max_input_bytes)
        .is_none()
    {
        return Err(ComparisonError::InputLimit);
    }
    let old_hash = Hash::of(old.text.as_bytes());
    let new_hash = Hash::of(new.text.as_bytes());
    let mut result = Comparison {
        operation: COMPARISON_OPERATION,
        relationship,
        settings: settings.clone(),
        old: identity(old, old_hash),
        new: identity(new, new_hash),
        spans: Vec::new(),
    };
    if old_hash == new_hash {
        if !old.text.is_empty() {
            if settings.max_output_spans == 0 {
                return Err(ComparisonError::OutputLimit);
            }
            let end = old.text.chars().count();
            result.spans.push(ComparisonSpan {
                kind: SpanKind::Equal,
                old: Some(PassageSelector { start: 0, end }),
                new: Some(PassageSelector { start: 0, end }),
            });
        }
        return Ok(result);
    }
    // Count before allocating line vectors or the table. The work limit also
    // bounds vector sizes even when one side is empty.
    let n = old.text.split_inclusive('\n').count();
    let m = new.text.split_inclusive('\n').count();
    let cells = n
        .checked_add(1)
        .and_then(|n| m.checked_add(1).and_then(|m| n.checked_mul(m)))
        .filter(|c| *c <= settings.max_diff_work)
        .ok_or(ComparisonError::WorkLimit)?;
    let a = lines(&old.text);
    let b = lines(&new.text);
    let width = m + 1;
    let mut remaining_work = settings.max_diff_work - cells;
    let mut dp = vec![0usize; cells];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            dp[i * width + j] = if equal_lines(&a[i], &b[j], &mut remaining_work)? {
                1 + dp[(i + 1) * width + j + 1]
            } else {
                dp[(i + 1) * width + j].max(dp[i * width + j + 1])
            };
        }
    }
    let (mut i, mut j) = (0, 0);
    while i < n || j < m {
        if result.spans.len() >= settings.max_output_spans {
            return Err(ComparisonError::OutputLimit);
        }
        let span = if i < n && j < m && equal_lines(&a[i], &b[j], &mut remaining_work)? {
            let s = ComparisonSpan {
                kind: SpanKind::Equal,
                old: Some(selector(&a[i])),
                new: Some(selector(&b[j])),
            };
            i += 1;
            j += 1;
            s
        } else if i < n && (j == m || dp[(i + 1) * width + j] >= dp[i * width + j + 1]) {
            let s = ComparisonSpan {
                kind: SpanKind::Delete,
                old: Some(selector(&a[i])),
                new: None,
            };
            i += 1;
            s
        } else {
            let s = ComparisonSpan {
                kind: SpanKind::Insert,
                old: None,
                new: Some(selector(&b[j])),
            };
            j += 1;
            s
        };
        result.spans.push(span);
    }
    Ok(result)
}
