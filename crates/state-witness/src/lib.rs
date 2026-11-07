// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! A labelled state witness: FNV-1a 64 over postcard bytes, kept per label.
//!
//! A [`Witness`] holds entries unique by label, in natural order (digit runs
//! compare as numbers), and [`first_divergence`] names the first label two
//! witnesses disagree on. A [`Trace`] holds one witness per tick. Both carry
//! serde derives and a framed postcard form (`mere-framing`).
//!
//! This is an equality witness for replay, not a cryptographic digest.
//! Rulings: the dynamics grammar plan, F116 to F131.

mod trace;
mod witness;

pub use framing::FrameError;
pub use trace::{Trace, TraceDivergence, TraceError, first_trace_divergence};
pub use witness::{Divergence, Entry, Witness, divergences, first_divergence};

use std::fmt;

use serde::Serialize;

const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const PRIME: u64 = 0x0000_0100_0000_01B3;

/// FNV-1a 64 over `bytes`, equal to `isometer_core::snapshot::hash_bytes`.
pub fn hash_bytes(bytes: &[u8]) -> u64 {
    let mut fnv = Fnv(OFFSET);
    fnv.write(bytes);
    fnv.0
}

/// FNV-1a 64 over `value`'s postcard bytes, streamed without allocating:
/// equal to `hash_bytes(&postcard::to_allocvec(value)?)`.
pub fn hash_value<T: Serialize + ?Sized>(value: &T) -> Result<u64, Error> {
    postcard::serialize_with_flavor(value, Fnv(OFFSET)).map_err(|_| Error::Encode)
}

/// Builds a label in the grammar `kind:a/b`; with no parts, just `kind`.
pub fn label<I, P>(kind: &str, parts: I) -> String
where
    I: IntoIterator<Item = P>,
    P: fmt::Display,
{
    let mut out = String::from(kind);
    for (i, part) in parts.into_iter().enumerate() {
        out.push(if i == 0 { ':' } else { '/' });
        out.push_str(&part.to_string());
    }
    out
}

/// Why an entry was refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// The value did not encode as postcard.
    Encode,
    /// The witness already holds this label.
    DuplicateLabel(String),
    /// A NUL would make the fold ambiguous (F122).
    NulInLabel(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Encode => f.write_str("value did not encode as postcard"),
            Error::DuplicateLabel(l) => write!(f, "duplicate label {l:?}"),
            Error::NulInLabel(l) => write!(f, "label {l:?} contains a NUL"),
        }
    }
}

impl std::error::Error for Error {}

/// The running FNV-1a state, also a postcard flavor.
struct Fnv(u64);

impl Fnv {
    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 ^= u64::from(*byte);
            self.0 = self.0.wrapping_mul(PRIME);
        }
    }
}

impl postcard::ser_flavors::Flavor for Fnv {
    type Output = u64;

    fn try_push(&mut self, data: u8) -> postcard::Result<()> {
        self.write(&[data]);
        Ok(())
    }

    fn try_extend(&mut self, data: &[u8]) -> postcard::Result<()> {
        self.write(data);
        Ok(())
    }

    fn finalize(self) -> postcard::Result<u64> {
        Ok(self.0)
    }
}

fn fold<'a>(entries: impl IntoIterator<Item = (&'a str, u64)>) -> u64 {
    let mut fnv = Fnv(OFFSET);
    for (label, digest) in entries {
        fnv.write(label.as_bytes());
        fnv.write(&[0]);
        fnv.write(&digest.to_le_bytes());
    }
    fnv.0
}
