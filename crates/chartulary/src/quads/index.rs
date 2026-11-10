// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The quad index: one base table and five orderings of positions into it.

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

/// The five orderings held as positions into the base table, which is itself
/// sorted [`Order::Spog`].
const ORDINAL_ORDERS: [Order; 5] = [
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

    /// The slot this order's positions live in, or `None` for the base table.
    fn ordinal_slot(self) -> Option<usize> {
        ORDINAL_ORDERS.iter().position(|&order| order == self)
    }
}

/// Quads indexed six ways for range reads by any bound pattern.
///
/// Built once from `(quad, meta)` rows and read until the next rebuild.
/// Identical rows collapse; rows of the same quad with different metas are all
/// kept, and a [`pattern`](QuadIndex::pattern) read yields each quad once per
/// view, its first visible row in the order held, asserter, statement.
#[derive(Clone, Debug, Default)]
pub struct QuadIndex {
    /// Every row, sorted SPO then graph, then by [`QuadMeta::rank`].
    rows: Vec<(Quad, QuadMeta)>,
    /// Positions into `rows` for each of [`ORDINAL_ORDERS`], ties broken by
    /// position so the rows of one quad keep their held-first order.
    ordinals: [Vec<u32>; 5],
    /// Each named graph with whether it has a held row, sorted by id.
    graphs: Vec<(TermId, bool)>,
}

impl QuadIndex {
    /// Index the rows: collapse identical `(quad, meta)` rows, then sort the
    /// six orderings.
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

        let ordinals = ORDINAL_ORDERS.map(|order| {
            let mut positions: Vec<u32> = (0..count).collect();
            positions.sort_unstable_by_key(|&at| (order.key(&rows[at as usize].0), at));
            positions
        });

        let gspo = Order::Gspo
            .ordinal_slot()
            .expect("GSPO is an ordinal order");
        let mut graphs: Vec<(TermId, bool)> = Vec::new();
        for &at in &ordinals[gspo] {
            let (quad, meta) = &rows[at as usize];
            if quad.graph.is_default_graph() {
                continue;
            }
            match graphs.last_mut() {
                Some((graph, held)) if *graph == quad.graph => *held |= meta.held,
                _ => graphs.push((quad.graph, meta.held)),
            }
        }
        graphs.shrink_to_fit();

        QuadIndex {
            rows,
            ordinals,
            graphs,
        }
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

        let mut last: Option<Quad> = None;
        range
            .map(move |at| self.row(order, at))
            .filter(move |(_, meta)| view.admits(meta))
            .filter(move |(quad, _)| {
                // Rows of one quad are adjacent in every ordering, so the first
                // visible one stands for the quad.
                let fresh = last != Some(**quad);
                last = Some(**quad);
                fresh
            })
    }

    /// The named graphs with at least one row visible in `view`, by id. The
    /// default graph is not a named graph and is never listed.
    pub fn graph_names(&self, view: View) -> impl Iterator<Item = TermId> {
        self.graphs
            .iter()
            .filter(move |(_, held)| view == View::History || *held)
            .map(|(graph, _)| *graph)
    }

    /// Whether `graph` is a named graph with at least one row visible in
    /// `view`. Always `false` for [`TermId::DEFAULT_GRAPH`].
    #[must_use]
    pub fn contains_graph(&self, graph: TermId, view: View) -> bool {
        self.graphs
            .binary_search_by_key(&graph, |(id, _)| *id)
            .is_ok_and(|at| view == View::History || self.graphs[at].1)
    }

    /// The number of stored rows: distinct `(quad, meta)` pairs, so a quad
    /// carried by several metas counts once per meta.
    #[must_use]
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// Whether the index holds no rows.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// Bytes held by the index: the struct, the base table, the five
    /// orderings and the graph list, by allocated capacity.
    #[must_use]
    pub fn memory_bytes(&self) -> usize {
        size_of::<Self>()
            + self.rows.capacity() * size_of::<(Quad, QuadMeta)>()
            + self
                .ordinals
                .iter()
                .map(|positions| positions.capacity() * size_of::<u32>())
                .sum::<usize>()
            + self.graphs.capacity() * size_of::<(TermId, bool)>()
    }

    fn row(&self, order: Order, at: usize) -> (&Quad, &QuadMeta) {
        let at = match order.ordinal_slot() {
            None => at,
            Some(slot) => self.ordinals[slot][at] as usize,
        };
        let (quad, meta) = &self.rows[at];
        (quad, meta)
    }

    /// The positions in `order` whose first `bound` key entries equal the
    /// probe's.
    fn range(&self, order: Order, probe: &[u32; 4], bound: usize) -> Range<usize> {
        if bound == 0 {
            return 0..self.rows.len();
        }
        let compare = |quad: &Quad| order.key(quad)[..bound].cmp(&probe[..bound]);
        match order.ordinal_slot() {
            None => {
                let low = self.rows.partition_point(|(quad, _)| compare(quad).is_lt());
                let high = self.rows.partition_point(|(quad, _)| compare(quad).is_le());
                low..high
            },
            Some(slot) => {
                let positions = &self.ordinals[slot];
                let at = |position: &u32| &self.rows[*position as usize].0;
                let low = positions.partition_point(|position| compare(at(position)).is_lt());
                let high = positions.partition_point(|position| compare(at(position)).is_le());
                low..high
            },
        }
    }
}
