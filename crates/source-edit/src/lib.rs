// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Pure source-edit proposals. Hosts retain the only editor buffer and decide
//! whether to apply an application, record undo, validate a format, or save.
//!
//! A host assigns a fresh instance on open and advances its source revision on
//! every text change, including undo/redo. Reusing a stamp for changed source
//! violates the host contract; exact source matching supplies a second guard.
//! Stamps and receipts describe local observations, not durable identities,
//! authority, hashes, or proof that the host applied or persisted anything.

use serde::{Deserialize, Serialize};
use std::{fmt, sync::Arc};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceStamp {
    pub document: String,
    pub instance: u64,
    pub revision: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TextEdit {
    /// UTF-8 byte offsets in the base source, never in intermediate text.
    pub start: usize,
    pub end: usize,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ChangedRange {
    pub base_start: usize,
    pub base_end: usize,
    pub result_start: usize,
    pub result_end: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct EditReceipt {
    pub base: SourceStamp,
    pub result: SourceStamp,
    pub ranges: Vec<ChangedRange>,
}

/// A validated candidate, not evidence of mutation. Publish `receipt.result`
/// only after the host has accepted `text` through its existing edit boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Application {
    pub text: String,
    pub receipt: EditReceipt,
}

/// One immutable captured source observation. Clones and proposals share the
/// retained base bytes; this is not a mutable editor or a second save authority.
#[derive(Clone, Debug)]
pub struct SourceSnapshot {
    stamp: SourceStamp,
    source: Arc<str>,
}

impl SourceSnapshot {
    pub fn capture(stamp: SourceStamp, source: &str) -> Self {
        Self {
            stamp,
            source: Arc::from(source),
        }
    }
    pub fn stamp(&self) -> &SourceStamp {
        &self.stamp
    }
    pub fn source(&self) -> &str {
        &self.source
    }
    /// Recheck a captured observation without making or applying an edit.
    pub fn check(&self, current: &SourceStamp, source: &str) -> Result<(), EditError> {
        if current.document != self.stamp.document || current.instance != self.stamp.instance {
            return Err(EditError::WrongDocument);
        }
        if current.revision != self.stamp.revision {
            return Err(EditError::StaleRevision);
        }
        if source != self.source.as_ref() {
            return Err(EditError::SourceChanged);
        }
        Ok(())
    }
    pub fn propose(&self, edits: Vec<TextEdit>) -> Result<Proposal, EditError> {
        Proposal::from_snapshot(self.clone(), edits)
    }
}

/// An immutable proposal retaining its exact base source. It is deliberately
/// not deserializable: untrusted edits must pass `prepare`, not restore a preview
/// or receipt supplied by a caller. It does not own a mutable document buffer.
#[derive(Clone, Debug)]
pub struct Proposal {
    base: SourceSnapshot,
    edits: Vec<TextEdit>,
    preview: String,
    ranges: Vec<ChangedRange>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditError {
    EmptyBatch,
    InvalidBoundary,
    Overlap,
    WrongDocument,
    StaleRevision,
    SourceChanged,
    RevisionExhausted,
}

impl fmt::Display for EditError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::EmptyBatch => "empty edit batch",
            Self::InvalidBoundary => "edit range must use UTF-8 byte boundaries",
            Self::Overlap => "overlapping or same-start edits",
            Self::WrongDocument => "wrong document instance",
            Self::StaleRevision => "stale base revision",
            Self::SourceChanged => "source changed without a matching revision",
            Self::RevisionExhausted => "source revision exhausted",
        })
    }
}
impl std::error::Error for EditError {}

impl Proposal {
    /// Validate, sort and preview a batch without changing the source or stamp.
    /// Ambiguous same-position insertions are refused, as are overlapping edits.
    pub fn prepare(
        base: SourceStamp,
        source: &str,
        edits: Vec<TextEdit>,
    ) -> Result<Self, EditError> {
        SourceSnapshot::capture(base, source).propose(edits)
    }

    fn from_snapshot(base: SourceSnapshot, mut edits: Vec<TextEdit>) -> Result<Self, EditError> {
        let source = base.source();
        if edits.is_empty() {
            return Err(EditError::EmptyBatch);
        }
        base.stamp
            .revision
            .checked_add(1)
            .ok_or(EditError::RevisionExhausted)?;
        edits.sort_by_key(|e| (e.start, e.end));
        for (i, edit) in edits.iter().enumerate() {
            if edit.start > edit.end
                || edit.end > source.len()
                || !source.is_char_boundary(edit.start)
                || !source.is_char_boundary(edit.end)
            {
                return Err(EditError::InvalidBoundary);
            }
            if i > 0 && (edit.start < edits[i - 1].end || edit.start == edits[i - 1].start) {
                return Err(EditError::Overlap);
            }
        }
        let mut preview = String::new();
        let mut cursor = 0;
        let mut ranges = Vec::with_capacity(edits.len());
        for edit in &edits {
            preview.push_str(&source[cursor..edit.start]);
            let result_start = preview.len();
            preview.push_str(&edit.text);
            ranges.push(ChangedRange {
                base_start: edit.start,
                base_end: edit.end,
                result_start,
                result_end: preview.len(),
            });
            cursor = edit.end;
        }
        preview.push_str(&source[cursor..]);
        Ok(Self {
            base,
            edits,
            preview,
            ranges,
        })
    }

    pub fn base(&self) -> &SourceStamp {
        self.base.stamp()
    }
    /// Sorted base-coordinate edits, not a sequential mutation program. A host
    /// replaying them must use descending offsets or simultaneous semantics.
    pub fn edits(&self) -> &[TextEdit] {
        &self.edits
    }
    pub fn preview(&self) -> &str {
        &self.preview
    }

    /// Check a proposal against the host's current observation. This still does
    /// not mutate anything; the host must serialize this check with its edit.
    /// Apply the entire candidate as one undoable transaction or refuse without
    /// changing source, source stamp or undo state. Hosts lacking batch undo
    /// must refuse multi-range proposals rather than partially replaying them.
    pub fn application(
        &self,
        current: &SourceStamp,
        source: &str,
    ) -> Result<Application, EditError> {
        self.base.check(current, source)?;
        let mut result = self.base.stamp().clone();
        result.revision = result
            .revision
            .checked_add(1)
            .ok_or(EditError::RevisionExhausted)?;
        Ok(Application {
            text: self.preview.clone(),
            receipt: EditReceipt {
                base: self.base.stamp().clone(),
                result,
                ranges: self.ranges.clone(),
            },
        })
    }
}

#[cfg(test)]
mod tests;
