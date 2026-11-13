// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The `mere.pack/2` header (declared-lens D10), its pin and its digest.
//!
//! The schema is `mere-pack-2.cddl` beside this file ([`PACK2_CDDL`]). The
//! header is plain data; [`PackHeader::to_canonical`] puts set-valued arrays in
//! canonical order and checks every field, and [`PackHeader::from_canonical`]
//! decodes strictly and refuses anything `to_canonical` would not write.

use std::collections::BTreeMap;

use serde::de::{self, Deserializer, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Serialize, Serializer};

use super::body::{BodyError, LensBody, TypedBody, VocabularyBody};
use super::canonical::{CanonicalError, from_canonical, sort_set, to_canonical};
use crate::schema::{Hash, HashFn};

/// The format string a `mere.pack/2` document carries.
pub const PACK2_FORMAT: &str = "mere.pack/2";

/// The header's CDDL schema (RFC 8610).
pub const PACK2_CDDL: &str = include_str!("mere-pack-2.cddl");

/// What a pack holds. A pack may be several kinds; its body has one entry each.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PackKind {
    /// Prefixes and term rows.
    Vocabulary,
    /// SPARQL 1.2 RL rule sets.
    Rules,
    /// SHACL shapes graphs.
    Shapes,
    /// A declared lens (D1).
    Lens,
    /// A participant: today's `mere.pack/v1` content (§3.7).
    Participant,
    /// A host profile that pins a lens; the engine never decodes it (D2).
    HostProfile,
}

/// What a pack needs: another pack at or above a version (D4), or a
/// capability named by IRI (D16).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Requirement {
    /// A pack id and the lowest version that satisfies.
    Pack {
        /// The required pack's id.
        pack: String,
        /// The lower bound, a SemVer 2.0.0 version.
        version: String,
    },
    /// An environment capability, an open IRI.
    Capability {
        /// The capability's IRI.
        capability: String,
    },
}

/// The four contribution kinds of doctrine Surfaces 3.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ContributionKind {
    /// A reader or surface producer a node's resource opens into.
    Presentation,
    /// A feed into an existing pane.
    PaneAdapter,
    /// A new pane: a new question.
    Pane,
    /// A participant under a grant.
    Servitor,
}

/// One declared contribution: its kind and its id.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Contribution {
    pub kind: ContributionKind,
    /// The contribution's IRI.
    pub id: String,
}

/// A referenced file (R6), described the way OCI's content descriptor is:
/// media type, digest and size. The bytes live elsewhere, as muniment blobs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Part {
    /// The part's name within the pack.
    pub name: String,
    /// What the part is for (`wasm-component`, `scenario-source`, `asset`).
    pub role: String,
    /// The media type.
    pub media: String,
    pub digest: Multihash,
    /// The part's size in bytes.
    pub bytes: u64,
}

/// A digest written as a binary multihash (a CBOR byte string).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Multihash(pub Hash);

impl Serialize for Multihash {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_bytes(&self.0.to_multihash_bytes())
    }
}

impl<'de> Deserialize<'de> for Multihash {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct BytesVisitor;
        impl Visitor<'_> for BytesVisitor {
            type Value = Multihash;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("a multihash byte string")
            }
            fn visit_bytes<E: de::Error>(self, bytes: &[u8]) -> Result<Multihash, E> {
                Hash::from_multihash_bytes(bytes)
                    .map(Multihash)
                    .map_err(E::custom)
            }
        }
        deserializer.deserialize_bytes(BytesVisitor)
    }
}

/// Plain data in a kind's body: no floats, tags or null, and text map keys.
/// The `vocabulary` and `lens` bodies must also read as their typed forms
/// ([`VocabularyBody`], [`LensBody`]); the other kinds stay plain until theirs land.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BodyValue {
    Bool(bool),
    Integer(i64),
    Text(String),
    Bytes(Vec<u8>),
    Array(Vec<BodyValue>),
    Map(BTreeMap<String, BodyValue>),
}

