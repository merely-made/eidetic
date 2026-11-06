// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Pure connection-lifecycle state for the IndexedDB backend.
//!
//! Kept free of browser types so it is unit-testable on native targets; the
//! browser wiring (`onversionchange`, `onblocked`) lives in
//! `indexeddb_backend`.

use std::cell::Cell;
use std::rc::Rc;

use crate::StoreError;

/// Message returned by every operation after the connection was closed
/// because a newer version of the database was requested elsewhere.
pub(crate) const CLOSED_FOR_UPGRADE: &str = "database closed for a newer version; reopen";

/// Message returned by `open` when another connection holding an older
/// version prevents an upgrade from proceeding.
pub(crate) const OPEN_BLOCKED: &str = "database open blocked: another connection is holding an older version open; close it (or reload that tab) and retry";

/// Shared flag set when the browser asks this connection to close for an
/// upgrade. Clones of a backend share one flag.
#[derive(Clone, Default, Debug)]
pub(crate) struct ClosedFlag(Rc<Cell<bool>>);

impl ClosedFlag {
    pub(crate) fn mark_closed(&self) {
        self.0.set(true);
    }

    pub(crate) fn is_closed(&self) -> bool {
        self.0.get()
    }

    /// `Ok` while the connection is usable, a clear error once it is closed.
    pub(crate) fn check(&self) -> Result<(), StoreError> {
        if self.is_closed() {
            Err(StoreError::Backend(CLOSED_FOR_UPGRADE.to_string()))
        } else {
            Ok(())
        }
    }
}

pub(crate) fn blocked_error() -> StoreError {
    StoreError::Backend(OPEN_BLOCKED.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flag_starts_open_and_checks_ok() {
        let flag = ClosedFlag::default();
        assert!(!flag.is_closed());
        assert!(flag.check().is_ok());
    }

    #[test]
    fn marking_closed_fails_fast_with_clear_error() {
        let flag = ClosedFlag::default();
        flag.mark_closed();
        assert!(flag.is_closed());
        match flag.check() {
            Err(StoreError::Backend(message)) => assert_eq!(message, CLOSED_FOR_UPGRADE),
            other => panic!("expected backend error, got {other:?}"),
        }
    }

    #[test]
    fn clones_share_state() {
        let a = ClosedFlag::default();
        let b = a.clone();
        a.mark_closed();
        assert!(b.is_closed());
    }

    #[test]
    fn blocked_error_names_the_cause() {
        match blocked_error() {
            StoreError::Backend(message) => assert!(message.contains("older version")),
            other => panic!("expected backend error, got {other:?}"),
        }
    }
}
