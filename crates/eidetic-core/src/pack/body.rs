// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The typed `vocabulary` and `lens` bodies of `mere.pack/2` (declared-lens
//! L2; R1, D13). Plain data with axis values as IRIs: interpretation (axis
//! enums, interning, resolution) is the engine's, so a reader such as Knot
//! needs nothing of it. The CDDL rules are `vocabulary-body` and `lens-body`
//! in `mere-pack-2.cddl`.

use std::collections::{BTreeMap, BTreeSet};

use ciborium::value::Value;
use serde::{Deserialize, Serialize};

use super::canonical::{CanonicalError, sort_set};
use super::header::BodyValue;

/// What a term row declares.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TermKind {
    Predicate,
    Class,
    Family,
}

/// One term row (declared-lens §3.3, D13).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Term {
    /// The term's IRI, its identity (D9).
    pub iri: String,
    pub kind: TermKind,
    /// The family IRI this term belongs to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub family: Option<String>,
    /// A handler id the host's activation verifies (D21).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handler: Option<String>,
    /// Axis IRI to a chain of value IRIs, newest first (D32); one value is a
    /// chain of one.
    pub axes: BTreeMap<String, Vec<String>>,
    /// Language tag to label text; a slug is only a label (D9).
    pub labels: BTreeMap<String, String>,
}

/// The `vocabulary` body: term rows and axis defaults.
#[derive(Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VocabularyBody {
    /// A set, one row per IRI.
    pub terms: Vec<Term>,
    /// Axis IRI to its default value IRI. Only the core pack's are honoured (D13).
    pub defaults: BTreeMap<String, String>,
}

/// A pack the lens selects, at or above a version (D4).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Selection {
    pub pack: String,
    /// A SemVer 2.0.0 lower bound.
    pub version: String,
}

/// Which pack wins a term two packs declare (D12).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settlement {
    pub term: String,
    pub pack: String,
}

/// The standard entailment regimes a lens switches on (R9).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entail {
    pub rdfs: bool,
    #[serde(rename = "owl2-rl")]
    pub owl2_rl: bool,
}

/// A presentation choice for a term or class (Surfaces 3, D7).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Presentation {
    pub term: String,
    /// The presentation-class value IRI.
    pub class: String,
    /// What the term opens into.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opens: Option<String>,
}

/// The `lens` body (D1, D13).
#[derive(Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LensBody {
    /// Requirements, in order; one per pack id.
    pub select: Vec<Selection>,
    /// A set.
    pub settle: Vec<Settlement>,
    /// Active rule-set IRIs, a set (interpreted from L4).
    pub rules: Vec<String>,
    /// Active shape-set IRIs, a set (interpreted from L4).
    pub shapes: Vec<String>,
    pub entail: Entail,
    /// A set.
    pub present: Vec<Presentation>,
}

/// Why a typed body is refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BodyError {
    /// Not of the body's shape: a missing, unknown or mistyped field.
    Shape(String),
    /// A set is out of canonical order, or holds a duplicate.
    Set(CanonicalError),
    /// A field that must be an absolute IRI is not one.
    Iri { field: &'static str, value: String },
    /// A selection's version is not SemVer 2.0.0.
    Version(String),
    /// Two rows declare one IRI.
    DuplicateTerm(String),
    /// A lens selects one pack id twice.
    DuplicateSelection(String),
    /// A term names an axis with an empty chain.
    EmptyChain { term: String, axis: String },
    /// A label's language tag is not a BCP 47-shaped tag.
    LanguageTag(String),
}

impl std::fmt::Display for BodyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Shape(why) => write!(f, "not a body of this kind: {why}"),
            Self::Set(e) => write!(f, "{e}"),
            Self::Iri { field, value } => write!(f, "{field} {value:?} is not an absolute IRI"),
            Self::Version(v) => write!(f, "selection version {v:?} is not SemVer 2.0.0"),
            Self::DuplicateTerm(iri) => write!(f, "two rows declare {iri}"),
            Self::DuplicateSelection(iri) => write!(f, "the lens selects {iri} twice"),
            Self::EmptyChain { term, axis } => write!(f, "{term} has an empty chain for {axis}"),
            Self::LanguageTag(tag) => write!(f, "{tag:?} is not a language tag"),
        }
    }
}

impl std::error::Error for BodyError {}

impl From<CanonicalError> for BodyError {
    fn from(e: CanonicalError) -> Self {
        Self::Set(e)
    }
}

/// A typed body: read from and written to a header's [`BodyValue`].
pub trait TypedBody: Serialize + for<'de> Deserialize<'de> + Clone + PartialEq {
    /// Put sets in canonical order, refusing duplicates.
    ///
    /// # Errors
    ///
    /// A duplicate in a set.
    fn normalize(&mut self) -> Result<(), BodyError>;

