// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The tick trace (F123).

use state_witness::{Trace, TraceDivergence, TraceError, Witness, first_trace_divergence, label};

/// A toy run: each tick witnesses three sites, and `plant` bumps one site's
/// value from a tick onwards.
fn run(ticks: &[u64], plant: Option<(u64, u64)>) -> Trace {
    let mut trace = Trace::new();
    for &tick in ticks {
        let mut w = Witness::new();
        w.insert("clock", &tick).unwrap();
        for site in 0..3u64 {
            let bumped = matches!(plant, Some((at, s)) if tick >= at && s == site);
            w.insert(
                label("site", [site]),
                &(site * 10 + tick + u64::from(bumped)),
            )
            .unwrap();
        }
        trace.push(tick, w).unwrap();
    }
    trace
}

#[test]
fn ticks_must_increase() {
    let mut trace = Trace::new();
    trace.push(4, Witness::new()).unwrap();
    assert_eq!(
        trace.push(4, Witness::new()),
        Err(TraceError::TickNotIncreasing { last: 4, tick: 4 })
    );
    assert_eq!(
        trace.push(2, Witness::new()),
        Err(TraceError::TickNotIncreasing { last: 4, tick: 2 })
    );
    trace.push(9, Witness::new()).unwrap();
    assert_eq!(trace.len(), 2);
    assert!(trace.get(9).is_some() && trace.get(5).is_none());
}

#[test]
fn equal_runs_do_not_diverge() {
    let ticks: Vec<u64> = (1..=50).collect();
    assert_eq!(
        first_trace_divergence(&run(&ticks, None), &run(&ticks, None)),
        None
    );
}

#[test]
fn a_planted_change_names_its_tick_and_label_and_not_before() {
    let ticks: Vec<u64> = (1..=50).collect();
    let found = first_trace_divergence(&run(&ticks, None), &run(&ticks, Some((23, 2)))).unwrap();
    assert_eq!(found.tick, 23);
    assert_eq!(found.divergence.unwrap().label, "site:2");
}

#[test]
fn a_tick_on_one_side_is_a_tick_divergence() {
    let full = run(&[1, 2, 3, 4], None);
    let gapped = run(&[1, 2, 4], None);
    let expected = Some(TraceDivergence {
        tick: 3,
        divergence: None,
    });
    assert_eq!(first_trace_divergence(&full, &gapped), expected);
    assert_eq!(first_trace_divergence(&gapped, &full), expected);
    let short = run(&[1, 2], None);
    assert_eq!(first_trace_divergence(&full, &short).unwrap().tick, 3);
    assert_eq!(
        first_trace_divergence(&full, &gapped).unwrap().to_string(),
        "tick 3: on one side only"
    );
}

#[test]
fn an_earlier_entry_divergence_beats_a_later_missing_tick() {
    let a = run(&[1, 2, 3, 4], Some((2, 0)));
    let b = run(&[1, 2, 4], None);
    let found = first_trace_divergence(&a, &b).unwrap();
    assert_eq!(
        (found.tick, found.divergence.unwrap().label.as_str()),
        (2, "site:0")
    );
}
