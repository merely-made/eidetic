// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The quad index: one base table and six orderings of positions into it.

use std::collections::HashSet;
use std::mem::size_of;
use std::ops::Range;

use super::{Quad, QuadMeta, TermId, View};

/// One of the six sort orders. The first four letters of each name are the
/// order in which a quad's positions are compared.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Order {
    Spog,
    Posg,
    Ospg,
    Gspo,
    Gpos,
    Gosp,
}

/// The six orders, in the slots of [`QuadIndex::ordinals`].
const ORDERS: [Order; 6] = [
    Order::Spog,
    Order::Posg,
    Order::Ospg,
    Order::Gspo,
    Order::Gpos,
    Order::Gosp,
];

impl Order {
    fn key(self, quad: &Quad) -> [u32; 4] {
        let (s, p, o, g) = (
            quad.subject.as_u32(),
            quad.predicate.as_u32(),
            quad.object.as_u32(),
            quad.graph.as_u32(),
        );
        match self {
            Order::Spog => [s, p, o, g],
            Order::Posg => [p, o, s, g],
            Order::Ospg => [o, s, p, g],
            Order::Gspo => [g, s, p, o],
            Order::Gpos => [g, p, o, s],
            Order::Gosp => [g, o, s, p],
        }
    }

    /// The order whose key prefix the bound positions fill exactly, so the
    /// matching rows are one contiguous range of it.
    fn for_bound(s: bool, p: bool, o: bool, g: bool) -> Order {
        match (s, p, o, g) {
            (_, false, false, false) | (true, true, _, false) | (true, true, true, true) => {
                Order::Spog
            },
            (false, true, _, false) => Order::Posg,
            (_, false, true, false) => Order::Ospg,
            (_, false, false, true) | (true, true, false, true) => Order::Gspo,
            (false, true, _, true) => Order::Gpos,
            (_, false, true, true) => Order::Gosp,
        }
    }

    fn slot(self) -> usize {
        self as usize
    }
}

/// A named graph's row counts, for the graph listings of either view.
#[derive(Clone, Copy, Debug)]
struct GraphRows {
    graph: TermId,
    held: u32,
    rows: u32,
}

/// Quads indexed six ways for range reads by any bound pattern.
///
/// Built from `(quad, meta)` rows, then either read until the next rebuild or
/// maintained in place by [`apply`](QuadIndex::apply). Identical rows collapse;
/// rows of the same quad with different metas are all kept, and a
/// [`pattern`](QuadIndex::pattern) read yields each quad once per view, its
/// visible row first in the order held, asserter, statement.
#[derive(Clone, Debug, Default)]
pub struct QuadIndex {
    /// Every row ever stored. Sorted SPO then graph at build; applied rows are
    /// appended, so positions never move.
    rows: Vec<(Quad, QuadMeta)>,
    /// Positions into `rows` for each of [`ORDERS`]. The SPOG slot stays empty
    /// while `base_sorted`.
    ordinals: [Vec<u32>; 6],
    /// Whether `rows` is itself in SPOG order: true from build to the first
    /// apply.
    base_sorted: bool,
    /// Each named graph with its held and total live row counts, by id.
    graphs: Vec<GraphRows>,
    /// Rows removed, or dropped as duplicates by a withdrawal, as a bitset.
    dead: Vec<u64>,
    dead_count: usize,
}

