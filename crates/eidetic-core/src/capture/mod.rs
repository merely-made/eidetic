// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Native capture, sealing and verification of retained source documents,
//! promoted from Turnstone's `trail_memory` (memory-gates plan S6, D17).
//!
//! The core owns write order, custody, identity checks and recovery over a
//! host-supplied [`Store`](crate::Store) and [`PayloadSealer`](crate::PayloadSealer).
//! The host supplies the annotation record ([`SourceAnnotator`]; Fleece in
//! Mere, which sits above Eidetic), a [`CaptureClock`], and the neutral
//! [`FetchedDocument`], [`CaptureRequest`] and [`StoredSourceDocument`] it
//! `From`-converts to and from its own types.
//!
//! Stored formats are frozen: the golden fixtures under
//! `tests/fixtures/capture_golden/` are bytes Turnstone wrote, and
//! `tests/capture_golden.rs` reads them through this module.
//!
//! Page text and resource captures are not duplicated here. They live in
//! [`crate::browsing`] ([`PageTextStore`](crate::PageTextStore),
//! [`ResourceCaptureStore`](crate::ResourceCaptureStore)), which these
//! functions call.

//! ## Not yet promoted
//!
//! Turnstone keeps `saved_sources`, the `*_receipts` codecs and the
//! `emit_*_recovery` publishers until a later slice. They will need, from
//! here: a `BoundedRead::list_inventory(prefix, max_items, max_key_bytes)`
//! beside `get_limited`; a sealed `content()` reader returning the canonical
//! text from an annotation (an `AcquisitionEvidence` plus canonical text and
//! hash on [`SourceAnnotator`]); receipt envelopes and the legacy
//! `CaptureAttachment` as plain serde types over the neutral
//! [`CaptureRequest`] and [`StoredSourceDocument`]; and host-owned admission,
//! budget and `SourceReadError` types converted at the edge through
//! [`CaptureError`].

mod annotator;
mod error;
mod records;
pub mod sealed;
mod seams;
pub mod source_document;
mod types;

pub use annotator::{AcquisitionEvidence, SourceAnnotator, is_html};
pub use error::CaptureError;
pub use records::{
    deposit_definition, deposit_record, load_bounded, load_definition, load_record,
    load_record_bounded, validate_schema,
};
#[cfg(not(target_arch = "wasm32"))]
pub use seams::SystemClock;
pub use seams::{BoundedRead, CaptureClock, FixedClock};
pub use source_document::{
    capture_source_document_in, preflight_source_artifact, verify_source_document,
    verify_source_document_typed,
};
pub use types::{
    CaptureRequest, DepositReceipt, FetchedDocument, SourceIdentity, StoredSourceDocument,
};
