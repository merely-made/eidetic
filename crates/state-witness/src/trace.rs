// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! One witness per tick, and the first tick and label that diverge (F123).

use std::cmp::Ordering;
use std::fmt;

use serde::{Deserialize, Serialize, Serializer};

use crate::{Divergence, FrameError, Witness, first_divergence};

/// A framed trace's magic. The leading payload bytes cannot spell it: that
/// would need a label beginning `TRC\0`, and labels hold no NUL.
pub(crate) const MAGIC: [u8; 8] = *b"MERETRC\0";
pub(crate) const VERSION: u16 = 1;

/// Witnesses by tick, ticks strictly increasing. Serializes as a sequence of
/// `(tick, witness)` pairs; loading refuses ticks out of order.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Deserialize)]
#[serde(try_from = "Vec<(u64, Witness)>")]
pub struct Trace {
    ticks: Vec<(u64, Witness)>,
}

/// Why a tick was refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TraceError {
    /// The tick is not after the trace's last.
    TickNotIncreasing { last: u64, tick: u64 },
}

impl fmt::Display for TraceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TraceError::TickNotIncreasing { last, tick } => {
                write!(f, "tick {tick} does not follow tick {last}")
            },
        }
    }
}

impl std::error::Error for TraceError {}

impl Trace {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, tick: u64, witness: Witness) -> Result<(), TraceError> {
        if let Some(&(last, _)) = self.ticks.last()
            && tick <= last
        {
            return Err(TraceError::TickNotIncreasing { last, tick });
        }
        self.ticks.push((tick, witness));
        Ok(())
    }

    pub fn ticks(&self) -> &[(u64, Witness)] {
        &self.ticks
    }

    pub fn get(&self, tick: u64) -> Option<&Witness> {
        self.ticks
            .binary_search_by_key(&tick, |(t, _)| *t)
            .ok()
            .map(|i| &self.ticks[i].1)
    }

    pub fn len(&self) -> usize {
        self.ticks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ticks.is_empty()
    }

    /// The trace as a framed postcard record.
    pub fn to_framed(&self) -> Result<Vec<u8>, FrameError> {
        framing::frame(MAGIC, VERSION, self)
    }

    /// Reads a framed trace, refusing another magic, another version, ticks
    /// out of order or an invalid witness.
    pub fn from_framed(bytes: &[u8]) -> Result<Self, FrameError> {
        framing::unframe(MAGIC, VERSION, bytes)
    }
}

impl Serialize for Trace {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.ticks.serialize(serializer)
    }
}

impl TryFrom<Vec<(u64, Witness)>> for Trace {
    type Error = TraceError;

    fn try_from(ticks: Vec<(u64, Witness)>) -> Result<Self, TraceError> {
        let mut trace = Trace::new();
        for (tick, witness) in ticks {
            trace.push(tick, witness)?;
        }
        Ok(trace)
    }
}

/// Where two traces first disagree. `divergence` is `None` when only one
/// side holds the tick; [`Trace::get`] says which.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceDivergence {
    pub tick: u64,
    pub divergence: Option<Divergence>,
}

impl fmt::Display for TraceDivergence {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.divergence {
            Some(d) => write!(f, "tick {}: {d}", self.tick),
            None => write!(f, "tick {}: on one side only", self.tick),
        }
    }
}

/// The first tick, in order, at which the traces disagree.
pub fn first_trace_divergence(left: &Trace, right: &Trace) -> Option<TraceDivergence> {
    let (mut l, mut r) = (left.ticks.iter().peekable(), right.ticks.iter().peekable());
    loop {
        let tick = match (l.peek(), r.peek()) {
            (None, None) => return None,
            (Some((t, _)), None) | (None, Some((t, _))) => *t,
            (Some((a, wa)), Some((b, wb))) => match a.cmp(b) {
                Ordering::Equal => {
                    if let Some(d) = first_divergence(wa, wb) {
                        return Some(TraceDivergence {
                            tick: *a,
                            divergence: Some(d),
                        });
                    }
                    l.next();
                    r.next();
                    continue;
                },
                Ordering::Less => *a,
                Ordering::Greater => *b,
            },
        };
        return Some(TraceDivergence {
            tick,
            divergence: None,
        });
    }
}