impl Serialize for BodyValue {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Bool(b) => serializer.serialize_bool(*b),
            Self::Integer(i) => serializer.serialize_i64(*i),
            Self::Text(t) => serializer.serialize_str(t),
            Self::Bytes(b) => serializer.serialize_bytes(b),
            Self::Array(items) => items.serialize(serializer),
            Self::Map(entries) => entries.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for BodyValue {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(BodyVisitor)
    }
}

struct BodyVisitor;

impl<'de> Visitor<'de> for BodyVisitor {
    type Value = BodyValue;

    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("a bool, integer, text, bytes, array or text-keyed map")
    }
    fn visit_bool<E: de::Error>(self, v: bool) -> Result<BodyValue, E> {
        Ok(BodyValue::Bool(v))
    }
    fn visit_i64<E: de::Error>(self, v: i64) -> Result<BodyValue, E> {
        Ok(BodyValue::Integer(v))
    }
    fn visit_u64<E: de::Error>(self, v: u64) -> Result<BodyValue, E> {
        i64::try_from(v)
            .map(BodyValue::Integer)
            .map_err(|_| E::custom("an integer beyond the int64 range"))
    }
    fn visit_str<E: de::Error>(self, v: &str) -> Result<BodyValue, E> {
        Ok(BodyValue::Text(v.to_string()))
    }
    fn visit_bytes<E: de::Error>(self, v: &[u8]) -> Result<BodyValue, E> {
        Ok(BodyValue::Bytes(v.to_vec()))
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<BodyValue, A::Error> {
        let mut items = Vec::new();
        while let Some(item) = seq.next_element()? {
            items.push(item);
        }
        Ok(BodyValue::Array(items))
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<BodyValue, A::Error> {
        let mut entries = BTreeMap::new();
        while let Some((key, value)) = map.next_entry::<String, BodyValue>()? {
            if entries.contains_key(&key) {
                return Err(de::Error::custom(format!("duplicate map key {key:?}")));
            }
            entries.insert(key, value);
        }
        Ok(BodyValue::Map(entries))
    }
}

/// The shared pack header (declared-lens §3.2), field for field the CDDL's
/// `pack` rule.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackHeader {
    /// Always [`PACK2_FORMAT`].
    pub format: String,
    /// The pack id, an absolute IRI (R4).
    pub id: String,
    /// A SemVer 2.0.0 version.
    pub version: String,
    pub kind: Vec<PackKind>,
    /// An SPDX license expression.
    pub licence: String,
    /// Attribution lines, in the author's order.
    pub credit: Vec<String>,
    pub requires: Vec<Requirement>,
    /// Namespace IRIs whose terms this pack declares.
    pub provides: Vec<String>,
    pub contributes: Vec<Contribution>,
    /// Grants asked for, from the host's closed list (D16).
    pub scopes: Vec<String>,
    pub parts: Vec<Part>,
    /// One entry per declared kind.
    pub body: BTreeMap<PackKind, BodyValue>,
}

/// `{id, version, digest}`: what a requirement resolves to, recorded beside
/// the document in a selection record or an envelope.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pin {
    pub id: String,
    pub version: String,
    pub digest: Multihash,
}

impl Pin {
    /// Whether `canonical` is the document this pin names: the digest under
    /// the pin's own function, and the id and version inside.
    pub fn matches(&self, canonical: &[u8]) -> bool {
        Hash::digest(self.digest.0.func(), canonical) == self.digest.0
            && PackHeader::from_canonical(canonical)
                .is_ok_and(|h| h.id == self.id && h.version == self.version)
    }
}