    /// Check every field.
    ///
    /// # Errors
    ///
    /// The first field that fails.
    fn validate(&self) -> Result<(), BodyError>;

    /// Read and check a body value; `strict` also refuses unsorted sets.
    ///
    /// # Errors
    ///
    /// Any [`BodyError`].
    fn from_value(value: &BodyValue, strict: bool) -> Result<Self, BodyError> {
        let tree = Value::serialized(value).map_err(|e| BodyError::Shape(e.to_string()))?;
        let body: Self = tree
            .deserialized()
            .map_err(|e| BodyError::Shape(e.to_string()))?;
        body.validate()?;
        if strict {
            let mut sorted = body.clone();
            sorted.normalize()?;
            if sorted != body {
                return Err(BodyError::Set(CanonicalError::NotCanonical));
            }
        }
        Ok(body)
    }

    /// The canonical body value: sets sorted, every field checked.
    ///
    /// # Errors
    ///
    /// Any [`BodyError`].
    fn to_value(&self) -> Result<BodyValue, BodyError> {
        let mut body = self.clone();
        body.normalize()?;
        body.validate()?;
        let tree = Value::serialized(&body).map_err(|e| BodyError::Shape(e.to_string()))?;
        tree.deserialized()
            .map_err(|e| BodyError::Shape(e.to_string()))
    }
}

impl TypedBody for VocabularyBody {
    fn normalize(&mut self) -> Result<(), BodyError> {
        Ok(sort_set(&mut self.terms)?)
    }

    fn validate(&self) -> Result<(), BodyError> {
        let mut seen = BTreeSet::new();
        for term in &self.terms {
            iri("terms.iri", &term.iri)?;
            if !seen.insert(term.iri.as_str()) {
                return Err(BodyError::DuplicateTerm(term.iri.clone()));
            }
            opt_iri("terms.family", term.family.as_deref())?;
            opt_iri("terms.handler", term.handler.as_deref())?;
            for (axis, chain) in &term.axes {
                iri("terms.axes", axis)?;
                if chain.is_empty() {
                    return Err(BodyError::EmptyChain {
                        term: term.iri.clone(),
                        axis: axis.clone(),
                    });
                }
                chain.iter().try_for_each(|v| iri("terms.axes.value", v))?;
            }
            term.labels.keys().try_for_each(|t| language_tag(t))?;
        }
        for (axis, value) in &self.defaults {
            iri("defaults", axis)?;
            iri("defaults.value", value)?;
        }
        Ok(())
    }
}

impl TypedBody for LensBody {
    fn normalize(&mut self) -> Result<(), BodyError> {
        sort_set(&mut self.settle)?;
        sort_set(&mut self.rules)?;
        sort_set(&mut self.shapes)?;
        Ok(sort_set(&mut self.present)?)
    }

    fn validate(&self) -> Result<(), BodyError> {
        let mut seen = BTreeSet::new();
        for s in &self.select {
            iri("select.pack", &s.pack)?;
            semver::Version::parse(&s.version)
                .map_err(|_| BodyError::Version(s.version.clone()))?;
            if !seen.insert(s.pack.as_str()) {
                return Err(BodyError::DuplicateSelection(s.pack.clone()));
            }
        }
        for s in &self.settle {
            iri("settle.term", &s.term)?;
            iri("settle.pack", &s.pack)?;
        }
        self.rules.iter().try_for_each(|r| iri("rules", r))?;
        self.shapes.iter().try_for_each(|s| iri("shapes", s))?;
        for p in &self.present {
            iri("present.term", &p.term)?;
            iri("present.class", &p.class)?;
            opt_iri("present.opens", p.opens.as_deref())?;
        }
        Ok(())
    }
}

fn iri(field: &'static str, value: &str) -> Result<(), BodyError> {
    oxiri::Iri::parse(value)
        .map(|_| ())
        .map_err(|_| BodyError::Iri {
            field,
            value: value.to_string(),
        })
}

fn opt_iri(field: &'static str, value: Option<&str>) -> Result<(), BodyError> {
    value.map_or(Ok(()), |v| iri(field, v))
}

/// BCP 47's outer shape: subtags of 1 to 8 ASCII alphanumerics joined by `-`,
/// the first alphabetic.
fn language_tag(tag: &str) -> Result<(), BodyError> {
    let mut parts = tag.split('-');
    let first_ok = parts
        .next()
        .is_some_and(|p| (1..=8).contains(&p.len()) && p.bytes().all(|b| b.is_ascii_alphabetic()));
    let rest_ok =
        parts.all(|p| (1..=8).contains(&p.len()) && p.bytes().all(|b| b.is_ascii_alphanumeric()));
    if first_ok && rest_ok {
        Ok(())
    } else {
        Err(BodyError::LanguageTag(tag.to_string()))
    }
}
