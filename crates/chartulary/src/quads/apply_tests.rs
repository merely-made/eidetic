// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! `QuadIndex::apply` against `QuadIndex::build` over the same final rows.

use std::collections::HashSet;

use super::tests::{ABSENT, NAMED, Pattern, Rng, bind, dataset, id};
use super::{Quad, QuadIndex, QuadMeta, TermId, View};

type Row = (Quad, QuadMeta);

/// The rows `apply` should leave: removals dropped, asserts added, then each
/// withdrawal turns its held row (if any) into the withdrawn one.
fn model(rows: &mut HashSet<Row>, asserted: &[Row], withdrawn: &[Row], removed: &[Row]) {
    for row in removed {
        rows.remove(row);
    }
    rows.extend(asserted.iter().copied());
    for (quad, meta) in withdrawn {
        rows.remove(&(*quad, QuadMeta { held: true, ..*meta }));
        rows.insert((*quad, QuadMeta { held: false, ..*meta }));
    }
}

fn pattern(index: &QuadIndex, (s, p, o, g): Pattern, view: View) -> Vec<Row> {
    index
        .pattern(s, p, o, g, view)
        .map(|(quad, meta)| (*quad, *meta))
        .collect()
}

/// Every bound shape over every probe, both views, in yield order; then the
/// graph listings and the row count.
fn assert_same(incremental: &QuadIndex, rows: &HashSet<Row>, probes: &[Quad], context: &str) {
    let oracle = QuadIndex::build(rows.iter().copied());
    for view in [View::Live, View::History] {
        for mask in 0..16 {
            for probe in probes {
                let shape = bind(mask, probe);
                assert_eq!(
                    pattern(incremental, shape, view),
                    pattern(&oracle, shape, view),
                    "{context}: shape {mask:04b} {view:?} probe {probe:?}"
                );
            }
        }
        assert!(
            incremental.graph_names(view).eq(oracle.graph_names(view)),
            "{context}: graph_names {view:?}"
        );
        for graph in NAMED.map(id).into_iter().chain([id(ABSENT), TermId::DEFAULT_GRAPH]) {
            assert_eq!(
                incremental.contains_graph(graph, view),
                oracle.contains_graph(graph, view),
                "{context}: contains_graph {graph:?} {view:?}"
            );
        }
    }
    assert_eq!(incremental.len(), oracle.len(), "{context}: len");
}

/// A row over a small id space, so asserts repeat and withdrawals hit.
fn small_row(rng: &mut Rng) -> Row {
    let graph = match rng.next(4) {
        0 => TermId::DEFAULT_GRAPH,
        pick => id(NAMED[pick as usize - 1]),
    };
    let quad = Quad {
        subject: id(1 + rng.next(4)),
        predicate: id(10 + rng.next(2)),
        object: id(1 + rng.next(4)),
        graph,
    };
    let meta = QuadMeta {
        held: rng.next(4) != 0,
        asserter: [None, Some(id(20)), Some(id(21))][rng.next(3) as usize],
        statement: [None, Some(id(30))][rng.next(2) as usize],
    };
    (quad, meta)
}

fn small_probes() -> Vec<Quad> {
    let mut probes = Vec::new();
    for subject in 1..=4 {
        for (object, graph) in [(1, TermId::DEFAULT_GRAPH), (2, id(NAMED[0])), (4, id(NAMED[2]))] {
            probes.push(Quad {
                subject: id(subject),
                predicate: id(10 + subject % 2),
                object: id(object),
                graph,
            });
        }
    }
    probes.push(Quad {
        subject: id(ABSENT),
        predicate: id(ABSENT),
        object: id(ABSENT),
        graph: id(ABSENT),
    });
    probes
}

/// Random batches of asserts and withdrawals, from an empty index and from a
/// built one, checked against a rebuild after every batch.
#[test]
fn random_edit_sequences_match_a_rebuild() {
    let probes = small_probes();
    for seed in 1..=60_u64 {
        let mut rng = Rng(0x2545_F491_4F6C_DD1D ^ seed.wrapping_mul(0x9E37_79B9));
        let mut rows: HashSet<Row> = HashSet::new();
        let initial: Vec<Row> = (0..rng.next(12)).map(|_| small_row(&mut rng)).collect();
        rows.extend(initial.iter().copied());
        let mut index = if seed % 3 == 0 {
            let mut index = QuadIndex::default();
            index.apply(initial, [], []);
            index
        } else {
            QuadIndex::build(initial)
        };
        assert_same(&index, &rows, &probes, &format!("seed {seed} start"));
        for batch in 0..25 {
            let asserted: Vec<Row> = (0..rng.next(5)).map(|_| small_row(&mut rng)).collect();
            let mut withdrawn: Vec<Row> = (0..rng.next(4)).map(|_| small_row(&mut rng)).collect();
            let mut removed: Vec<Row> = (0..rng.next(3)).map(|_| small_row(&mut rng)).collect();
            // Most withdrawals and removals name a row that is there.
            let present: Vec<Row> = rows.iter().copied().collect();
            if !present.is_empty() {
                for _ in 0..rng.next(4) {
                    withdrawn.push(present[rng.next(present.len() as u32) as usize]);
                }
                for _ in 0..rng.next(4) {
                    removed.push(present[rng.next(present.len() as u32) as usize]);
                }
            }
            model(&mut rows, &asserted, &withdrawn, &removed);
            index.apply(asserted, withdrawn, removed);
            assert_same(&index, &rows, &probes, &format!("seed {seed} batch {batch}"));
        }
    }
}

