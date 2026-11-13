// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use std::collections::{BTreeMap, HashSet};

use super::{Dictionary, Quad, QuadIndex, QuadMeta, TermId, View};

pub(super) fn id(raw: u32) -> TermId {
    TermId::from_u32(raw).expect("non-zero")
}

/// Named graph ids in the generated data; the last holds only withdrawn rows,
/// like Mere's history graph.
pub(super) const NAMED: [u32; 4] = [100_001, 100_002, 100_003, 100_004];
const WITHDRAWN_ONLY: u32 = 100_004;
/// An id no generated row uses.
pub(super) const ABSENT: u32 = 999_999;

pub(super) struct Rng(pub(super) u64);

impl Rng {
    pub(super) fn next(&mut self, below: u32) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        u32::try_from((self.0 >> 32) % u64::from(below)).expect("below a u32")
    }
}

/// `count` rows over `nodes` subjects, eight predicates, objects that are
/// nodes or literals, the default graph and four named graphs. Every seventh
/// row repeats exactly; every eleventh repeats its quad with another meta.
pub(super) fn dataset(count: usize, nodes: u32) -> Vec<(Quad, QuadMeta)> {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let mut rows = Vec::with_capacity(count);
    while rows.len() < count {
        let graph = match rng.next(10) {
            0..4 => TermId::DEFAULT_GRAPH,
            pick => id(NAMED[(pick % 4) as usize]),
        };
        let object = if rng.next(2) == 0 {
            1 + rng.next(nodes)
        } else {
            200_000 + rng.next(nodes * 2)
        };
        let quad = Quad {
            subject: id(1 + rng.next(nodes)),
            predicate: id(10_000_001 + rng.next(8)),
            object: id(object),
            graph,
        };
        let meta = QuadMeta {
            held: graph != id(WITHDRAWN_ONLY) && rng.next(5) != 0,
            asserter: [None, Some(id(300_001)), Some(id(300_002))][rng.next(3) as usize],
            statement: [None, Some(id(400_000 + rng.next(9) + 1))][rng.next(2) as usize],
        };
        rows.push((quad, meta));
        if rows.len() % 7 == 0 {
            rows.push((quad, meta));
        }
        if rows.len() % 11 == 0 {
            let other = QuadMeta {
                held: graph != id(WITHDRAWN_ONLY) && !meta.held,
                asserter: Some(id(300_003)),
                statement: None,
            };
            rows.push((quad, other));
        }
    }
    rows
}

pub(super) type Pattern = (
    Option<TermId>,
    Option<TermId>,
    Option<TermId>,
    Option<Option<TermId>>,
);

/// The pattern binding the positions in `mask` (bit 0 subject, 1 predicate,
/// 2 object, 3 graph) to `quad`'s values.
pub(super) fn bind(mask: u32, quad: &Quad) -> Pattern {
    let graph = (!quad.graph.is_default_graph()).then_some(quad.graph);
    (
        (mask & 1 != 0).then_some(quad.subject),
        (mask & 2 != 0).then_some(quad.predicate),
        (mask & 4 != 0).then_some(quad.object),
        (mask & 8 != 0).then_some(graph),
    )
}

/// What a pattern should yield, by brute force: each matching quad once,
/// with its first visible meta in held, asserter, statement order.
fn brute_force(rows: &[(Quad, QuadMeta)], pattern: Pattern, view: View) -> Vec<(Quad, QuadMeta)> {
    let (s, p, o, g) = pattern;
    let g = g.map(|graph| graph.unwrap_or(TermId::DEFAULT_GRAPH));
    let mut best: BTreeMap<Quad, QuadMeta> = BTreeMap::new();
    for (quad, meta) in rows {
        let matches = s.is_none_or(|s| s == quad.subject)
            && p.is_none_or(|p| p == quad.predicate)
            && o.is_none_or(|o| o == quad.object)
            && g.is_none_or(|g| g == quad.graph);
        if !matches || !view.admits(meta) {
            continue;
        }
        best.entry(*quad)
            .and_modify(|kept| {
                if meta.rank() < kept.rank() {
                    *kept = *meta;
                }
            })
            .or_insert(*meta);
    }
    best.into_iter().collect()
}

fn read(index: &QuadIndex, pattern: Pattern, view: View) -> Vec<(Quad, QuadMeta)> {
    let (s, p, o, g) = pattern;
    let mut got: Vec<(Quad, QuadMeta)> = index
        .pattern(s, p, o, g, view)
        .map(|(quad, meta)| (*quad, *meta))
        .collect();
    let distinct: HashSet<Quad> = got.iter().map(|(quad, _)| *quad).collect();
    assert_eq!(distinct.len(), got.len(), "a quad was yielded twice");
    got.sort_by_key(|(quad, _)| *quad);
    got
}

/// Every one of the sixteen bound shapes, over probes taken from the data and
/// one absent id, against the brute-force filter.
fn sixteen_shapes_match(view: View) {
    let rows = dataset(3_000, 40);
    let index = QuadIndex::build(rows.iter().copied());
    let absent = Quad {
        subject: id(ABSENT),
        predicate: id(ABSENT),
        object: id(ABSENT),
        graph: id(ABSENT),
    };
    let mut probes: Vec<Quad> = rows.iter().step_by(97).map(|(quad, _)| *quad).collect();
    probes.push(absent);
    assert!(probes.iter().any(|quad| quad.graph.is_default_graph()));
    let mut nonempty = 0;
    for mask in 0..16 {
        for probe in &probes {
            let pattern = bind(mask, probe);
            let expected = brute_force(&rows, pattern, view);
            assert_eq!(
                read(&index, pattern, view),
                expected,
                "shape {mask:04b} {view:?}"
            );
            nonempty += usize::from(!expected.is_empty());
        }
    }
    assert!(nonempty > 16 * 20, "probes mostly hit: {nonempty}");
}

