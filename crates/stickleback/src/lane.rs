// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The parts of joining a lane that need no transport.
//!
//! A lane's protocol id and the staged [`JoinError`] belong to every domain
//! that names a lane, including a build without the `net` feature, where
//! [`JoinedSpace`](crate::JoinedSpace) itself is absent. Keeping them here
//! means a domain's error type does not change shape with the feature.

/// Derive a lane's sync protocol id from its kind and its space.
///
/// One canonical spelling so both peers agree by construction:
/// `<kind>/<space-hex>`, e.g. `gemot/constitution/v1/6d6d…`. The kind names
/// the lane's wire grammar and should carry its own version segment; the
/// space id keeps two spaces of the same kind on one endpoint from colliding.
pub fn lane_id(kind: &str, space: [u8; 32]) -> String {
    let hex: String = space.iter().map(|byte| format!("{byte:02x}")).collect();
    format!("{kind}/{hex}")
}

/// A join failure, staged: session spawn, stream open, subscription, or a
/// later live publish. Sources are formatted to strings at the boundary so
/// the error stays free of p2panda's generic parameters.
#[derive(Debug, thiserror::Error)]
pub enum JoinError {
    #[error("logsync spawn: {0}")]
    Spawn(String),
    #[error("logsync stream: {0}")]
    Stream(String),
    #[error("logsync subscribe: {0}")]
    Subscribe(String),
    #[error("publish: {0}")]
    Publish(String),
    #[error("logsync shutdown: {0}")]
    Shutdown(String),
}
