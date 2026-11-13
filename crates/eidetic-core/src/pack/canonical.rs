// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The canonical CBOR codec (declared-lens D5): RFC 8949 §4.2.1 core
//! deterministic encoding over `ciborium`, text keys only, no tags, no floats,
//! no null, and strict decoding by re-encoding and comparing bytes.
//!
//! `ciborium` writes integers and lengths in shortest form and every length
//! definite; map keys are put in order with its own `CanonicalValue` ordering
//! (RFC 8949 §4.2.3's length-first rule, which equals §4.2.1's bytewise rule
//! when every key is a text string).

use std::cmp::Ordering;

use ciborium::value::{CanonicalValue, Value};
use serde::Serialize;
use serde::de::DeserializeOwned;

/// Why bytes are not a canonical document, or a value cannot become one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CanonicalError {
    /// Not well-formed CBOR, or not the expected shape (a wrong type, a float
    /// where an integer belongs, a duplicate or unknown field).
    Malformed(String),
    /// Bytes remain after the one data item.
    TrailingBytes,
    /// Well-formed, but re-encoding gives other bytes: unsorted keys, a
    /// non-shortest integer or length, an indefinite length, a tag, a
    /// duplicate map key or a null that decoding dropped.
    NotCanonical,
    /// A float, which no identity-bearing field may hold.
    Float,
    /// A tag, which the profile does not admit.
    Tag,
    /// A null or undefined value.
    Null,
    /// A map key that is not a text string.
    NonTextKey,
    /// Two equal keys in one map.
    DuplicateKey(String),
}

impl std::fmt::Display for CanonicalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Malformed(why) => write!(f, "not a well-formed document: {why}"),
            Self::TrailingBytes => f.write_str("bytes follow the document"),
            Self::NotCanonical => f.write_str("not in canonical form (RFC 8949 §4.2.1)"),
            Self::Float => f.write_str("a float in an identity-bearing field"),
            Self::Tag => f.write_str("a CBOR tag"),
            Self::Null => f.write_str("a null or undefined value"),
            Self::NonTextKey => f.write_str("a map key that is not text"),
            Self::DuplicateKey(key) => write!(f, "duplicate map key {key:?}"),
        }
    }
}

impl std::error::Error for CanonicalError {}

/// Encode `value` in canonical form.
///
/// # Errors
///
/// Refuses a value whose data model holds a float, tag, null, non-text map
/// key or duplicate key.
pub fn to_canonical<T: Serialize + ?Sized>(value: &T) -> Result<Vec<u8>, CanonicalError> {
    let mut tree =
        Value::serialized(value).map_err(|e| CanonicalError::Malformed(e.to_string()))?;
    canonicalize(&mut tree)?;
    let mut out = Vec::new();
    ciborium::ser::into_writer(&tree, &mut out)
        .map_err(|e| CanonicalError::Malformed(e.to_string()))?;
    Ok(out)
}

/// Decode `bytes` strictly: one data item, then re-encoded and compared.
///
/// # Errors
///
/// Refuses malformed CBOR, trailing bytes, and any document whose canonical
/// re-encoding differs from `bytes`.
pub fn from_canonical<T: Serialize + DeserializeOwned>(bytes: &[u8]) -> Result<T, CanonicalError> {
    let mut rest = bytes;
    let value: T = ciborium::de::from_reader(&mut rest)
        .map_err(|e| CanonicalError::Malformed(e.to_string()))?;
    if !rest.is_empty() {
        return Err(CanonicalError::TrailingBytes);
    }
    if to_canonical(&value)? != bytes {
        return Err(CanonicalError::NotCanonical);
    }
    Ok(value)
}

/// Sort `items` by their canonical encodings, refusing duplicates: the order
/// a set-valued array takes in a canonical document.
pub(crate) fn sort_set<T: Serialize>(items: &mut Vec<T>) -> Result<(), CanonicalError> {
    let mut keyed = Vec::with_capacity(items.len());
    for item in items.drain(..) {
        keyed.push((to_canonical(&item)?, item));
    }
    keyed.sort_by(|a, b| a.0.cmp(&b.0));
    if keyed.windows(2).any(|w| w[0].0 == w[1].0) {
        return Err(CanonicalError::DuplicateKey("a set element".to_string()));
    }
    items.extend(keyed.into_iter().map(|(_, item)| item));
    Ok(())
}

fn canonicalize(value: &mut Value) -> Result<(), CanonicalError> {
    match value {
        Value::Float(_) => Err(CanonicalError::Float),
        Value::Tag(..) => Err(CanonicalError::Tag),
        Value::Null => Err(CanonicalError::Null),
        Value::Array(items) => items.iter_mut().try_for_each(canonicalize),
        Value::Map(entries) => {
            for (key, item) in entries.iter_mut() {
                if !matches!(key, Value::Text(_)) {
                    return Err(CanonicalError::NonTextKey);
                }
                canonicalize(item)?;
            }
            entries.sort_by(|a, b| key_order(&a.0, &b.0));
            if let Some(w) = entries
                .windows(2)
                .find(|w| key_order(&w[0].0, &w[1].0) == Ordering::Equal)
            {
                let key = w[0].0.as_text().unwrap_or_default().to_string();
                return Err(CanonicalError::DuplicateKey(key));
            }
            Ok(())
        }
        Value::Integer(_) | Value::Bytes(_) | Value::Text(_) | Value::Bool(_) => Ok(()),
        // `Value` is non-exhaustive; anything newer is refused, not guessed at.
        _ => Err(CanonicalError::Malformed(
            "an unsupported CBOR item".to_string(),
        )),
    }
}

fn key_order(a: &Value, b: &Value) -> Ordering {
    CanonicalValue::from(a.clone()).cmp(&CanonicalValue::from(b.clone()))
}