impl QuadIndex {
    /// Index the rows: collapse identical `(quad, meta)` rows, then sort the
    /// orderings.
    ///
    /// # Panics
    ///
    /// If more than `u32::MAX` distinct rows are given; positions are 32-bit.
    pub fn build(rows: impl IntoIterator<Item = (Quad, QuadMeta)>) -> QuadIndex {
        let mut rows: Vec<(Quad, QuadMeta)> = rows.into_iter().collect();
        rows.sort_unstable_by_key(|(quad, meta)| (Order::Spog.key(quad), meta.rank()));
        rows.dedup();
        rows.shrink_to_fit();
        let count = u32::try_from(rows.len()).expect("at most u32::MAX quad rows");

        let ordinals = ORDERS.map(|order| {
            if order == Order::Spog {
                return Vec::new();
            }
            let mut positions: Vec<u32> = (0..count).collect();
            positions.sort_unstable_by_key(|&at| (order.key(&rows[at as usize].0), at));
            positions
        });

        let mut graphs: Vec<GraphRows> = Vec::new();
        for &at in &ordinals[Order::Gspo.slot()] {
            let (quad, meta) = &rows[at as usize];
            if quad.graph.is_default_graph() {
                continue;
            }
            match graphs.last_mut() {
                Some(entry) if entry.graph == quad.graph => {
                    entry.held += u32::from(meta.held);
                    entry.rows += 1;
                },
                _ => graphs.push(GraphRows {
                    graph: quad.graph,
                    held: u32::from(meta.held),
                    rows: 1,
                }),
            }
        }
        graphs.shrink_to_fit();

        QuadIndex {
            rows,
            ordinals,
            base_sorted: true,
            graphs,
            dead: Vec::new(),
            dead_count: 0,
        }
    }

    /// Apply one batch of changes in place: every removal, then every asserted
    /// row, then every withdrawal.
    ///
    /// A removed row (matched exactly, `held` included) is dropped: a
    /// tombstone every read skips, until a rebuild compacts it (see
    /// [`tombstones`](QuadIndex::tombstones)); an absent one is ignored.
    /// An asserted row already present is ignored; a new one is appended and
    /// entered in each ordering at its sorted place. A withdrawal names a row
    /// by quad, asserter and statement (its `held` is ignored): the held row
    /// becomes withdrawn, or is dropped if that withdrawn row already exists;
    /// with no held row, the withdrawn row is added. The result reads the same
    /// as [`QuadIndex::build`] over the rows that remain.
    ///
    /// # Panics
    ///
    /// If the index would hold more than `u32::MAX` rows.
    pub fn apply(
        &mut self,
        asserted: impl IntoIterator<Item = (Quad, QuadMeta)>,
        withdrawn: impl IntoIterator<Item = (Quad, QuadMeta)>,
        removed: impl IntoIterator<Item = (Quad, QuadMeta)>,
    ) {
        if self.base_sorted {
            let count = u32::try_from(self.rows.len()).expect("at most u32::MAX quad rows");
            self.ordinals[Order::Spog.slot()] = (0..count).collect();
            self.base_sorted = false;
        }
        for (quad, meta) in removed {
            if let Some(at) = self.find(&quad, &meta) {
                self.kill(at);
                self.count_graph(quad.graph, -i32::from(meta.held), -1);
            }
        }
        self.insert(asserted.into_iter().collect());

        let mut absent = Vec::new();
        for (quad, meta) in withdrawn {
            let held = QuadMeta { held: true, ..meta };
            let gone = QuadMeta { held: false, ..meta };
            match (self.find(&quad, &held), self.find(&quad, &gone)) {
                (Some(at), Some(_)) => {
                    self.kill(at);
                    self.count_graph(quad.graph, -1, -1);
                },
                (Some(at), None) => {
                    self.rows[at].1.held = false;
                    self.count_graph(quad.graph, -1, 0);
                },
                (None, Some(_)) => {},
                (None, None) => absent.push((quad, gone)),
            }
        }
        self.insert(absent);
    }

