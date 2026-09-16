// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;

fn entry(axis: &'static str, value: i64, holder: u32) -> Entry<&'static str, u32> {
    Entry {
        axis,
        value,
        holder,
    }
}

#[test]
fn an_entry_on_an_empty_axis_takes_but_is_not_a_feat() {
    let mut record: Record<&str, u32> = Record::new();
    let judgements = record.reckon(&[entry("growth", 10, 1)]);

    assert_eq!(
        judgements,
        vec![Judgement {
            took: true,
            feat: false
        }]
    );
}

#[test]
fn a_strict_beat_of_an_older_mark_is_a_feat() {
    let mut record: Record<&str, u32> = Record::new();
    record.note("growth", 10, 1);

    let judgements = record.reckon(&[entry("growth", 20, 2)]);
    assert_eq!(
        judgements,
        vec![Judgement {
            took: true,
            feat: true
        }]
    );
}

#[test]
fn two_beats_of_one_older_mark_are_both_feats_in_either_order() {
    let base: Record<&str, u32> = {
        let mut record = Record::new();
        record.note("growth", 10, 1);
        record
    };
    let first_then_second = [entry("growth", 20, 2), entry("growth", 30, 3)];
    let second_then_first = [entry("growth", 30, 3), entry("growth", 20, 2)];

    let mut a = base.clone();
    let judged_a = a.reckon(&first_then_second);
    let mut b = base.clone();
    let judged_b = b.reckon(&second_then_first);

    assert!(
        judged_a.iter().all(|j| j.feat),
        "both beat the mark that stood before this reckoning"
    );
    assert!(
        judged_b.iter().all(|j| j.feat),
        "order must not change that"
    );

    // `took` is the sequential-note answer, so order does change it: noted
    // second, 20 no longer takes the record once 30 already has.
    assert_eq!(
        judged_a.iter().map(|j| j.took).collect::<Vec<_>>(),
        vec![true, true],
        "20 then 30: each beats what stood at the time it was noted"
    );
    assert_eq!(
        judged_b.iter().map(|j| j.took).collect::<Vec<_>>(),
        vec![true, false],
        "30 then 20: 20 no longer beats the record 30 just set"
    );
}

#[test]
fn took_matches_sequential_note_over_a_mixed_batch() {
    let entries = [
        entry("growth", 10, 1),
        entry("growth", 10, 2), // ties the first
        entry("growth", 5, 3),  // below standing
        entry("spread", 1, 1),  // a different, empty axis
        entry("growth", 20, 4), // beats the tie
    ];

    let mut via_reckon: Record<&str, u32> = Record::new();
    let judgements = via_reckon.reckon(&entries);

    let mut via_note: Record<&str, u32> = Record::new();
    let sequential: Vec<bool> = entries
        .iter()
        .map(|e| via_note.note(e.axis, e.value, e.holder))
        .collect();

    assert_eq!(
        judgements.iter().map(|j| j.took).collect::<Vec<_>>(),
        sequential
    );
    assert_eq!(via_reckon, via_note, "and the resulting records agree too");
}