/// Why a header is refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HeaderError {
    /// The bytes are not a canonical document of this shape.
    Canonical(CanonicalError),
    /// `format` is not [`PACK2_FORMAT`].
    Format(String),
    /// A field that must be an absolute IRI is not one.
    Iri { field: &'static str, value: String },
    /// A field that must be a SemVer 2.0.0 version is not one.
    Version { field: &'static str, value: String },
    /// `licence` is not an SPDX license expression.
    Licence(String),
    /// `kind` is empty.
    NoKind,
    /// The body's entries are not exactly the declared kinds.
    BodyKinds,
    /// A part's name, role or media type is empty.
    EmptyPartField(&'static str),
    /// A typed body is refused.
    Body { kind: PackKind, error: BodyError },
}

impl std::fmt::Display for HeaderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Canonical(e) => write!(f, "{e}"),
            Self::Format(v) => write!(f, "format {v:?} is not {PACK2_FORMAT}"),
            Self::Iri { field, value } => write!(f, "{field} {value:?} is not an absolute IRI"),
            Self::Version { field, value } => write!(f, "{field} {value:?} is not SemVer 2.0.0"),
            Self::Licence(v) => write!(f, "licence {v:?} is not an SPDX expression"),
            Self::NoKind => f.write_str("a pack declares at least one kind"),
            Self::BodyKinds => f.write_str("the body holds one entry per declared kind"),
            Self::EmptyPartField(field) => write!(f, "a part's {field} is empty"),
            Self::Body { kind, error } => write!(f, "the {kind:?} body: {error}"),
        }
    }
}

impl std::error::Error for HeaderError {}

impl From<CanonicalError> for HeaderError {
    fn from(e: CanonicalError) -> Self {
        Self::Canonical(e)
    }
}

impl PackHeader {
    /// The canonical bytes: set-valued arrays sorted, every field checked.
    ///
    /// # Errors
    ///
    /// Refuses a header [`Self::validate`] refuses, or a duplicate in a set.
    pub fn to_canonical(&self) -> Result<Vec<u8>, HeaderError> {
        let mut header = self.clone();
        header.normalize()?;
        // `normalize` checked the typed bodies as it rewrote them.
        header.validate_fields()?;
        Ok(to_canonical(&header)?)
    }

    /// Decode canonical bytes strictly and check every field.
    ///
    /// # Errors
    ///
    /// Refuses non-canonical bytes (unsorted sets included) and invalid fields.
    pub fn from_canonical(bytes: &[u8]) -> Result<Self, HeaderError> {
        let header: Self = from_canonical(bytes)?;
        header.validate_fields()?;
        let mut sorted = header.clone();
        sorted.sort_sets()?;
        if sorted != header {
            return Err(HeaderError::Canonical(CanonicalError::NotCanonical));
        }
        // Typed bodies read strictly: valid, and their sets already in order.
        header.typed_body_strict::<VocabularyBody>(PackKind::Vocabulary)?;
        header.typed_body_strict::<LensBody>(PackKind::Lens)?;
        Ok(header)
    }

    fn typed_body_strict<B: TypedBody>(&self, kind: PackKind) -> Result<(), HeaderError> {
        let Some(value) = self.body.get(&kind) else {
            return Ok(());
        };
        match B::from_value(value, true) {
            Ok(_) => Ok(()),
            Err(BodyError::Set(e)) => Err(HeaderError::Canonical(e)),
            Err(error) => Err(HeaderError::Body { kind, error }),
        }
    }

    /// The pin of the document whose canonical bytes are `canonical`.
    ///
    /// # Errors
    ///
    /// Refuses bytes that are not a valid canonical header.
    pub fn pin_of(canonical: &[u8], func: HashFn) -> Result<Pin, HeaderError> {
        let header = Self::from_canonical(canonical)?;
        Ok(Pin {
            id: header.id,
            version: header.version,
            digest: Multihash(Hash::digest(func, canonical)),
        })
    }

