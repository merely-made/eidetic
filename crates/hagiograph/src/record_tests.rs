// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;

// --- mirrors of Mesocosm's real types, for the byte-compatibility fixture ---
// (`isometry/mesocosm/crates/mesocosm-core/src/record.rs`). Variant order and
// field order must match the real types exactly: postcard's encoding depends
// on both, and the fixture bytes below came from the real `WorldRecord`.

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
enum Feat {
    Growth,
    Predation,
    Symbiosis,
    Endurance,
    Spread,
    Construction,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
enum Scale {
    Local,
    Regional,
    Worldwide,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
struct SpeciesId(u32);

fn mesocosm_fixture() -> Record<(Feat, Scale), SpeciesId> {
    let mut world = Record::new();
    world.note((Feat::Growth, Scale::Local), 130, SpeciesId(4));
    world.note((Feat::Growth, Scale::Local), 130, SpeciesId(2));
    world.note((Feat::Predation, Scale::Worldwide), 2082, SpeciesId(2));
    world.note((Feat::Spread, Scale::Regional), -3, SpeciesId(300));
    world
}

#[test]
fn matches_mesocosms_pinned_bytes_exactly() {
    let bytes = postcard::to_allocvec(&mesocosm_fixture()).unwrap();
    assert_eq!(
        bytes,
        [
            3, 0, 0, 132, 2, 2, 2, 4, 1, 2, 196, 32, 1, 2, 4, 1, 5, 1, 172, 2
        ],
        "these bytes come from the real WorldRecord; the implementation must fit them, not the reverse"
    );
}

#[test]
fn an_empty_record_encodes_as_a_single_zero_byte() {
    let empty: Record<(Feat, Scale), SpeciesId> = Record::new();
    let bytes = postcard::to_allocvec(&empty).unwrap();
    assert_eq!(bytes, [0]);
}

#[test]
fn the_fixture_round_trips() {
    let world = mesocosm_fixture();
    let bytes = postcard::to_allocvec(&world).unwrap();
    let decoded: Record<(Feat, Scale), SpeciesId> = postcard::from_bytes(&bytes).unwrap();
    assert_eq!(decoded, world);
}

// --- note ---

#[test]
fn note_takes_the_record_on_an_empty_axis_and_on_a_strict_beat() {
    let mut record: Record<&str, u32> = Record::new();
    assert!(record.note("growth", 10, 1), "first mark on an empty axis");
    assert!(record.note("growth", 20, 2), "a strict beat of 10");
}

#[test]
fn note_does_not_take_the_record_on_a_tie_or_below() {
    let mut record: Record<&str, u32> = Record::new();
    record.note("growth", 10, 1);

    assert!(!record.note("growth", 10, 2), "a tie is not a take");
    assert!(!record.note("growth", 5, 3), "below standing is not a take");
}

#[test]
fn a_higher_mark_replaces_the_holders() {
    let mut record: Record<&str, u32> = Record::new();
    record.note("growth", 10, 1);
    record.note("growth", 20, 2);

    let mark = record.standing(&"growth").unwrap();
    assert_eq!(mark.high, 20);
    assert_eq!(mark.holders, BTreeSet::from([2]), "the old holder is gone");
}

#[test]
fn a_tie_unions_the_holders() {
    let mut record: Record<&str, u32> = Record::new();
    record.note("growth", 10, 1);
    record.note("growth", 10, 2);

    let mark = record.standing(&"growth").unwrap();
    assert_eq!(mark.high, 10);
    assert_eq!(mark.holders, BTreeSet::from([1, 2]));
}

#[test]
fn standing_and_untouched_agree_with_note() {
    let mut record: Record<&str, u32> = Record::new();
    assert!(record.untouched(&"growth"));
    assert!(record.is_unprecedented(&"growth", 1));

    record.note("growth", 10, 1);
    assert!(!record.untouched(&"growth"));
    assert!(
        !record.is_unprecedented(&"growth", 10),
        "matching is not beating"
    );
    assert!(record.is_unprecedented(&"growth", 11));
    assert_eq!(record.filled(), 1);
    assert_eq!(record.axes().collect::<Vec<_>>(), vec![&"growth"]);
}

// --- merge: the three laws that make it safe without a protocol ---

fn record_from(entries: &[(&'static str, i64, u32)]) -> Record<&'static str, u32> {
    let mut record = Record::new();
    for (axis, value, holder) in entries {
        record.note(*axis, *value, *holder);
    }
    record
}

#[test]
fn merge_is_commutative() {
    let left = record_from(&[("growth", 10, 1), ("spread", 5, 1)]);
    let right = record_from(&[("growth", 30, 2)]);

    let mut a = left.clone();
    a.merge(&right);
    let mut b = right.clone();
    b.merge(&left);

    assert_eq!(a, b, "which side merged first cannot matter");
}

#[test]
fn merge_is_associative() {
    let x = record_from(&[("growth", 10, 1)]);
    let y = record_from(&[("growth", 30, 2)]);
    let z = record_from(&[("growth", 20, 3), ("endurance", 7, 3)]);

    let mut left = x.clone();
    left.merge(&y);
    left.merge(&z);

    let mut yz = y.clone();
    yz.merge(&z);
    let mut right = x.clone();
    right.merge(&yz);

    assert_eq!(left, right, "grouping cannot matter either");
}

#[test]
fn merge_is_idempotent() {
    let mine = record_from(&[("growth", 10, 1)]);
    let theirs = record_from(&[("growth", 30, 2)]);

    let mut once = mine.clone();
    once.merge(&theirs);

    let mut twice = once.clone();
    twice.merge(&theirs);
    twice.merge(&theirs);

    assert_eq!(once, twice, "merging again changes nothing");
}

#[test]
fn merge_keeps_the_better_mark_and_unions_a_tie() {
    let mut old = record_from(&[("growth", 40, 1), ("spread", 12, 1)]);
    let new = record_from(&[("growth", 60, 2), ("spread", 12, 3), ("construction", 3, 2)]);

    old.merge(&new);

    let growth = old.standing(&"growth").unwrap();
    assert_eq!(
        (growth.high, growth.holders.len()),
        (60, 1),
        "the better mark wins outright"
    );

    let spread = old.standing(&"spread").unwrap();
    assert_eq!(
        spread.holders,
        BTreeSet::from([1, 3]),
        "an equal mark keeps both names"
    );

    assert!(
        !old.untouched(&"construction"),
        "and axes only the other side had arrive"
    );
}
