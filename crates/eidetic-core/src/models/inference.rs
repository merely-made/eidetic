// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Retained inference evidence (P4). Execution and checkpoint mechanics stay
//! with the inference host; these records describe what a run actually used.
use crate::{Hash, ManifestId, SchemaRef, TypedPayload};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InferenceRunStatus {
    Queued,
    Running,
    Paused,
    Finished,
    Cancelled,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InferenceContinuation {
    pub device: String,
    /// Zero-based position of the first token produced on this device visit.
    pub at_token: usize,
    pub reason: String,
    /// Filled only by an actual comparison, never inferred from a device change.
    pub first_differing_token: Option<usize>,
    pub compared_token_ids_hash: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InferenceRunRecord {
    pub run: String,
    pub question_blob: ManifestId,
    pub supplied_passages: Vec<ManifestId>,
    pub session_id: Option<String>,
    pub prompt_template_hash: Option<String>,
    pub weight_format: Option<String>,
    pub request: serde_json::Value,
    pub output_blob: ManifestId,
    pub checkpoint_blob: Option<ManifestId>,
    pub status: InferenceRunStatus,
    pub continuations: Vec<InferenceContinuation>,
    pub error: Option<String>,
}
impl TypedPayload for InferenceRunRecord {
    fn schema_ref() -> SchemaRef {
        SchemaRef::from_id(ManifestId::from_hash(Hash::of(br#"{"format":"mere-native","schema_id":"eidetic.InferenceRunRecord/v1","body":{"version":1,"description":"Exact input, session, output, status and continuation evidence for an inference run."}}"#)))
    }
}
