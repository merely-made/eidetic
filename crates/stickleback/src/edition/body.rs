// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The edition body and its canonical encoding.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::EditionError;

/// Operation hash of an edition.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EditionId(pub [u8; 32]);

impl Serialize for EditionId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&hex::encode(self.0))
    }
}

impl<'de> Deserialize<'de> for EditionId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        let bytes = hex::decode(&text).map_err(serde::de::Error::custom)?;
        let array = <[u8; 32]>::try_from(bytes.as_slice())
            .map_err(|_| serde::de::Error::custom("edition id must be 32 bytes"))?;
        // Reject uppercase so one id has one encoding.
        if hex::encode(array) != text {
            return Err(serde::de::Error::custom("edition id must be lowercase hex"));
        }
        Ok(Self(array))
    }
}

impl EditionId {
    /// Lowercase hex form.
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }
}

/// What an edition publishes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EditionKind {
    /// A site: content is a published snapshot.
    Site,
    /// A shared folder or file set: content is a blob manifest.
    Share,
    /// A small capture from an engine: content is a blob manifest.
    Clip,
}

/// Content named by digest. Bytes travel separately and are checked against it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ContentRef {
    /// Digest of a canonical `PublishedSnapshotV1`, which this record never edits.
    Snapshot { digest: EditionId },
    /// Root of a blob manifest.
    Manifest { root: EditionId },
}

impl ContentRef {
    /// The named digest.
    pub fn digest(&self) -> [u8; 32] {
        match self {
            Self::Snapshot { digest } => digest.0,
            Self::Manifest { root } => root.0,
        }
    }
}

/// Where an edition came from, stated by its author.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Provenance {
    /// Producing tool or engine, for example `knot` or `turnstone`.
    pub source: String,
    /// Creation time in milliseconds, as claimed by the author.
    pub created_ms: u64,
}

/// Marks a payload sealed for admitted readers; holders keep ciphertext.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SealedMarker {
    /// Name of the sealing scheme the reader must apply.
    pub scheme: String,
}

/// Size and count bounds for an edition body.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EditionLimits {
    /// Maximum encoded body bytes.
    pub max_body_bytes: usize,
    /// Maximum provider hints.
    pub max_hints: usize,
    /// Maximum bytes in any text field or hint.
    pub max_text_bytes: usize,
    /// Maximum editions in one verified chain.
    pub max_editions: usize,
}

impl Default for EditionLimits {
    fn default() -> Self {
        Self {
            max_body_bytes: 16 * 1024,
            max_hints: 16,
            max_text_bytes: 256,
            max_editions: 65_536,
        }
    }
}

/// The signed body of one edition.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditionBody {
    /// Site, share or clip.
    pub kind: EditionKind,
    /// The content this edition names.
    pub content: ContentRef,
    /// Author-stated origin.
    pub provenance: Provenance,
    /// For the first edition of a fork: the edition it forks from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fork_of: Option<EditionId>,
    /// Advisory places to fetch from. They route; the log decides.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hints: Vec<String>,
    /// Present when the payload is sealed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sealed: Option<SealedMarker>,
}

impl EditionBody {
    /// Canonical bytes.
    ///
    /// # Errors
    /// Refuses a body that breaks `limits`.
    pub fn encode(&self, limits: EditionLimits) -> Result<Vec<u8>, EditionError> {
        self.check(limits)?;
        let bytes = serde_json::to_vec(self).map_err(|e| EditionError::Body(e.to_string()))?;
        if bytes.len() > limits.max_body_bytes {
            return Err(EditionError::Body("encoded body exceeds the limit".into()));
        }
        Ok(bytes)
    }

    /// Decode exact canonical bytes.
    ///
    /// # Errors
    /// Refuses oversized, malformed, non-canonical or out-of-bounds bodies.
    pub fn decode(bytes: &[u8], limits: EditionLimits) -> Result<Self, EditionError> {
        if bytes.len() > limits.max_body_bytes {
            return Err(EditionError::Body("encoded body exceeds the limit".into()));
        }
        let body: Self =
            serde_json::from_slice(bytes).map_err(|e| EditionError::Body(e.to_string()))?;
        body.check(limits)?;
        let canonical = serde_json::to_vec(&body).map_err(|e| EditionError::Body(e.to_string()))?;
        if canonical != bytes {
            return Err(EditionError::Body("body is not canonically encoded".into()));
        }
        Ok(body)
    }

    fn check(&self, limits: EditionLimits) -> Result<(), EditionError> {
        let refuse = |detail: &str| Err(EditionError::Body(detail.into()));
        let too_long = |text: &str| text.len() > limits.max_text_bytes;
        if too_long(&self.provenance.source) || self.provenance.source.is_empty() {
            return refuse("provenance source is empty or too long");
        }
        if self.hints.len() > limits.max_hints || self.hints.iter().any(|h| too_long(h)) {
            return refuse("too many hints or a hint is too long");
        }
        if self
            .sealed
            .as_ref()
            .is_some_and(|s| too_long(&s.scheme) || s.scheme.is_empty())
        {
            return refuse("sealed scheme is empty or too long");
        }
        match (self.kind, self.content) {
            (EditionKind::Site, ContentRef::Snapshot { .. })
            | (EditionKind::Share | EditionKind::Clip, ContentRef::Manifest { .. }) => Ok(()),
            _ => refuse("content reference does not match the edition kind"),
        }
    }
}
