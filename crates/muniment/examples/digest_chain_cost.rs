// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! What the v2 journal digest costs: append time and heap per entry, building
//! a chain whole (what a reopen pays), and naming the newest and middle
//! cursors, beside v1 recomputed at the same cursors.
//!
//! Each entry is 516 canonical bytes, near the 500 of Mere's graph journal
//! entries, in two shapes: `mapped` holds an eight-pair map (the canonical
//! encoder buffers and sorts each pair), `flat` holds strings only, so the
//! two bracket what encoding adds to hashing. Run with `--release`; give entry counts as arguments (default
//! 10000 100000 600000; 600000 is the journal of Mere's 100k-node scale).
//! Each timing is checked against a recomputation from scratch.

use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::BTreeMap;
use std::hint::black_box;
use std::sync::atomic::{AtomicIsize, Ordering};
use std::time::{Duration, Instant};

use muniment::Journal;
use muniment::journal::DigestChain;
use serde::Serialize;

/// Counts live heap bytes, so the chain's footprint is measured, not assumed.
struct Counting;

static LIVE: AtomicIsize = AtomicIsize::new(0);

#[allow(unsafe_code)]
// SAFETY: every call is forwarded unchanged to System; only a counter is added.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        LIVE.fetch_add(layout.size() as isize, Ordering::Relaxed);
        // SAFETY: forwarded unchanged to the system allocator.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        LIVE.fetch_sub(layout.size() as isize, Ordering::Relaxed);
        // SAFETY: `ptr` came from `alloc` above with this layout.
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

fn live() -> isize {
    LIVE.load(Ordering::Relaxed)
}

#[derive(Serialize)]
struct Flat {
    author: String,
    at_ms: u64,
    subject: String,
    body: String,
}

fn flat(n: usize) -> Flat {
    Flat {
        author: format!("author-{}", n % 5),
        at_ms: n as u64,
        subject: format!("urn:uuid:{n:032x}"),
        body: format!("{}{n:010}", "x".repeat(353)),
    }
}

#[derive(Serialize)]
struct Entry {
    author: String,
    at_ms: u64,
    subject: String,
    fields: BTreeMap<String, String>,
}

fn mapped(n: usize) -> Entry {
    Entry {
        author: format!("author-{}", n % 5),
        at_ms: n as u64,
        subject: format!("urn:uuid:{n:032x}"),
        fields: (0..8)
            .map(|f| (format!("field-{f}"), format!("value-{n:010}-{f:04}")))
            .collect(),
    }
}

fn median(mut samples: Vec<Duration>) -> Duration {
    samples.sort();
    samples[samples.len() / 2]
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1e3
}

fn run<E: Serialize>(n: usize, shape: &str, make: fn(usize) -> E) {
    let mut log = Journal::new();
    for i in 0..n {
        log.append(make(i));
    }

    // Append: one push per entry, the chain grown as a holder grows it.
    let before = live();
    let mut chain = DigestChain::new();
    let t = Instant::now();
    for e in log.entries() {
        chain.push(e).unwrap();
    }
    let append = t.elapsed();
    let grown = live() - before;
    println!(
        "{n}\t{shape}\tappend_us_per_entry\t{:.3}",
        append.as_secs_f64() * 1e6 / n as f64
    );
    println!(
        "{n}\t{shape}\theap_bytes_pushed\t{grown}\t{:.1} per entry",
        grown as f64 / n as f64
    );

    // Build whole: what a reopen pays, with the vector reserved once.
    let before = live();
    let t = Instant::now();
    let whole = DigestChain::of(&log).unwrap();
    let build = t.elapsed();
    let reserved = live() - before;
    assert_eq!(whole, chain, "built whole equals pushed");
    println!("{n}\t{shape}\tbuild_whole_ms\t{:.3}", ms(build));
    println!(
        "{n}\t{shape}\theap_bytes_built\t{reserved}\t{:.1} per entry",
        reserved as f64 / n as f64
    );
    drop(whole);

    // Lookups at the newest and middle cursors, against recomputing them.
    let reps = if n >= 100_000 { 5 } else { 11 };
    for (name, k) in [("newest", n), ("mid", n / 2)] {
        let lookups = 1_000_000u32;
        let t = Instant::now();
        for _ in 0..lookups {
            black_box(chain.at(black_box(k)));
        }
        let lookup = t.elapsed();
        println!(
            "{n}\t{shape}\tlookup_{name}_ns\t{:.2}",
            lookup.as_secs_f64() * 1e9 / f64::from(lookups)
        );
        let v2 = (0..reps)
            .map(|_| {
                let t = Instant::now();
                let d = log.chain_digest(k).unwrap();
                let e = t.elapsed();
                assert_eq!(d, chain.at(k), "lookup equals recomputation at {name}");
                e
            })
            .collect();
        let v1 = (0..reps)
            .map(|_| {
                let t = Instant::now();
                let d = log.prefix_digest(k).unwrap();
                let e = t.elapsed();
                assert!(d.is_some() && d != chain.at(k), "control: v1 differs");
                e
            })
            .collect();
        println!(
            "{n}\t{shape}\tv2_from_scratch_{name}_ms\t{:.3}",
            ms(median(v2))
        );
        println!(
            "{n}\t{shape}\tv1_prefix_digest_{name}_ms\t{:.3}",
            ms(median(v1))
        );
    }
    assert_ne!(chain.at(n), chain.at(n / 2), "control: cursors differ");
}

fn main() {
    let counts: Vec<usize> = std::env::args()
        .skip(1)
        .map(|a| a.parse().expect("an entry count"))
        .collect();
    let counts = if counts.is_empty() {
        vec![10_000, 100_000, 600_000]
    } else {
        counts
    };
    for n in counts {
        run(n, "flat", flat);
        run(n, "mapped", mapped);
    }
}