#[test]
fn sixteen_shapes_match_brute_force_live() {
    sixteen_shapes_match(View::Live);
}

#[test]
fn sixteen_shapes_match_brute_force_history() {
    sixteen_shapes_match(View::History);
}

#[test]
fn the_default_graph_is_some_none() {
    let rows = dataset(3_000, 40);
    let index = QuadIndex::build(rows.iter().copied());
    let default: Vec<_> = index
        .pattern(None, None, None, Some(None), View::History)
        .collect();
    assert!(!default.is_empty());
    assert!(
        default
            .iter()
            .all(|(quad, _)| quad.graph.is_default_graph())
    );
    let explicit = index.pattern(
        None,
        None,
        None,
        Some(Some(TermId::DEFAULT_GRAPH)),
        View::History,
    );
    assert_eq!(explicit.count(), default.len());
    let named = index.pattern(None, None, None, Some(Some(id(NAMED[0]))), View::History);
    assert!(
        named
            .into_iter()
            .all(|(quad, _)| quad.graph == id(NAMED[0]))
    );
}

#[test]
fn identical_rows_collapse_and_differing_metas_stay() {
    let quad = Quad::in_default_graph(id(1), id(2), id(3));
    let withdrawn = QuadMeta {
        held: false,
        asserter: Some(id(7)),
        statement: Some(id(9)),
    };
    let by_eight = QuadMeta {
        asserter: Some(id(8)),
        ..QuadMeta::HELD
    };
    let rows = [
        (quad, withdrawn),
        (quad, by_eight),
        (quad, withdrawn),
        (quad, QuadMeta::HELD),
    ];
    let index = QuadIndex::build(rows);
    assert_eq!(index.len(), 3);

    let live: Vec<_> = index
        .pattern(Some(id(1)), None, None, None, View::Live)
        .collect();
    assert_eq!(
        live,
        [(&quad, &QuadMeta::HELD)],
        "held first, None asserter first"
    );
    let history: Vec<_> = index
        .pattern(None, None, Some(id(3)), None, View::History)
        .collect();
    assert_eq!(history, [(&quad, &QuadMeta::HELD)]);

    let only_withdrawn = QuadIndex::build([(quad, withdrawn)]);
    assert_eq!(
        only_withdrawn
            .pattern(None, None, None, None, View::Live)
            .count(),
        0
    );
    let seen: Vec<_> = only_withdrawn
        .pattern(None, None, None, None, View::History)
        .collect();
    assert_eq!(seen, [(&quad, &withdrawn)]);
}

#[test]
fn graph_names_follow_the_view() {
    let rows = dataset(3_000, 40);
    let index = QuadIndex::build(rows.iter().copied());
    let history: Vec<TermId> = index.graph_names(View::History).collect();
    assert_eq!(history, NAMED.map(id));
    let live: Vec<TermId> = index.graph_names(View::Live).collect();
    assert_eq!(
        live,
        NAMED[..3].iter().map(|&raw| id(raw)).collect::<Vec<_>>()
    );

    assert!(index.contains_graph(id(NAMED[0]), View::Live));
    assert!(index.contains_graph(id(WITHDRAWN_ONLY), View::History));
    assert!(!index.contains_graph(id(WITHDRAWN_ONLY), View::Live));
    assert!(!index.contains_graph(id(ABSENT), View::History));
    assert!(!index.contains_graph(TermId::DEFAULT_GRAPH, View::History));
}

#[test]
fn an_empty_index_reads_nothing() {
    let index = QuadIndex::build([]);
    assert!(index.is_empty());
    assert_eq!(
        index.pattern(None, None, None, None, View::History).count(),
        0
    );
    assert_eq!(
        index
            .pattern(Some(id(1)), None, None, Some(None), View::Live)
            .count(),
        0
    );
    assert_eq!(index.graph_names(View::History).count(), 0);
}

#[test]
fn the_dictionary_interns_once_and_never_mints_the_default_graph() {
    let mut dictionary = Dictionary::new();
    let a = dictionary.intern("a");
    let b = dictionary.intern("b");
    assert_eq!(dictionary.intern("a"), a);
    assert_eq!((a.as_u32(), b.as_u32()), (1, 2));
    assert_eq!(dictionary.len(), 2);
    assert_eq!(dictionary.lookup(&"b"), Some(b));
    assert_eq!(dictionary.lookup(&"c"), None);
    assert_eq!(*dictionary.resolve(a), "a");
    assert_eq!(dictionary.get(TermId::DEFAULT_GRAPH), None);
    assert_eq!(dictionary.get(id(3)), None);
    assert_eq!(TermId::from_u32(0), None);
    assert_eq!(size_of::<Option<TermId>>(), 4);
    assert_eq!(size_of::<QuadMeta>(), 12);
}

#[test]
#[should_panic(expected = "not minted")]
fn resolving_the_default_graph_panics() {
    let mut dictionary = Dictionary::new();
    dictionary.intern(1_u8);
    let _ = dictionary.resolve(TermId::DEFAULT_GRAPH);
}

#[test]
fn eighty_thousand_quads_fit_the_budget() {
    let mut seen = HashSet::new();
    let rows: Vec<(Quad, QuadMeta)> = dataset(100_000, 10_000)
        .into_iter()
        .filter(|row| seen.insert(*row))
        .take(80_000)
        .collect();
    let index = QuadIndex::build(rows);
    assert_eq!(index.len(), 80_000);
    let bytes = index.memory_bytes();
    println!("quads::memory_bytes at {} rows: {bytes}", index.len());
    assert!(
        bytes < 10_000_000,
        "brief §4: under 10 MB at 80k quads, got {bytes}"
    );
}
