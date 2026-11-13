// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Times `QuadIndex::apply` against `QuadIndex::build` on a chain-shaped
//! dataset (8 rows per node). Release only: `cargo run --release -p
//! chartulary --example quads_apply [nodes]` (default 100,000 = 800k rows).

use std::hint::black_box;
use std::time::{Duration, Instant};

use chartulary::quads::{Quad, QuadIndex, QuadMeta, TermId, View};

const PREDICATES: u32 = 8;
const GRAPHS: u32 = 4;

fn id(raw: u32) -> TermId {
    TermId::from_u32(raw).expect("non-zero")
}

struct Ids {
    next: u32,
}

impl Ids {
    fn fresh(&mut self) -> TermId {
        self.next += 1;
        id(self.next)
    }
}

/// Node `k`'s id; predicates and graphs sit above the nodes.
fn node(k: u32) -> TermId {
    id(1 + k)
}

fn predicate(nodes: u32, k: u32) -> TermId {
    id(nodes + 1 + k % PREDICATES)
}

fn graph(nodes: u32, k: u32) -> TermId {
    id(nodes + 1 + PREDICATES + k % GRAPHS)
}

/// One statement from `from` to `to` and its reifier rows: eight rows, the
/// shape one asserted statement projects to.
fn statement(nodes: u32, ids: &mut Ids, from: u32, to: u32) -> Vec<(Quad, QuadMeta)> {
    let g = graph(nodes, from);
    let mut rows = vec![(
        Quad {
            subject: node(from),
            predicate: predicate(nodes, from),
            object: node(to),
            graph: g,
        },
        QuadMeta::HELD,
    )];
    let reifier = ids.fresh();
    for k in 1..8 {
        let object = if k == 1 { node(from) } else { ids.fresh() };
        rows.push((
            Quad {
                subject: reifier,
                predicate: predicate(nodes, k),
                object,
                graph: g,
            },
            QuadMeta::HELD,
        ));
    }
    rows
}

fn median(mut samples: Vec<Duration>) -> f64 {
    samples.sort();
    samples[samples.len() / 2].as_secs_f64() * 1e3
}

fn reads(index: &QuadIndex, nodes: u32) -> (f64, f64) {
    let mut point = Vec::new();
    let mut scan = Vec::new();
    for k in 0..51 {
        let t = Instant::now();
        black_box(index.pattern(Some(node(k * 7 % nodes)), None, None, None, View::Live).count());
        point.push(t.elapsed());
        let t = Instant::now();
        black_box(index.pattern(None, Some(predicate(nodes, 3)), None, None, View::Live).count());
        scan.push(t.elapsed());
    }
    (median(point), median(scan))
}

fn main() {
    let nodes: u32 = std::env::args()
        .nth(1)
        .map_or(100_000, |arg| arg.parse().expect("nodes"));
    let mut ids = Ids {
        next: nodes + PREDICATES + GRAPHS + 1,
    };
    let rows: Vec<_> = (0..nodes)
        .flat_map(|k| statement(nodes, &mut ids, k, (k + 1) % nodes))
        .collect();
    let t = Instant::now();
    let mut index = QuadIndex::build(rows);
    println!("rows {}: build {:.1} ms, {} bytes", index.len(), t.elapsed().as_secs_f64() * 1e3, index.memory_bytes());
    let (point, scan) = reads(&index, nodes);
    println!("reads before apply: point {point:.4} ms, bound predicate {scan:.3} ms");

    let mut edit = 0_u32;
    let mut next_edit = |ids: &mut Ids| {
        edit += 1;
        statement(nodes, ids, edit * 7919 % nodes, edit * 104_729 % nodes)
    };
    let t = Instant::now();
    index.apply(next_edit(&mut ids), []);
    println!("first apply after build (materializes SPOG, grows every Vec): {:.2} ms", t.elapsed().as_secs_f64() * 1e3);

    let mut asserts = Vec::new();
    let mut applied = Vec::new();
    for _ in 0..101 {
        let rows = next_edit(&mut ids);
        let t = Instant::now();
        index.apply(rows.iter().copied(), []);
        asserts.push(t.elapsed());
        applied.push(rows);
    }
    println!("assert one statement (8 rows): median {:.3} ms", median(asserts));

    let mut withdraws = Vec::new();
    for rows in &applied {
        let t = Instant::now();
        index.apply([], rows.iter().copied());
        withdraws.push(t.elapsed());
    }
    println!("withdraw one statement (8 rows flipped): median {:.4} ms", median(withdraws));

    // Mere's withdrawal shape: the held rows go and history rows arrive.
    let mut mixed = Vec::new();
    for _ in 0..101 {
        let held = next_edit(&mut ids);
        index.apply(held.iter().copied(), []);
        let history: Vec<_> = next_edit(&mut ids)
            .into_iter()
            .map(|(quad, meta)| (quad, QuadMeta { held: false, ..meta }))
            .collect();
        let t = Instant::now();
        index.apply(history, held);
        mixed.push(t.elapsed());
    }
    println!("8 withdrawn rows added plus 8 flipped: median {:.3} ms", median(mixed));

    let (point, scan) = reads(&index, nodes);
    println!("reads after apply: point {point:.4} ms, bound predicate {scan:.3} ms; {} bytes", index.memory_bytes());
}
