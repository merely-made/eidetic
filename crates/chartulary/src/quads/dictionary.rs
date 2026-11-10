// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The term dictionary: terms to dense ids and back.

use std::collections::HashMap;
use std::hash::Hash;

use super::TermId;

/// An append-only map from terms to dense [`TermId`]s and back.
///
/// Ids are minted in interning order from 1; [`TermId::DEFAULT_GRAPH`] is
/// never minted. A dictionary only grows: a new revision builds a new one.
#[derive(Clone, Debug)]
pub struct Dictionary<T: Hash + Eq + Clone> {
    terms: Vec<T>,
    ids: HashMap<T, TermId>,
}

impl<T: Hash + Eq + Clone> Default for Dictionary<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Hash + Eq + Clone> Dictionary<T> {
    /// An empty dictionary.
    #[must_use]
    pub fn new() -> Self {
        Dictionary {
            terms: Vec::new(),
            ids: HashMap::new(),
        }
    }

    /// An empty dictionary with room for `capacity` terms.
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Dictionary {
            terms: Vec::with_capacity(capacity),
            ids: HashMap::with_capacity(capacity),
        }
    }

    /// The id of `term`, minting one if the term is new.
    ///
    /// # Panics
    ///
    /// If the dictionary already holds `u32::MAX - 1` terms, the most a
    /// 32-bit id can name beside the reserved default graph.
    pub fn intern(&mut self, term: T) -> TermId {
        if let Some(&id) = self.ids.get(&term) {
            return id;
        }
        let raw = u32::try_from(self.terms.len() + 1)
            .ok()
            .filter(|&raw| raw < u32::MAX)
            .expect("dictionary full: u32::MAX - 1 terms");
        let id = TermId::from_u32(raw).expect("raw id is at least 1");
        self.terms.push(term.clone());
        self.ids.insert(term, id);
        id
    }

    /// The id of `term`, if it has been interned.
    #[must_use]
    pub fn lookup(&self, term: &T) -> Option<TermId> {
        self.ids.get(term).copied()
    }

    /// The term behind `id`, or `None` for an id this dictionary did not mint
    /// (the default graph among them).
    #[must_use]
    pub fn get(&self, id: TermId) -> Option<&T> {
        self.terms.get(id.as_u32() as usize - 1)
    }

    /// The term behind `id`.
    ///
    /// # Panics
    ///
    /// If this dictionary did not mint `id`; [`TermId::DEFAULT_GRAPH`] is never
    /// minted, so resolving it panics.
    #[must_use]
    pub fn resolve(&self, id: TermId) -> &T {
        self.get(id)
            .unwrap_or_else(|| panic!("term id {} was not minted here", id.as_u32()))
    }

    /// The number of interned terms.
    #[must_use]
    pub fn len(&self) -> usize {
        self.terms.len()
    }

    /// Whether no term has been interned.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.terms.is_empty()
    }
}