/// The P1 dataset with duplicates and a withdrawn-only graph, edited in place
/// and read through the sixteen shapes as the P1 tests read it.
#[test]
fn a_large_index_edited_in_place_matches_a_rebuild() {
    let start = dataset(3_000, 40);
    let mut rows: HashSet<Row> = start.iter().copied().collect();
    let mut index = QuadIndex::build(start.iter().copied());
    let more = dataset(3_600, 41);
    let mut rng = Rng(0xD1B5_4A32_D192_ED03);
    for chunk in more[3_000..].chunks(8) {
        let present: Vec<Row> = rows.iter().copied().collect();
        let mut pick = || present[rng.next(present.len() as u32) as usize];
        let withdrawn: Vec<Row> = (0..2).map(|_| pick()).collect();
        let removed: Vec<Row> = (0..3).map(|_| pick()).collect();
        model(&mut rows, chunk, &withdrawn, &removed);
        index.apply(chunk.iter().copied(), withdrawn, removed);
    }
    let mut probes: Vec<Quad> = more.iter().step_by(97).map(|(quad, _)| *quad).collect();
    probes.push(Quad {
        subject: id(ABSENT),
        predicate: id(ABSENT),
        object: id(ABSENT),
        graph: id(ABSENT),
    });
    assert_same(&index, &rows, &probes, "large");
}

#[test]
fn withdrawing_onto_an_existing_withdrawn_row_drops_the_duplicate() {
    let quad = Quad::in_default_graph(id(1), id(2), id(3));
    let gone = QuadMeta {
        held: false,
        ..QuadMeta::HELD
    };
    let mut index = QuadIndex::build([(quad, QuadMeta::HELD), (quad, gone)]);
    assert_eq!(index.len(), 2);
    index.apply([], [(quad, QuadMeta::HELD)], []);
    assert_eq!(index.len(), 1);
    assert_eq!(index.pattern(None, None, None, None, View::Live).count(), 0);
    let seen: Vec<_> = index.pattern(None, None, None, None, View::History).collect();
    assert_eq!(seen, [(&quad, &gone)]);

    // A withdrawn-only named graph leaves the live listing and stays in history.
    let graph = id(NAMED[0]);
    let named = Quad { graph, ..quad };
    index.apply([(named, QuadMeta::HELD)], [], []);
    assert!(index.contains_graph(graph, View::Live));
    index.apply([], [(named, QuadMeta::HELD)], []);
    assert!(!index.contains_graph(graph, View::Live));
    assert!(index.contains_graph(graph, View::History));
}

#[test]
fn removing_rows_tombstones_them_until_a_rebuild() {
    let graph = id(NAMED[1]);
    let a = Quad {
        graph,
        ..Quad::in_default_graph(id(1), id(2), id(3))
    };
    let b = Quad { object: id(4), ..a };
    let mut index = QuadIndex::build([(a, QuadMeta::HELD), (b, QuadMeta::HELD)]);
    // An absent row, or one differing only in `held`, removes nothing.
    let gone = QuadMeta {
        held: false,
        ..QuadMeta::HELD
    };
    index.apply([], [], [(a, gone), (Quad { object: id(9), ..a }, QuadMeta::HELD)]);
    assert_eq!((index.len(), index.tombstones()), (2, 0));

    index.apply([], [], [(a, QuadMeta::HELD)]);
    assert_eq!((index.len(), index.tombstones()), (1, 1));
    assert!(index.contains_graph(graph, View::Live));
    index.apply([], [], [(b, QuadMeta::HELD)]);
    assert!(index.is_empty());
    assert!(!index.contains_graph(graph, View::History));
    assert_eq!(index.pattern(None, None, None, None, View::History).count(), 0);

    // Removed then asserted again in one batch: present.
    index.apply([(a, QuadMeta::HELD)], [], [(a, QuadMeta::HELD)]);
    assert_eq!((index.len(), index.tombstones()), (1, 2));
    assert_eq!(index.pattern(Some(id(1)), None, None, None, View::Live).count(), 1);
}
