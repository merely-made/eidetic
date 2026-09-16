// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// A toy simulation whose epoch closes every `epoch_length` ticks.
struct Metronome {
    tick: u64,
    epoch_length: u64,
    epochs: u64,
}

impl Metronome {
    fn new(epoch_length: u64) -> Self {
        Self {
            tick: 0,
            epoch_length,
            epochs: 0,
        }
    }
}

impl Epochal for Metronome {
    fn advance(&mut self) {
        self.tick += 1;
        if self.tick.is_multiple_of(self.epoch_length) {
            self.epochs += 1;
        }
    }

    fn epochs(&self) -> u64 {
        self.epochs
    }

    fn tick(&self) -> u64 {
        self.tick
    }
}

/// A simulation whose epoch rule never closes — the `Gated` /
/// `PlayerTriggered` case deep time's ceiling exists for.
struct Stubborn {
    tick: u64,
}

impl Epochal for Stubborn {
    fn advance(&mut self) {
        self.tick += 1;
    }

    fn epochs(&self) -> u64 {
        0
    }

    fn tick(&self) -> u64 {
        self.tick
    }
}

#[test]
fn a_three_epoch_span_stops_on_the_third_boundary() {
    let mut world = Metronome::new(4);
    let handover = run(&mut world, DeepTime { epochs: 3 }, 100).unwrap();

    assert_eq!(
        handover,
        Handover {
            span: DeepTime { epochs: 3 },
            from_epoch: 0,
            to_epoch: 3,
            from_tick: 0,
            to_tick: 12,
        }
    );
    assert_eq!(
        world.tick, 12,
        "stopped immediately after the third boundary, not before or after it"
    );
}

#[test]
fn a_zero_span_leaves_the_tick_unchanged() {
    let mut world = Metronome::new(4);
    world.advance();
    world.advance();

    let handover = run(&mut world, DeepTime { epochs: 0 }, 100).unwrap();

    assert_eq!(world.tick, 2, "no ticks were run");
    assert_eq!(handover.from_tick, 2);
    assert_eq!(handover.to_tick, 2);
    assert_eq!(handover.from_epoch, handover.to_epoch);
}

#[test]
fn a_simulation_that_never_closes_an_epoch_is_refused_at_the_ceiling() {
    let mut world = Stubborn { tick: 0 };

    let err = run(&mut world, DeepTime { epochs: 1 }, 10).unwrap_err();

    assert_eq!(
        err,
        DeepTimeError::Stalled {
            ticks: 10,
            epochs_closed: 0
        }
    );
    assert_eq!(
        world.tick, 10,
        "it still ran up to the ceiling before refusing"
    );
}

#[test]
fn starting_mid_epoch_still_runs_exactly_the_requested_closures() {
    let mut world = Metronome::new(4);
    // Two ticks in, mid-way through the first epoch: none closed yet.
    world.advance();
    world.advance();

    let handover = run(&mut world, DeepTime { epochs: 2 }, 100).unwrap();

    // The partial epoch already underway does not count: two full closures
    // are required from here, landing on tick 8 (the second boundary after
    // tick 2), not tick 6.
    assert_eq!(handover.from_epoch, 0);
    assert_eq!(handover.to_epoch, 2);
    assert_eq!(handover.from_tick, 2);
    assert_eq!(handover.to_tick, 8);
}

#[test]
fn a_stalled_error_displays_the_ticks_and_epochs_closed() {
    let err = DeepTimeError::Stalled {
        ticks: 5,
        epochs_closed: 1,
    };
    let message = err.to_string();
    assert!(message.contains('5'));
    assert!(message.contains('1'));
}
