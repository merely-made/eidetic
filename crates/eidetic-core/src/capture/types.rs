// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Host-neutral capture records.
//!
//! Turnstone's `FetchedDocument`, `SourceIdentity`, `CaptureRequest`,
//! `StoredSourceDocument` and `DepositReceipt` are the same shapes with
//! host id newtypes; a host `From`-converts at its edge. The serde shapes
//! here are the stored shapes: field names, order and defaults must not move,
//! because sealed receipts and legacy envelopes already on disk carry them.
//! The id newtypes (`SessionId`, `GraphId`) serialize as bare UUID strings, so
//! plain [`uuid::Uuid`] reads and writes identical bytes.

use serde::{Deserialize, Serialize};

/// A fetched HTML document in neutral terms: the exact response bytes, their
/// lossy-decoded text, and what the transport observed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FetchedDocument {
    /// Exact response bytes, before text decoding.
    pub bytes: Vec<u8>,
    /// The decoded body text the annotator reads.
    pub body: String,
    /// The final response URL after redirects.
    pub effective_url: Option<String>,
    /// The response `Content-Type`, verbatim.
    pub content_type: Option<String>,
    /// When the response was observed, unix milliseconds. Zero means unobserved.
    pub acquired_at_ms: u64,
}

/// Identity of the acquired response, independent of the current URL or cursor.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceIdentity {
    /// BLAKE3 of the exact bytes.
    pub hash: String,
    /// BLAKE3 of the decoded body.
    pub body_hash: String,
    /// The final response URL.
    pub effective_url: Option<String>,
    /// The response content type.
    pub content_type: Option<String>,
    /// When the response was observed.
    pub acquired_at_ms: u64,
}

impl SourceIdentity {
    /// The identity a document presents.
    #[must_use]
    pub fn of(document: &FetchedDocument) -> Self {
        Self {
            hash: crate::Hash::of(&document.bytes).to_string(),
            body_hash: crate::Hash::of(document.body.as_bytes()).to_string(),
            effective_url: document.effective_url.clone(),
            content_type: document.content_type.clone(),
            acquired_at_ms: document.acquired_at_ms,
        }
    }
}

/// A request to capture one source for one node.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaptureRequest {
    /// The request's own id; it names the sealed receipt.
    pub id: uuid::Uuid,
    /// The owning session.
    pub session: uuid::Uuid,
    /// The owning graph.
    pub graph: uuid::Uuid,
    /// The node the capture belongs to.
    pub node: uuid::Uuid,
    /// The address the person asked to capture.
    pub url: String,
    /// The source the person was shown.
    pub source: Option<SourceIdentity>,
    /// Receipts that predate completion-bound Keep read as `false`.
    #[serde(default)]
    pub keep: bool,
}

/// Manifest identities written by one explicit source-document capture.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredSourceDocument {
    /// The raw response manifest.
    pub raw_manifest: String,
    /// The annotation manifest (`sealed-v1:` prefixed when sealed).
    pub annotation_manifest: String,
}

/// A durable request receipt: recovery inventory, never corpus admission.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DepositReceipt {
    /// Whether the host acknowledged the save.
    #[serde(default)]
    pub saved: bool,
    /// The request the receipt answers.
    pub request: CaptureRequest,
    /// What was stored, once the deposit completed.
    pub stored: Option<StoredSourceDocument>,
}
