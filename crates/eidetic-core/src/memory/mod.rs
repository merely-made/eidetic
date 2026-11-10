// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The memory program (engine split D45, D87): the store-side retention
//! passes, generic over node records. It names no kernel type, only Eidetic's
//! store. Not to be confused with [`crate::retention`], which holds
//! owner-scoped retained copies and never evicts.
//!
//! - [`levels`]: short-term and long-term memory over any node record, and the
//!   eviction policy;
//! - [`forgetting`]: applying a forget proposal (dropping cached content);
//! - [`retirement`]: proposing and applying the purge of aged tombstones.
//!
//! Graph-reading halves live with the graph, as thin adapters that read nodes
//! into [`MemoryRecord`]s and map evictable ids to urls.

pub mod forgetting;
pub mod levels;
pub mod retirement;

pub use forgetting::{ForgetProposal, apply_forgetting};
pub use levels::{
    EvictionPolicy, MemoryCensus, MemoryLevel, MemoryRecord, census, evictable_short_term,
    level_of,
};
pub use retirement::{RetirementProposal, apply_retirement, propose_retirement};
