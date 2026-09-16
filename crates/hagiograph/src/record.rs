// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The standing record: high-water marks per axis, and who holds them.
//!
//! Significance is abnormality measured against a world's own record, so
//! there has to be a record. Its join is the whole reason for its shape:
//! taking the higher mark per axis is commutative, associative and
//! idempotent, which makes it a join-semilattice — forked and grafted
//! worlds combine their records in any order, any number of times, with no
//! merge protocol. A pure maximum would forget *who*, so a [`Mark`] keeps
//! the threshold and the set of holders standing at it: a strictly higher
//! mark replaces both, a tie unions the holders.
//!
//! Axes (`A`) and holders (`H`) are left to the product; this crate only
//! needs them ordered.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

/// A high-water mark, and who stands at it.
///
/// Ties share it; a strictly higher mark takes it outright and forgets the
/// old holders, which is what keeps this a few integers rather than a full
/// history of who has ever come close.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mark<H: Ord> {
    /// The highest value seen on this axis.
    pub high: i64,
    /// Who is standing at `high`.
    pub holders: BTreeSet<H>,
}

impl<H: Ord> Mark<H> {
    fn new(high: i64, by: H) -> Self {
        Self {
            high,
            holders: BTreeSet::from([by]),
        }
    }
}

impl<H: Ord + Clone> Mark<H> {
    /// Joins another mark into this one: higher replaces, equal unions,
    /// lower is ignored. The whole semilattice in one match — it does not
    /// matter which side this is called on, or how many times.
    fn join(&mut self, other: &Mark<H>) {
        match other.high.cmp(&self.high) {
            Ordering::Greater => *self = other.clone(),
            Ordering::Equal => self.holders.extend(other.holders.iter().cloned()),
            Ordering::Less => {},
        }
    }
}

/// Everything noted so far, keyed by axis.
///
/// `marks` stays private and `BTreeMap`-backed so iteration and
/// serialization are deterministic regardless of insertion order.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Record<A: Ord, H: Ord> {
    marks: BTreeMap<A, Mark<H>>,
}

// Default is written by hand rather than derived: `derive(Default)` would add
// an `A: Default, H: Default` bound that an empty `BTreeMap` never needs.
impl<A: Ord, H: Ord> Default for Record<A, H> {
    fn default() -> Self {
        Self {
            marks: BTreeMap::new(),
        }
    }
}

impl<A: Ord, H: Ord> Record<A, H> {
    pub fn new() -> Self {
        Self::default()
    }

    /// The standing mark for one axis, if anyone has set one.
    pub fn standing(&self, axis: &A) -> Option<&Mark<H>> {
        self.marks.get(axis)
    }

    /// Whether this would be the first, or the best, anyone has managed.
    ///
    /// The abnormality query: one comparison, which is the whole reason the
    /// record is a handful of integers rather than an index.
    pub fn is_unprecedented(&self, axis: &A, value: i64) -> bool {
        self.standing(axis).is_none_or(|mark| value > mark.high)
    }

    /// Whether anyone has ever done this at all, at any magnitude.
    pub fn untouched(&self, axis: &A) -> bool {
        self.standing(axis).is_none()
    }

    /// Notes what was done. Returns whether it took the record.
    pub fn note(&mut self, axis: A, value: i64, holder: H) -> bool {
        let took = self.is_unprecedented(&axis, value);
        match self.marks.get_mut(&axis) {
            Some(mark) => match value.cmp(&mark.high) {
                Ordering::Greater => {
                    mark.high = value;
                    mark.holders.clear();
                    mark.holders.insert(holder);
                },
                Ordering::Equal => {
                    mark.holders.insert(holder);
                },
                Ordering::Less => {},
            },
            None => {
                self.marks.insert(axis, Mark::new(value, holder));
            },
        }
        took
    }

    /// Axes anyone has reached, in a deterministic order.
    pub fn axes(&self) -> impl Iterator<Item = &A> {
        self.marks.keys()
    }

    /// How many axes have a standing mark.
    pub fn filled(&self) -> usize {
        self.marks.len()
    }
}

impl<A: Ord + Clone, H: Ord + Clone> Record<A, H> {
    /// Joins another record into this one, axis by axis.
    ///
    /// Order-independent and repeatable, so records from peers fold in as
    /// they arrive without sequencing them.
    pub fn merge(&mut self, other: &Record<A, H>) {
        for (axis, mark) in &other.marks {
            match self.marks.get_mut(axis) {
                Some(mine) => mine.join(mark),
                None => {
                    self.marks.insert(axis.clone(), mark.clone());
                },
            }
        }
    }
}

#[cfg(test)]
#[path = "record_tests.rs"]
mod tests;
