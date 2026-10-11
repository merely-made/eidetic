// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Source-preserving intake envelopes for interchange adapters.
//!
//! An adapter translates source material into a typed value and reports what
//! its projection omits. Intake does not write a store or mutate a graph. The
//! receiving session decides what to retain and which changes to authorize.

use serde::{Deserialize, Serialize};

use super::{
    BookmarkFileFormat, BookmarkImportError, BrowserImportBatch, BrowserImportPayload,
    BrowserImportRun, detect_bookmark_file_format, parser,
};
use crate::Hash;

/// Exact source material retained alongside an adapter's typed projection.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntakeSource {
    /// Name of the translating adapter, independent of the source's transport.
    pub adapter: String,
    /// A file address, engine address, or other caller-supplied source locator.
    /// This is evidence, not a claim that the locator is globally canonical.
    pub locator: Option<String>,
    pub media_type: String,
    pub bytes: Vec<u8>,
}

impl IntakeSource {
    /// The digest covers the original bytes, including unrepresented fields.
    pub fn content_hash(&self) -> Hash {
        Hash::of(&self.bytes)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum IntakeNoticeKind {
    /// A source record could not be projected into the adapter's output model.
    SkippedRecord,
    /// A field remains in source evidence but is absent from the typed value.
    UnrepresentedField,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntakeNotice {
    pub kind: IntakeNoticeKind,
    /// A source record id or source-local field name when the adapter has one.
    pub source_locator: Option<String>,
    pub message: String,
}

/// A portable conversion result for documents, records, or collections.
///
/// `value` is deliberately generic: importing engine-generated content or an
/// archive need not pretend to be a bookmark or a browsing encounter. Keeping
/// the original source also makes adapter losses inspectable and retryable.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Intake<T> {
    pub source: IntakeSource,
    pub value: T,
    pub notices: Vec<IntakeNotice>,
}

/// Translate a bookmark file while retaining source bytes and reporting losses.
/// The older batch-only parser remains available with its unchanged data shape.
pub fn parse_bookmark_file_to_intake(
    contents: &str,
    run: BrowserImportRun,
    source_locator: Option<String>,
) -> Result<Intake<BrowserImportBatch>, BookmarkImportError> {
    let format =
        detect_bookmark_file_format(contents).ok_or(BookmarkImportError::UnsupportedFormat)?;
    let (items, notices, adapter, media_type) = match format {
        BookmarkFileFormat::ChromeJson => {
            let (items, notices) = parser::parse_chrome_bookmark_json_with_notices(contents)?;
            (
                items,
                notices,
                "browser-bookmarks.chrome-json",
                "application/json",
            )
        },
        BookmarkFileFormat::NetscapeHtml => {
            let (items, notices) = parser::parse_netscape_bookmark_html_with_notices(contents);
            (
                items,
                notices,
                "browser-bookmarks.netscape-html",
                "text/html",
            )
        },
    };
    Ok(Intake {
        source: IntakeSource {
            adapter: adapter.into(),
            locator: source_locator,
            media_type: media_type.into(),
            bytes: contents.as_bytes().to_vec(),
        },
        value: BrowserImportBatch {
            run,
            items: items
                .into_iter()
                .map(BrowserImportPayload::Bookmark)
                .collect(),
        },
        notices,
    })
}
