// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The capture seams a host supplies besides the store and the sealer.

use std::sync::atomic::{AtomicU64, Ordering};

/// Wall-clock milliseconds, supplied rather than read, so the capture core
/// compiles for wasm and tests run on a fixed time.
pub trait CaptureClock {
    /// Unix milliseconds now.
    fn now_ms(&self) -> u64;
}

/// The system clock. Absent on wasm32, where `SystemTime::now` is unavailable.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemClock;

#[cfg(not(target_arch = "wasm32"))]
impl CaptureClock for SystemClock {
    fn now_ms(&self) -> u64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
            .unwrap_or(0)
    }
}

/// A settable clock for tests and replays.
#[derive(Debug, Default)]
pub struct FixedClock(AtomicU64);

impl FixedClock {
    /// A clock reading `ms`.
    #[must_use]
    pub fn new(ms: u64) -> Self {
        Self(AtomicU64::new(ms))
    }

    /// Move the clock.
    pub fn set(&self, ms: u64) {
        self.0.store(ms, Ordering::SeqCst);
    }
}

impl CaptureClock for FixedClock {
    fn now_ms(&self) -> u64 {
        self.0.load(Ordering::SeqCst)
    }
}

/// Reads of a bounded store: the value is checked against a size ceiling
/// before the backend materializes it, so a structured load cannot be made to
/// allocate past its budget. A host with a read budget (Turnstone's
/// `BoundedStore`) implements this over its own accounting.
#[async_trait::async_trait(?Send)]
pub trait BoundedRead: muniment::Backend {
    /// The value at `key`, refused if it exceeds `max_value_bytes`.
    ///
    /// # Errors
    /// The backend's error, or `StoreError::LimitExceeded` over the ceiling.
    async fn get_limited(
        &self,
        key: &str,
        max_value_bytes: usize,
    ) -> Result<Option<Vec<u8>>, muniment::error::StoreError>;
}
