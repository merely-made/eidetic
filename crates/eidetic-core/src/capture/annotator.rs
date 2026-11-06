// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The annotation seam.
//!
//! A captured source is stored as exact raw bytes plus an annotation record
//! describing a canonical-text projection of them. In Mere that record is
//! Fleece's `FleeceAnnotationRecord`, and Fleece lives above Eidetic, so the
//! core cannot name it. The host implements [`SourceAnnotator`]; the core
//! decides roles, ordering, custody and verification, and asks the annotator
//! only to build a record, read its acquisition facts, and save or load it.

use super::{CaptureError, FetchedDocument};
use crate::{Hash, ManifestId, Store, Timestamp, TypedPayload};

/// The acquisition facts an annotation attests, in neutral terms.
///
/// These mirror Fleece's `CaptureEvidenceV1` fields the verifiers read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AcquisitionEvidence {
    /// The final response URL the annotation recorded.
    pub final_source: String,
    /// The capture hash, which must be the raw manifest's hash.
    pub capture_hash: Hash,
    /// When the response was observed.
    pub captured_at: Timestamp,
    /// The response content type.
    pub response_content_type: Option<String>,
    /// The raw manifest the annotation points at.
    pub raw_manifest: Option<ManifestId>,
    /// Source DOM mode (as opposed to a rendered or replayed DOM).
    pub source_dom: bool,
    /// Whether a replay manifest is attached.
    pub has_replay_manifest: bool,
    /// Whether the evidence carries authoritative acquisition context.
    pub authoritative: bool,
}

impl AcquisitionEvidence {
    /// The media type is `text/html`, parameters ignored.
    #[must_use]
    pub fn is_html(&self) -> bool {
        is_html(self.response_content_type.as_deref())
    }
}

/// Whether a content type names `text/html`, parameters ignored.
#[must_use]
pub fn is_html(content_type: Option<&str>) -> bool {
    content_type.is_some_and(|value| {
        value
            .split(';')
            .next()
            .is_some_and(|media| media.trim().eq_ignore_ascii_case("text/html"))
    })
}

/// The host's annotation record type and how to build, read and store it.
///
/// Implementations are expected to be pure over their inputs: building the
/// same document twice yields byte-identical records, because the record's
/// manifest id is its content hash and a retry must land on the same address.
#[async_trait::async_trait(?Send)]
pub trait SourceAnnotator {
    /// The stored annotation record (its schema is [`TypedPayload::schema_ref`]).
    type Record: TypedPayload + Send + Sync;

    /// The manifest id of the annotation schema, which no other role may share.
    fn schema_manifest(&self) -> ManifestId;

    /// Build the record for a document that already passed the core's checks
    /// (observed time, HTML content type, final URL).
    ///
    /// # Errors
    /// The annotator's refusal, e.g. no canonical text.
    fn annotate(&self, document: &FetchedDocument) -> Result<Self::Record, CaptureError>;

    /// Check the record's own integrity.
    ///
    /// # Errors
    /// When the record is internally inconsistent.
    fn validate(&self, record: &Self::Record) -> Result<(), CaptureError>;

    /// The acquisition facts the record attests, if it has any.
    fn evidence(&self, record: &Self::Record) -> Option<AcquisitionEvidence>;

    /// The canonical source the record describes, with the capture hash that
    /// associates it to the resource.
    fn association(&self, record: &Self::Record) -> (String, Hash);

    /// Seed the annotation schema into an unsealed store.
    ///
    /// # Errors
    /// The store's refusal.
    async fn bootstrap_schema(&self, store: &mut dyn Store) -> Result<(), CaptureError>;

    /// Save the record into an unsealed store with its resource association.
    ///
    /// # Errors
    /// The store's refusal.
    async fn save(
        &self,
        store: &mut dyn Store,
        record: &Self::Record,
        at_ms: u64,
    ) -> Result<(), CaptureError>;

    /// Load and validate a record from an unsealed store.
    ///
    /// # Errors
    /// The store's refusal or a validation failure.
    async fn load(
        &self,
        store: &mut dyn Store,
        id: ManifestId,
    ) -> Result<Option<Self::Record>, CaptureError>;
}
