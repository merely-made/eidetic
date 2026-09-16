// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The feat rule: a feat beats a mark that stood before the reckoning.
//!
//! A first mark on an empty axis is never a feat — there was nothing to
//! beat. Two lineages that both beat one older mark at the same reckoning
//! are both feats, in whichever order they are noted: `feat` is judged
//! against the record as it stood *before any of this reckoning's entries*,
//! not against each other.

use crate::record::Record;

/// One thing done: what axis, how far, and by whom.
pub struct Entry<A, H> {
    pub axis: A,
    pub value: i64,
    pub holder: H,
}

/// What noting one entry decided.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Judgement {
    /// Whether this entry took the record, in sequence with the rest of the
    /// batch — the same answer `note` would give, called in order.
    pub took: bool,
    /// Whether this entry beat a mark that already stood before the batch.
    pub feat: bool,
}

impl<A: Ord + Clone, H: Ord + Clone> Record<A, H> {
    /// Notes every entry in order, returning one judgement per entry.
    pub fn reckon(&mut self, entries: &[Entry<A, H>]) -> Vec<Judgement> {
        // `feat` is decided in a first pass, against the record as it stood
        // before any entry in this batch was noted, so it cannot depend on
        // the order entries are noted in.
        let feats: Vec<bool> = entries
            .iter()
            .map(|entry| {
                self.standing(&entry.axis)
                    .is_some_and(|mark| entry.value > mark.high)
            })
            .collect();

        entries
            .iter()
            .zip(feats)
            .map(|(entry, feat)| {
                let took = self.note(entry.axis.clone(), entry.value, entry.holder.clone());
                Judgement { took, feat }
            })
            .collect()
    }
}

#[cfg(test)]
#[path = "reckoning_tests.rs"]
mod tests;
