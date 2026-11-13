// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The authored form's compiler (declared-lens §3.1, D13): a `pack.toml`
//! (TOML 1.0) written with CURIEs (W3C CURIE Syntax 1.0) against the prefixes
//! it declares, compiled to the canonical CBOR document. Prefixes stay in the
//! authored form; the canonical one holds expanded IRIs only, so a prefix
//! rename does not move the digest. L2 compiles the `vocabulary` and `lens`
//! kinds; `parts` and the other kinds wait for their own phases.

use std::collections::BTreeMap;

use serde::Deserialize;

use super::body::{
    BodyError, Entail, LensBody, Presentation, Selection, Settlement, Term, TermKind, TypedBody,
    VocabularyBody,
};
use super::header::{Contribution, HeaderError, PACK2_FORMAT, PackHeader, PackKind, Requirement};

/// Schemes written as plain IRIs without a `//` authority; any other
/// `name:rest` must name a declared prefix, so a mistyped prefix is refused.
const OPAQUE_SCHEMES: [&str; 4] = ["urn", "did", "tag", "mailto"];

/// Why an authored pack does not compile.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AuthoredError {
    /// Not TOML of the authored shape.
    Toml(String),
    /// A prefix name is not an NCName, or its value is not an absolute IRI.
    Prefix(String),
    /// A CURIE names an undeclared prefix.
    UnknownPrefix { field: &'static str, value: String },
    /// Something the authored form does not compile yet.
    Unsupported(&'static str),
    /// A body is refused.
    Body { kind: PackKind, error: BodyError },
    /// The compiled header is refused.
    Header(HeaderError),
}

impl std::fmt::Display for AuthoredError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Toml(why) => write!(f, "not an authored pack: {why}"),
            Self::Prefix(p) => write!(f, "prefix {p:?} is not a valid declaration"),
            Self::UnknownPrefix { field, value } => {
                write!(f, "{field} {value:?} names no declared prefix")
            }
            Self::Unsupported(what) => {
                write!(f, "{what} is not compiled from the authored form yet")
            }
            Self::Body { kind, error } => write!(f, "the {kind:?} body: {error}"),
            Self::Header(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for AuthoredError {}

impl From<HeaderError> for AuthoredError {
    fn from(e: HeaderError) -> Self {
        Self::Header(e)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Authored {
    #[serde(default)]
    format: Option<String>,
    id: String,
    version: String,
    kind: Vec<PackKind>,
    licence: String,
    #[serde(default)]
    credit: Vec<String>,
    #[serde(default)]
    requires: Vec<Requirement>,
    #[serde(default)]
    provides: Vec<String>,
    #[serde(default)]
    contributes: Vec<Contribution>,
    #[serde(default)]
    scopes: Vec<String>,
    #[serde(default)]
    parts: Vec<toml::Value>,
    #[serde(default)]
    prefixes: BTreeMap<String, String>,
    #[serde(default)]
    vocabulary: Option<AuthoredVocabulary>,
    #[serde(default)]
    lens: Option<AuthoredLens>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AuthoredVocabulary {
    #[serde(default)]
    terms: Vec<AuthoredTerm>,
    #[serde(default)]
    defaults: BTreeMap<String, String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AuthoredTerm {
    iri: String,
    kind: TermKind,
    #[serde(default)]
    family: Option<String>,
    #[serde(default)]
    handler: Option<String>,
    #[serde(default)]
    axes: BTreeMap<String, OneOrChain>,
    #[serde(default)]
    labels: BTreeMap<String, String>,
}

/// An axis value as authored: one value, or a chain newest first.
#[derive(Deserialize)]
#[serde(untagged)]
enum OneOrChain {
    One(String),
    Chain(Vec<String>),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AuthoredLens {
    #[serde(default)]
    select: Vec<Selection>,
    #[serde(default)]
    settle: Vec<Settlement>,
    #[serde(default)]
    rules: Vec<String>,
    #[serde(default)]
    shapes: Vec<String>,
    #[serde(default)]
    entail: Entail,
    #[serde(default)]
    present: Vec<Presentation>,
}

/// Compile an authored `pack.toml` to its header and canonical bytes.
///
/// # Errors
///
/// An [`AuthoredError`] naming the first thing refused.
pub fn compile_authored(text: &str) -> Result<(PackHeader, Vec<u8>), AuthoredError> {
    let a: Authored = toml::from_str(text).map_err(|e| AuthoredError::Toml(e.to_string()))?;
    if !a.parts.is_empty() {
        return Err(AuthoredError::Unsupported("parts"));
    }
    if a.kind
        .iter()
        .any(|k| !matches!(k, PackKind::Vocabulary | PackKind::Lens))
    {
        return Err(AuthoredError::Unsupported(
            "a kind other than vocabulary or lens",
        ));
    }
    let x = Curies::new(&a.prefixes)?;
    // A declared kind without its section has an empty body; a section
    // without its kind is refused by the header's body check.
    let mut body = BTreeMap::new();
    if let Some(v) = &a.vocabulary {
        body.insert(
            PackKind::Vocabulary,
            typed(PackKind::Vocabulary, &x.vocabulary(v)?)?,
        );
    } else if a.kind.contains(&PackKind::Vocabulary) {
        body.insert(
            PackKind::Vocabulary,
            typed(PackKind::Vocabulary, &VocabularyBody::default())?,
        );
    }
    if let Some(l) = &a.lens {
        body.insert(PackKind::Lens, typed(PackKind::Lens, &x.lens(l)?)?);
    } else if a.kind.contains(&PackKind::Lens) {
        body.insert(PackKind::Lens, typed(PackKind::Lens, &LensBody::default())?);
    }
    let header = PackHeader {
        format: a.format.unwrap_or_else(|| PACK2_FORMAT.to_string()),
        id: x.expand("id", &a.id)?,
        version: a.version,
        kind: a.kind,
        licence: a.licence,
        credit: a.credit,
        requires: a
            .requires
            .iter()
            .map(|r| x.requirement(r))
            .collect::<Result<_, _>>()?,
        provides: x.all("provides", &a.provides)?,
        contributes: a
            .contributes
            .iter()
            .map(|c| {
                Ok(Contribution {
                    kind: c.kind,
                    id: x.expand("contributes.id", &c.id)?,
                })
            })
            .collect::<Result<_, AuthoredError>>()?,
        scopes: a.scopes,
        parts: Vec::new(),
        body,
    };
    let canonical = header.to_canonical()?;
    Ok((PackHeader::from_canonical(&canonical)?, canonical))
}

fn typed<B: TypedBody>(kind: PackKind, body: &B) -> Result<super::BodyValue, AuthoredError> {
    body.to_value()
        .map_err(|error| AuthoredError::Body { kind, error })
}

/// The pack's prefix map, and CURIE expansion against it.
struct Curies<'a>(&'a BTreeMap<String, String>);

impl<'a> Curies<'a> {
    fn new(prefixes: &'a BTreeMap<String, String>) -> Result<Self, AuthoredError> {
        for (name, ns) in prefixes {
            if !is_ncname(name) || oxiri::Iri::parse(ns.as_str()).is_err() {
                return Err(AuthoredError::Prefix(name.clone()));
            }
        }
        Ok(Self(prefixes))
    }

    /// A CURIE (plain or `[safe]`) to its IRI; an absolute IRI is kept.
    fn expand(&self, field: &'static str, value: &str) -> Result<String, AuthoredError> {
        let unknown = || AuthoredError::UnknownPrefix {
            field,
            value: value.to_string(),
        };
        let safe = value.strip_prefix('[').and_then(|v| v.strip_suffix(']'));
        let curie = safe.unwrap_or(value);
        let Some((prefix, reference)) = curie.split_once(':') else {
            return Ok(value.to_string());
        };
        if let Some(ns) = self.0.get(prefix) {
            return Ok(format!("{ns}{reference}"));
        }
        if safe.is_none() && (reference.starts_with("//") || OPAQUE_SCHEMES.contains(&prefix)) {
            return Ok(value.to_string());
        }
        Err(unknown())
    }

    fn all(&self, field: &'static str, values: &[String]) -> Result<Vec<String>, AuthoredError> {
        values.iter().map(|v| self.expand(field, v)).collect()
    }

    fn opt(
        &self,
        field: &'static str,
        v: Option<&String>,
    ) -> Result<Option<String>, AuthoredError> {
        v.map(|v| self.expand(field, v)).transpose()
    }

    fn requirement(&self, r: &Requirement) -> Result<Requirement, AuthoredError> {
        Ok(match r {
            Requirement::Pack { pack, version } => Requirement::Pack {
                pack: self.expand("requires.pack", pack)?,
                version: version.clone(),
            },
            Requirement::Capability { capability } => Requirement::Capability {
                capability: self.expand("requires.capability", capability)?,
            },
        })
    }

    fn vocabulary(&self, v: &AuthoredVocabulary) -> Result<VocabularyBody, AuthoredError> {
        let mut terms = Vec::with_capacity(v.terms.len());
        for t in &v.terms {
            let mut axes = BTreeMap::new();
            for (axis, value) in &t.axes {
                let chain = match value {
                    OneOrChain::One(one) => vec![self.expand("terms.axes.value", one)?],
                    OneOrChain::Chain(chain) => self.all("terms.axes.value", chain)?,
                };
                axes.insert(self.expand("terms.axes", axis)?, chain);
            }
            terms.push(Term {
                iri: self.expand("terms.iri", &t.iri)?,
                kind: t.kind,
                family: self.opt("terms.family", t.family.as_ref())?,
                handler: self.opt("terms.handler", t.handler.as_ref())?,
                axes,
                labels: t.labels.clone(),
            });
        }
        let defaults = v
            .defaults
            .iter()
            .map(|(a, d)| {
                Ok((
                    self.expand("defaults", a)?,
                    self.expand("defaults.value", d)?,
                ))
            })
            .collect::<Result<_, AuthoredError>>()?;
        Ok(VocabularyBody { terms, defaults })
    }

    fn lens(&self, l: &AuthoredLens) -> Result<LensBody, AuthoredError> {
        Ok(LensBody {
            select: l
                .select
                .iter()
                .map(|s| {
                    Ok(Selection {
                        pack: self.expand("select.pack", &s.pack)?,
                        version: s.version.clone(),
                    })
                })
                .collect::<Result<_, AuthoredError>>()?,
            settle: l
                .settle
                .iter()
                .map(|s| {
                    Ok(Settlement {
                        term: self.expand("settle.term", &s.term)?,
                        pack: self.expand("settle.pack", &s.pack)?,
                    })
                })
                .collect::<Result<_, AuthoredError>>()?,
            rules: self.all("rules", &l.rules)?,
            shapes: self.all("shapes", &l.shapes)?,
            entail: l.entail,
            present: l
                .present
                .iter()
                .map(|p| {
                    Ok(Presentation {
                        term: self.expand("present.term", &p.term)?,
                        class: self.expand("present.class", &p.class)?,
                        opens: self.opt("present.opens", p.opens.as_ref())?,
                    })
                })
                .collect::<Result<_, AuthoredError>>()?,
        })
    }
}

/// XML's NCName, ASCII subset: a letter or `_`, then letters, digits, `-`, `_`, `.`.
fn is_ncname(name: &str) -> bool {
    let mut bytes = name.bytes();
    bytes
        .next()
        .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_')
        && bytes.all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}
