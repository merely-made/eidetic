// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The keyed witness and its divergence report (F120 to F122, F124).

use std::cmp::Ordering;
use std::fmt;

use serde::{Deserialize, Serialize, Serializer};

use crate::{Error, FrameError, fold, hash_bytes, hash_value};

/// A framed witness's magic. The leading payload bytes cannot spell it: that
/// would need a label beginning `REWTN\0`, and labels hold no NUL.
pub(crate) const MAGIC: [u8; 8] = *b"MEREWTN\0";
pub(crate) const VERSION: u16 = 1;

/// One labelled digest.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Entry {
    pub label: String,
    pub digest: u64,
}

/// Entries unique by label, held in natural order whatever order they were
/// inserted in. Serializes as a sequence of [`Entry`]; loading refuses a
/// sequence out of order, with a duplicate, or with a NUL in a label.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Deserialize)]
#[serde(try_from = "Vec<Entry>")]
pub struct Witness {
    entries: Vec<Entry>,
}

impl Witness {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds `value`'s digest, FNV-1a over its postcard bytes.
    pub fn insert<T: Serialize + ?Sized>(
        &mut self,
        label: impl Into<String>,
        value: &T,
    ) -> Result<(), Error> {
        let label = label.into();
        let at = self.slot(&label)?;
        let digest = hash_value(value)?;
        self.entries.insert(at, Entry { label, digest });
        Ok(())
    }

    /// Adds the digest of raw bytes. The same data inserted as a serde value
    /// does not agree: a postcard sequence carries a length prefix (F128).
    pub fn insert_bytes(&mut self, label: impl Into<String>, bytes: &[u8]) -> Result<(), Error> {
        let label = label.into();
        let at = self.slot(&label)?;
        self.entries.insert(
            at,
            Entry {
                label,
                digest: hash_bytes(bytes),
            },
        );
        Ok(())
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn get(&self, label: &str) -> Option<u64> {
        self.find(label).ok().map(|i| self.entries[i].digest)
    }

    /// balaur's fold (F122): FNV-1a over each label, a NUL and its digest as
    /// a little-endian `u64`, in label order.
    pub fn digest(&self) -> u64 {
        fold(self.entries.iter().map(|e| (e.label.as_str(), e.digest)))
    }

    /// The witness as a framed postcard record.
    pub fn to_framed(&self) -> Result<Vec<u8>, FrameError> {
        framing::frame(MAGIC, VERSION, self)
    }

    /// Reads a framed witness, refusing another magic, another version or an
    /// invalid entry list.
    pub fn from_framed(bytes: &[u8]) -> Result<Self, FrameError> {
        framing::unframe(MAGIC, VERSION, bytes)
    }

    fn find(&self, label: &str) -> Result<usize, usize> {
        self.entries
            .binary_search_by(|e| natural_cmp(&e.label, label))
    }

    fn slot(&self, label: &str) -> Result<usize, Error> {
        if label.contains('\0') {
            return Err(Error::NulInLabel(label.to_owned()));
        }
        self.find(label)
            .err()
            .ok_or_else(|| Error::DuplicateLabel(label.to_owned()))
    }
}

impl Serialize for Witness {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.entries.serialize(serializer)
    }
}

impl TryFrom<Vec<Entry>> for Witness {
    type Error = Error;

    fn try_from(entries: Vec<Entry>) -> Result<Self, Error> {
        for (i, entry) in entries.iter().enumerate() {
            if entry.label.contains('\0') {
                return Err(Error::NulInLabel(entry.label.clone()));
            }
            if i > 0 && natural_cmp(&entries[i - 1].label, &entry.label) != Ordering::Less {
                return Err(Error::DuplicateLabel(entry.label.clone()));
            }
        }
        Ok(Self { entries })
    }
}

/// Where two witnesses first disagree: `None` means the label is absent on
/// that side (F121).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Divergence {
    pub label: String,
    pub left: Option<u64>,
    pub right: Option<u64>,
}

impl fmt::Display for Divergence {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let side = |d: Option<u64>| d.map_or_else(|| "absent".to_owned(), |d| format!("{d:016x}"));
        write!(
            f,
            "{}: {} vs {}",
            self.label,
            side(self.left),
            side(self.right)
        )
    }
}

/// The first label, in natural order, whose digests differ or which only one
/// side holds.
pub fn first_divergence(left: &Witness, right: &Witness) -> Option<Divergence> {
    divergences(left, right).next()
}

/// Every divergence, in natural label order.
pub fn divergences<'a>(
    left: &'a Witness,
    right: &'a Witness,
) -> impl Iterator<Item = Divergence> + 'a {
    let (mut l, mut r) = (
        left.entries.iter().peekable(),
        right.entries.iter().peekable(),
    );
    std::iter::from_fn(move || {
        loop {
            let found = match (l.peek(), r.peek()) {
                (None, None) => return None,
                (Some(a), None) => (a.label.clone(), Some(a.digest), None),
                (None, Some(b)) => (b.label.clone(), None, Some(b.digest)),
                (Some(a), Some(b)) => match natural_cmp(&a.label, &b.label) {
                    Ordering::Less => (a.label.clone(), Some(a.digest), None),
                    Ordering::Greater => (b.label.clone(), None, Some(b.digest)),
                    Ordering::Equal => {
                        let (a, b) = (l.next().unwrap(), r.next().unwrap());
                        if a.digest == b.digest {
                            continue;
                        }
                        return Some(Divergence {
                            label: a.label.clone(),
                            left: Some(a.digest),
                            right: Some(b.digest),
                        });
                    },
                },
            };
            match (found.1, found.2) {
                (Some(_), None) => drop(l.next()),
                _ => drop(r.next()),
            }
            return Some(Divergence {
                label: found.0,
                left: found.1,
                right: found.2,
            });
        }
    })
}

/// Natural order: maximal ASCII digit runs compare as numbers, other bytes by
/// value, and labels equal so far (`9` and `09`) fall back to byte order, so
/// the order is total and equal only for equal strings.
pub(crate) fn natural_cmp(a: &str, b: &str) -> Ordering {
    let (x, y) = (a.as_bytes(), b.as_bytes());
    let (mut i, mut j) = (0, 0);
    while i < x.len() && j < y.len() {
        let order = if x[i].is_ascii_digit() && y[j].is_ascii_digit() {
            let (si, sj) = (i, j);
            while i < x.len() && x[i].is_ascii_digit() {
                i += 1;
            }
            while j < y.len() && y[j].is_ascii_digit() {
                j += 1;
            }
            let (n, m) = (trim_zeros(&x[si..i]), trim_zeros(&y[sj..j]));
            n.len().cmp(&m.len()).then_with(|| n.cmp(m))
        } else {
            let order = x[i].cmp(&y[j]);
            i += 1;
            j += 1;
            order
        };
        if order != Ordering::Equal {
            return order;
        }
    }
    (x.len() - i).cmp(&(y.len() - j)).then_with(|| x.cmp(y))
}

fn trim_zeros(digits: &[u8]) -> &[u8] {
    let zeros = digits.iter().take_while(|&&c| c == b'0').count();
    &digits[zeros..]
}
