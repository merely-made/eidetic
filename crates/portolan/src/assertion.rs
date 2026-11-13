// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Portable assertion records for attributed graph relations.
//!
//! These values are owned by [`GraphLog`](crate::GraphLog), not embedded in an
//! application's generic edge payload. The edge id is the assertion identity;
//! the link between endpoint pairs remains a derived reading.

use serde::{Deserialize, Serialize};

use crate::commit::Author;
use crate::edit::EdgeId;

/// The method and evidence that explain one assertion.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssertionBasis {
    /// A human-readable reason.
    #[serde(default)]
    pub note: Option<String>,
    /// The rule, script, engine, or model that produced the assertion.
    #[serde(default)]
    pub method: Option<AssertionMethod>,
    /// Supporting resources, captures, or other assertion identities.
    #[serde(default)]
    pub evidence: Vec<String>,
}

/// A versioned method that produced an assertion.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssertionMethod {
    /// A stable method IRI.
    pub iri: String,
    /// Optional method version.
    #[serde(default)]
    pub version: Option<String>,
}

/// A retained withdrawal event.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssertionWithdrawal {
    /// Caller-supplied record time. `None` remains undated.
    pub at_ms: Option<u64>,
    /// The journal author who performed the withdrawal.
    pub by: Author,
}

/// Record-time span of one assertion; valid time is intentionally separate.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssertionSpan {
    /// Time the assertion was first recorded. Legacy/source-unknown values stay
    /// `None`; journal sequence is not converted to a timestamp.
    #[serde(default)]
    pub first_asserted_ms: Option<u64>,
    /// Most recent explicit confirmation time, if supplied.
    #[serde(default)]
    pub last_asserted_ms: Option<u64>,
    /// Terminal withdrawal, retained for history reads.
    #[serde(default)]
    pub withdrawn: Option<AssertionWithdrawal>,
}

impl AssertionSpan {
    /// A new span with the same optional source time as first and last record.
    pub fn new(at_ms: Option<u64>) -> Self {
        Self {
            first_asserted_ms: at_ms,
            last_asserted_ms: at_ms,
            withdrawn: None,
        }
    }

    /// Whether the assertion is currently held.
    pub fn is_held(&self) -> bool {
        self.withdrawn.is_none()
    }
}

/// One assertion and its generic relation payload.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(bound(
    serialize = "Id: Serialize, E: Serialize",
    deserialize = "Id: Deserialize<'de>, E: Deserialize<'de>"
))]
pub struct AssertionRecord<Id, E> {
    /// Stable identity minted by Chartulary.
    pub id: EdgeId,
    /// Directed subject endpoint identity.
    pub from: Id,
    /// Directed object endpoint identity.
    pub to: Id,
    /// Generic application relation payload.
    pub edge: E,
    /// Source asserter, separate from the batch author who admitted this edit.
    /// Missing identity stays missing; legacy records use their known batch author.
    pub asserter: Option<Author>,
    /// Record-time lifecycle.
    pub span: AssertionSpan,
    /// Why this assertion exists.
    #[serde(default)]
    pub basis: AssertionBasis,
}

impl<Id, E> AssertionRecord<Id, E> {
    /// Whether this record is still held.
    pub fn is_held(&self) -> bool {
        self.span.is_held()
    }
}
