// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Deep time: giving a generated world a past by running its own simulation
//! before anyone steps in, so the history a player has to overcome is made
//! the same way a played one is. This module drives; it never simulates.

use std::error::Error;
use std::fmt;

use serde::{Deserialize, Serialize};

/// A product's simulation, advanced one tick at a time with no other hand on
/// it. Deep time only ever calls these three methods.
pub trait Epochal {
    /// One tick of the world's own rules.
    fn advance(&mut self);
    /// How many epochs have closed so far.
    fn epochs(&self) -> u64;
    /// How many ticks have run so far.
    fn tick(&self) -> u64;
}

/// A span of epochs to run before anyone steps in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeepTime {
    pub epochs: u32,
}

/// Where a run of deep time started and where it landed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Handover {
    pub span: DeepTime,
    pub from_epoch: u64,
    pub to_epoch: u64,
    pub from_tick: u64,
    pub to_tick: u64,
}

/// Deep time's only failure: the world stopped closing epochs.
///
/// Some epoch rules (gated on a player action, for instance) may never close
/// on their own, so a tick ceiling is mandatory rather than advisory.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeepTimeError {
    Stalled { ticks: u64, epochs_closed: u64 },
}

impl fmt::Display for DeepTimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DeepTimeError::Stalled {
                ticks,
                epochs_closed,
            } => write!(
                f,
                "deep time stalled after {ticks} ticks having closed {epochs_closed} epoch(s)"
            ),
        }
    }
}

impl Error for DeepTimeError {}

/// Runs `world` until `span.epochs` more epochs have closed than stood when
/// it started, stopping immediately after the advance that closes the last
/// one, so the world stands on that boundary. A zero span advances nothing.
/// Refuses with [`DeepTimeError::Stalled`] rather than spin past `max_ticks`.
pub fn run<S: Epochal>(
    world: &mut S,
    span: DeepTime,
    max_ticks: u64,
) -> Result<Handover, DeepTimeError> {
    let from_epoch = world.epochs();
    let from_tick = world.tick();

    if span.epochs == 0 {
        return Ok(Handover {
            span,
            from_epoch,
            to_epoch: from_epoch,
            from_tick,
            to_tick: from_tick,
        });
    }

    let target = from_epoch + u64::from(span.epochs);
    let mut ticks = 0u64;
    while world.epochs() < target {
        if ticks >= max_ticks {
            return Err(DeepTimeError::Stalled {
                ticks,
                epochs_closed: world.epochs() - from_epoch,
            });
        }
        world.advance();
        ticks += 1;
    }

    Ok(Handover {
        span,
        from_epoch,
        to_epoch: world.epochs(),
        from_tick,
        to_tick: world.tick(),
    })
}

#[cfg(test)]
#[path = "deep_time_tests.rs"]
mod tests;
