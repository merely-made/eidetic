// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Exact retained source versions and passage selectors used by inference.
use crate::{Hash, ManifestId, SchemaRef, TypedPayload};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecallSourceKind {
    PageText,
    Capture,
    Clip,
    Fleece,
}

/// Identity belongs to the source owner. A URL is an address, never a version.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecallSourceId {
    pub kind: RecallSourceKind,
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RetainedSourceText {
    pub source: RecallSourceId,
    /// Page-text blob, capture content hash, immutable clip or source-owned version.
    pub version: String,
    pub title: Option<String>,
    /// Exact extracted text stream to which selectors apply.
    pub text: String,
}
impl Drop for RetainedSourceText {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        if let Some(title) = &mut self.title {
            title.zeroize();
        }
        self.text.zeroize();
        self.source.source.zeroize();
        if let Some(node) = &mut self.source.node {
            node.zeroize();
        }
        self.version.zeroize();
    }
}

/// Half-open Unicode scalar positions in the retained canonical text stream.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PassageSelector {
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InferencePassage {
    pub source: RecallSourceId,
    pub version: String,
    pub text_hash: Hash,
    pub selector: PassageSelector,
    pub chunker: String,
    pub text: String,
}
impl InferencePassage {
    pub fn id(&self) -> ManifestId {
        let encoded =
            zeroize::Zeroizing::new(serde_json::to_vec(self).expect("passage serialization"));
        ManifestId::of_blob(&encoded)
    }
}
impl Drop for InferencePassage {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.text.zeroize();
        self.source.source.zeroize();
        if let Some(node) = &mut self.source.node {
            node.zeroize();
        }
        self.version.zeroize();
    }
}
impl TypedPayload for InferencePassage {
    fn schema_ref() -> SchemaRef {
        SchemaRef::from_id(ManifestId::of_blob(br#"{"format":"mere-native","schema_id":"eidetic.InferencePassage/v1","body":{"version":1,"description":"Exact source version, canonical text hash, Unicode selector, chunker and supplied passage bytes."}}"#))
    }
}