    /// The rows matching a pattern, each quad once, as `(quad, meta)`.
    ///
    /// `None` leaves a position free. For the graph, `Some(None)` is the
    /// default graph and `Some(Some(id))` a named one. The read picks the
    /// ordering whose prefix the bound positions fill and walks one range of
    /// it found by binary search; `View::Live` skips withdrawn rows.
    pub fn pattern(
        &self,
        subject: Option<TermId>,
        predicate: Option<TermId>,
        object: Option<TermId>,
        graph: Option<Option<TermId>>,
        view: View,
    ) -> impl Iterator<Item = (&Quad, &QuadMeta)> {
        let graph = graph.map(|graph| graph.unwrap_or(TermId::DEFAULT_GRAPH));
        let order = Order::for_bound(
            subject.is_some(),
            predicate.is_some(),
            object.is_some(),
            graph.is_some(),
        );
        // Free positions take any value: only the bound prefix is compared.
        let probe = Quad {
            subject: subject.unwrap_or(TermId::DEFAULT_GRAPH),
            predicate: predicate.unwrap_or(TermId::DEFAULT_GRAPH),
            object: object.unwrap_or(TermId::DEFAULT_GRAPH),
            graph: graph.unwrap_or(TermId::DEFAULT_GRAPH),
        };
        let bound = [
            subject.is_some(),
            predicate.is_some(),
            object.is_some(),
            graph.is_some(),
        ]
        .iter()
        .filter(|&&bound| bound)
        .count();
        let range = self.range(order, &order.key(&probe), bound);

        let mut rows = range
            .map(move |at| self.base_position(order, at))
            .filter(move |&at| !self.is_dead(at))
            .map(move |at| {
                let (quad, meta) = &self.rows[at];
                (quad, meta)
            })
            .peekable();
        // Rows of one quad are adjacent in every ordering; the visible one of
        // least rank stands for the quad.
        std::iter::from_fn(move || {
            let mut best = loop {
                let (quad, meta) = rows.next()?;
                if view.admits(meta) {
                    break (quad, meta);
                }
            };
            while let Some(&(quad, meta)) = rows.peek() {
                if quad != best.0 {
                    break;
                }
                rows.next();
                if view.admits(meta) && meta.rank() < best.1.rank() {
                    best = (quad, meta);
                }
            }
            Some(best)
        })
    }

    /// The named graphs with at least one row visible in `view`, by id. The
    /// default graph is not a named graph and is never listed.
    pub fn graph_names(&self, view: View) -> impl Iterator<Item = TermId> {
        self.graphs
            .iter()
            .filter(move |entry| entry.visible(view))
            .map(|entry| entry.graph)
    }

    /// Whether `graph` is a named graph with at least one row visible in
    /// `view`. Always `false` for [`TermId::DEFAULT_GRAPH`].
    #[must_use]
    pub fn contains_graph(&self, graph: TermId, view: View) -> bool {
        self.graphs
            .binary_search_by_key(&graph, |entry| entry.graph)
            .is_ok_and(|at| self.graphs[at].visible(view))
    }

    /// The number of stored rows: distinct `(quad, meta)` pairs, so a quad
    /// carried by several metas counts once per meta.
    #[must_use]
    pub fn len(&self) -> usize {
        self.rows.len() - self.dead_count
    }

    /// Whether the exact row `(quad, meta)` is stored and not removed.
    #[must_use]
    pub fn contains(&self, quad: &Quad, meta: &QuadMeta) -> bool {
        self.find(quad, meta).is_some()
    }

    /// Rows dropped in place and still occupying space; a caller rebuilds
    /// with [`QuadIndex::build`] when it judges them too many.
    #[must_use]
    pub fn tombstones(&self) -> usize {
        self.dead_count
    }

    /// Whether the index holds no rows.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Bytes held by the index: the struct, the base table, the orderings, the
    /// graph list and the dropped-row bitset, by allocated capacity.
    #[must_use]
    pub fn memory_bytes(&self) -> usize {
        size_of::<Self>()
            + self.rows.capacity() * size_of::<(Quad, QuadMeta)>()
            + self
                .ordinals
                .iter()
                .map(|positions| positions.capacity() * size_of::<u32>())
                .sum::<usize>()
            + self.graphs.capacity() * size_of::<GraphRows>()
            + self.dead.capacity() * size_of::<u64>()
    }

