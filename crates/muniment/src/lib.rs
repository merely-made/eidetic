// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! muniment — a portable persistence store.
//!
//! A muniment room is where a household keeps its records: the deeds and
//! documents preserved as evidence. This crate is that room for an app's durable
//! state, and nothing more.
//!
//! Four pieces over one seam:
//!
//! - [`Backend`] is the host-supplied byte store. The host realizes it as the
//!   filesystem on desktop, IndexedDB in the browser (from a window or worker
//!   scope; OPFS is a possible future host-supplied backend, not shipped), or an
//!   embedded store (redb, fjall). It is `async` and `?Send` so a browser can
//!   await IndexedDB promises; desktop backends return ready futures and pay nothing. muniment
//!   ships only [`MemoryBackend`], the in-memory test floor.
//! - [`SlotStore`] holds typed **mutable named slots**: `save` overwrites, `load`
//!   returns the latest. The current-session, current-campaign, current-project
//!   pattern. Serialized through a pluggable [`Codec`] (JSON or postcard here,
//!   your own otherwise), so muniment mandates no wire format.
//! - [`BlobStore`] holds **content-addressed immutable blobs**: `put` returns a
//!   blake3 [`Hash`], `get` fetches by it. Identical content is stored once; a
//!   new version is new bytes with a new hash, never a mutation.
//! - [`Journal`] holds an **append-only replayable sequence** with stable cursors,
//!   optional causal links, fork provenance, and slot-backed persistence.
//!
//! muniment stores durable bytes and their journaled order. Domain meaning and
//! content models remain layers above it.

pub mod atomic_file;
pub mod backend;
pub mod blob;
pub mod codec;
#[cfg(test)]
mod custody_transact_tests;
#[cfg(feature = "directory")]
pub mod directory_backend;
pub mod error;
#[cfg(all(feature = "fjall", not(target_arch = "wasm32")))]
pub mod fjall_backend;
#[cfg(all(feature = "fjall", not(target_arch = "wasm32")))]
pub mod fjall_limits;
#[cfg(all(feature = "fjall", not(target_arch = "wasm32")))]
pub use fjall_backend::FjallStore;
#[cfg(all(feature = "fjall", not(target_arch = "wasm32")))]
pub use fjall_limits::{
    FjallBoundError, FjallLimitKind, FjallOwnerLimits, FjallReadBudget, FjallReadLimits,
    FjallRecoveryLimits,
};
#[cfg(all(feature = "indexeddb", target_arch = "wasm32"))]
pub mod indexeddb_backend;
pub mod journal;
pub mod key_range;
#[cfg(feature = "redb")]
pub mod redb_backend;
#[cfg(feature = "redb")]
pub mod redb_upgrade;
pub mod slot;
#[cfg(feature = "zip")]
pub mod zip_backend;

pub use backend::{Backend, MemoryBackend, TransactFn, TransactionReader, WriteOp};
pub use blob::{BlobStore, Hash};
pub use codec::Codec;
#[cfg(feature = "directory")]
pub use directory_backend::DirectoryBackend;
pub use error::{StoreError, StoreLimitKind};
pub use journal::{CausalError, Journal, LogId, Provenance, Seq};
pub use slot::SlotStore;

#[cfg(all(feature = "indexeddb", target_arch = "wasm32"))]
pub use indexeddb_backend::IndexedDbBackend;

#[cfg(feature = "redb")]
pub use redb_backend::RedbBackend;
#[cfg(feature = "redb")]
pub use redb_upgrade::RedbOpenOutcome;

#[cfg(feature = "zip")]
pub use zip_backend::ZipBackend;

#[cfg(feature = "json")]
pub use codec::JsonCodec;
#[cfg(feature = "postcard")]
pub use codec::PostcardCodec;

/// [`SlotStore`] with the JSON codec, the friendly default.
#[cfg(feature = "json")]
pub type JsonSlots<B> = slot::SlotStore<B, codec::JsonCodec>;

/// [`SlotStore`] with the deterministic postcard codec.
#[cfg(feature = "postcard")]
pub type PostcardSlots<B> = slot::SlotStore<B, codec::PostcardCodec>;