    fn sort_sets(&mut self) -> Result<(), CanonicalError> {
        sort_set(&mut self.kind)?;
        sort_set(&mut self.requires)?;
        sort_set(&mut self.provides)?;
        sort_set(&mut self.contributes)?;
        sort_set(&mut self.scopes)?;
        sort_set(&mut self.parts)
    }

    fn normalize(&mut self) -> Result<(), HeaderError> {
        self.sort_sets()?;
        for (kind, value) in &mut self.body {
            let typed = match kind {
                PackKind::Vocabulary => {
                    VocabularyBody::from_value(value, false).and_then(|b| b.to_value())
                }
                PackKind::Lens => LensBody::from_value(value, false).and_then(|b| b.to_value()),
                _ => continue,
            };
            *value = typed.map_err(|error| HeaderError::Body { kind: *kind, error })?;
        }
        Ok(())
    }

    /// The typed `vocabulary` body, if the pack declares that kind.
    ///
    /// # Errors
    ///
    /// The body does not read as a [`VocabularyBody`].
    pub fn vocabulary(&self) -> Result<Option<VocabularyBody>, HeaderError> {
        self.typed_body(PackKind::Vocabulary)
    }

    /// The typed `lens` body, if the pack declares that kind.
    ///
    /// # Errors
    ///
    /// The body does not read as a [`LensBody`].
    pub fn lens(&self) -> Result<Option<LensBody>, HeaderError> {
        self.typed_body(PackKind::Lens)
    }

    fn typed_body<B: TypedBody>(&self, kind: PackKind) -> Result<Option<B>, HeaderError> {
        self.body
            .get(&kind)
            .map(|v| B::from_value(v, false).map_err(|error| HeaderError::Body { kind, error }))
            .transpose()
    }

    /// Check every field against its spec.
    ///
    /// # Errors
    ///
    /// The first field that fails, as a [`HeaderError`].
    pub fn validate(&self) -> Result<(), HeaderError> {
        self.validate_fields()?;
        self.vocabulary()?;
        self.lens()?;
        Ok(())
    }

    /// Every field but the typed bodies' contents.
    fn validate_fields(&self) -> Result<(), HeaderError> {
        if self.format != PACK2_FORMAT {
            return Err(HeaderError::Format(self.format.clone()));
        }
        iri("id", &self.id)?;
        semver_version("version", &self.version)?;
        if self.kind.is_empty() {
            return Err(HeaderError::NoKind);
        }
        if spdx::Expression::parse(&self.licence).is_err() {
            return Err(HeaderError::Licence(self.licence.clone()));
        }
        for requirement in &self.requires {
            match requirement {
                Requirement::Pack { pack, version } => {
                    iri("requires.pack", pack)?;
                    semver_version("requires.version", version)?;
                }
                Requirement::Capability { capability } => iri("requires.capability", capability)?,
            }
        }
        self.provides.iter().try_for_each(|p| iri("provides", p))?;
        self.contributes
            .iter()
            .try_for_each(|c| iri("contributes.id", &c.id))?;
        for part in &self.parts {
            for (field, value) in [
                ("name", &part.name),
                ("role", &part.role),
                ("media", &part.media),
            ] {
                if value.is_empty() {
                    return Err(HeaderError::EmptyPartField(field));
                }
            }
        }
        let mut kinds = self.kind.clone();
        kinds.sort();
        kinds.dedup();
        if !kinds.iter().eq(self.body.keys()) || kinds.len() != self.kind.len() {
            return Err(HeaderError::BodyKinds);
        }
        Ok(())
    }
}

fn iri(field: &'static str, value: &str) -> Result<(), HeaderError> {
    oxiri::Iri::parse(value)
        .map(|_| ())
        .map_err(|_| HeaderError::Iri {
            field,
            value: value.to_string(),
        })
}

fn semver_version(field: &'static str, value: &str) -> Result<(), HeaderError> {
    semver::Version::parse(value)
        .map(|_| ())
        .map_err(|_| HeaderError::Version {
            field,
            value: value.to_string(),
        })
}