    /// Append the rows not already present and enter each in every ordering.
    fn insert(&mut self, rows: Vec<(Quad, QuadMeta)>) {
        let mut fresh = HashSet::new();
        let start = self.rows.len();
        grow(&mut self.rows, rows.len());
        for (quad, meta) in rows {
            if self.find(&quad, &meta).is_some() || !fresh.insert((quad, meta)) {
                continue;
            }
            self.rows.push((quad, meta));
            self.count_graph(quad.graph, i32::from(meta.held), 1);
        }
        let end = u32::try_from(self.rows.len()).expect("at most u32::MAX quad rows");
        let start = u32::try_from(start).expect("at most u32::MAX quad rows");
        if start == end {
            return;
        }
        for order in ORDERS {
            let rows = &self.rows;
            let key = |at: u32| order.key(&rows[at as usize].0);
            let positions = &mut self.ordinals[order.slot()];
            // Each new row goes after the rows sharing its key.
            let mut places: Vec<(usize, u32)> = (start..end)
                .map(|at| (positions.partition_point(|&old| key(old) <= key(at)), at))
                .collect();
            places.sort_unstable_by_key(|&(place, at)| (place, key(at)));
            let old = positions.len();
            grow(positions, places.len());
            positions.resize(old + places.len(), 0);
            // One backward pass: shift each stretch once, by the count of new
            // rows that sort before it.
            let mut stretch_end = old;
            for (shift, &(place, at)) in places.iter().enumerate().rev() {
                positions.copy_within(place..stretch_end, place + shift + 1);
                positions[place + shift] = at;
                stretch_end = place;
            }
        }
    }

    /// The live position of the row equal to `(quad, meta)`, if any.
    fn find(&self, quad: &Quad, meta: &QuadMeta) -> Option<usize> {
        self.range(Order::Spog, &Order::Spog.key(quad), 4)
            .map(|at| self.base_position(Order::Spog, at))
            .find(|&at| !self.is_dead(at) && self.rows[at] == (*quad, *meta))
    }

    fn count_graph(&mut self, graph: TermId, held: i32, rows: i32) {
        if graph.is_default_graph() {
            return;
        }
        let at = match self.graphs.binary_search_by_key(&graph, |entry| entry.graph) {
            Ok(at) => at,
            Err(at) => {
                self.graphs.insert(
                    at,
                    GraphRows {
                        graph,
                        held: 0,
                        rows: 0,
                    },
                );
                at
            },
        };
        let entry = &mut self.graphs[at];
        entry.held = entry.held.checked_add_signed(held).expect("held count stays >= 0");
        entry.rows = entry.rows.checked_add_signed(rows).expect("row count stays >= 0");
    }

    fn kill(&mut self, at: usize) {
        let words = self.rows.len().div_ceil(64);
        if self.dead.len() < words {
            self.dead.resize(words, 0);
        }
        self.dead[at / 64] |= 1 << (at % 64);
        self.dead_count += 1;
    }

    fn is_dead(&self, at: usize) -> bool {
        self.dead
            .get(at / 64)
            .is_some_and(|word| word >> (at % 64) & 1 == 1)
    }

    /// The base-table position of the `at`th entry of `order`.
    fn base_position(&self, order: Order, at: usize) -> usize {
        match self.positions(order) {
            None => at,
            Some(positions) => positions[at] as usize,
        }
    }

    /// The order's positions, or `None` while it is the base table's own order.
    fn positions(&self, order: Order) -> Option<&[u32]> {
        (order != Order::Spog || !self.base_sorted).then_some(&self.ordinals[order.slot()])
    }

    /// The entries of `order` whose first `bound` key entries equal the
    /// probe's.
    fn range(&self, order: Order, probe: &[u32; 4], bound: usize) -> Range<usize> {
        if bound == 0 {
            return 0..self.rows.len();
        }
        let compare = |quad: &Quad| order.key(quad)[..bound].cmp(&probe[..bound]);
        match self.positions(order) {
            None => {
                let low = self.rows.partition_point(|(quad, _)| compare(quad).is_lt());
                let high = self.rows.partition_point(|(quad, _)| compare(quad).is_le());
                low..high
            },
            Some(positions) => {
                let at = |position: &u32| &self.rows[*position as usize].0;
                let low = positions.partition_point(|position| compare(at(position)).is_lt());
                let high = positions.partition_point(|position| compare(at(position)).is_le());
                low..high
            },
        }
    }
}

/// Room for `additional` more, growing by a sixteenth rather than doubling,
/// so a maintained index stays near its built size.
fn grow<T>(items: &mut Vec<T>, additional: usize) {
    if items.capacity() - items.len() < additional {
        items.reserve_exact(additional.max(items.len() / 16));
    }
}

impl GraphRows {
    fn visible(&self, view: View) -> bool {
        match view {
            View::Live => self.held > 0,
            View::History => self.rows > 0,
        }
    }
}
