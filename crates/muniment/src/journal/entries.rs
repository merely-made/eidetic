// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! A journal kept one entry per key.
//!
//! [`save`](Journal::save) writes a whole log to one slot, so every save
//! rewrites all of it. This form keeps entry `n` at its own key,
//! `<prefix>/<n>`, with `n` as sixteen lowercase hex digits: keys sort in log
//! order and one [`scan`](Backend::scan) walks the log, the width
//! stickleback's operation logs use. Appending is then one small write on
//! every backend: a row on redb or IndexedDB, and one line of the file the
//! prefix names on the directory backend, when that prefix ends in `.jsonl`.
//!
//! Each entry is stored with the causes recorded for it. The log's identity
//! and provenance are not stored here. Whoever keeps the log records those (a
//! session manifest does) and passes them back to [`Journal::load_entries`].

use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::{Backend, Codec, SlotStore, StoreError, WriteOp};

use super::fork::{LogId, Provenance};
use super::log::Journal;
use super::seq::Seq;

/// The key of entry `seq` in a log kept under `prefix`.
pub fn entry_key(prefix: &str, seq: Seq) -> String {
    format!("{prefix}/{:016x}", seq.0)
}

/// An entry as written: the entry and its causes. Both fields are always
/// written, because a positional codec (postcard) cannot read back a field
/// that was written conditionally.
#[derive(Serialize)]
struct StoredRef<'a, T> {
    entry: &'a T,
    causes: &'a [Seq],
}

#[derive(Deserialize)]
struct Stored<T> {
    entry: T,
    causes: Vec<Seq>,
}

fn corrupt(prefix: &str, what: impl std::fmt::Display) -> StoreError {
    StoreError::Codec(format!("journal at {prefix}: {what}"))
}

impl<T> Journal<T> {
    /// The writes that store entries from `from` onward, one per key under
    /// `prefix`, for a caller committing them in its own batch (beside the
    /// checkpoint they belong with, say).
    pub fn entry_writes<C: Codec>(
        &self,
        prefix: &str,
        from: Seq,
    ) -> Result<Vec<WriteOp>, StoreError>
    where
        T: Serialize,
    {
        (from.index().min(self.len())..self.len())
            .map(|index| {
                let seq = Seq(index as u64);
                let value = C::encode(&StoredRef {
                    entry: &self.entries()[index],
                    causes: self.parents(seq),
                })?;
                Ok(WriteOp::Put {
                    key: entry_key(prefix, seq),
                    value,
                })
            })
            .collect()
    }

    /// Store entries from `from` onward, one per key under `prefix`, in one
    /// atomic batch.
    pub async fn append_entries<B, C>(
        &self,
        slots: &SlotStore<B, C>,
        prefix: &str,
        from: Seq,
    ) -> Result<(), StoreError>
    where
        B: Backend,
        C: Codec,
        T: Serialize,
    {
        let ops = self.entry_writes::<C>(prefix, from)?;
        if ops.is_empty() {
            return Ok(());
        }
        slots.backend().apply(&ops).await
    }

    /// Load a log kept one entry per key under `prefix`, with the identity and
    /// provenance its keeper recorded. A missing entry is an error, so a gap
    /// never reads as a shorter log.
    pub async fn load_entries<B, C>(
        slots: &SlotStore<B, C>,
        prefix: &str,
        id: Option<LogId>,
        provenance: Option<Provenance>,
    ) -> Result<Self, StoreError>
    where
        B: Backend,
        C: Codec,
        T: DeserializeOwned,
    {
        // '0' follows '/', so this range holds exactly the keys under prefix/.
        let keys = slots
            .backend()
            .scan(&format!("{prefix}/"), &format!("{prefix}0"))
            .await?;
        let mut log = Journal::with_identity(id, provenance);
        for key in keys {
            let expected = entry_key(prefix, log.next_seq());
            if key != expected {
                return Err(corrupt(
                    prefix,
                    format!("found {key} where {expected} belongs"),
                ));
            }
            let bytes = slots
                .backend()
                .get(&key)
                .await?
                .ok_or_else(|| corrupt(prefix, format!("{key} vanished while loading")))?;
            let stored: Stored<T> = C::decode(&bytes)?;
            log.append_caused_by(stored.causes, stored.entry)
                .map_err(|error| corrupt(prefix, format!("{key}: {error:?}")))?;
        }
        Ok(log)
    }
}

#[cfg(all(test, feature = "json"))]
mod tests {
    use super::*;
    use crate::{JsonSlots, MemoryBackend};

    fn slots() -> JsonSlots<MemoryBackend> {
        JsonSlots::new(MemoryBackend::new())
    }

    #[test]
    fn keys_sort_in_log_order() {
        let keys: Vec<String> = [0u64, 9, 10, 255, 256]
            .into_iter()
            .map(|n| entry_key("j", Seq(n)))
            .collect();
        let mut sorted = keys.clone();
        sorted.sort();
        assert_eq!(keys, sorted);
        assert_eq!(keys[2], "j/000000000000000a");
    }

    #[test]
    fn entries_round_trip_with_causes_and_identity() {
        pollster::block_on(async {
            let slots = slots();
            let mut log = Journal::starting_from(
                LogId::new("fork"),
                Provenance {
                    source: Some(LogId::new("parent")),
                    at: Seq(7),
                },
            );
            log.append("a".to_string());
            log.append_caused_by([Seq(0)], "b".to_string()).unwrap();
            log.append_entries(&slots, "j", Seq(0)).await.unwrap();

            let back: Journal<String> =
                Journal::load_entries(&slots, "j", log.id().cloned(), log.provenance().cloned())
                    .await
                    .unwrap();
            assert_eq!(back, log);
            assert_eq!(back.parents(Seq(1)), &[Seq(0)]);
        });
    }

    #[test]
    fn appending_from_a_cursor_writes_only_the_new_entries() {
        pollster::block_on(async {
            let backend = MemoryBackend::new();
            let slots = JsonSlots::new(backend.clone());
            let mut log: Journal<u32> = Journal::new();
            log.append(1);
            log.append(2);
            log.append_entries(&slots, "j", Seq(0)).await.unwrap();
            let saved = log.next_seq();
            log.append(3);

            let ops = log.entry_writes::<crate::JsonCodec>("j", saved).unwrap();
            assert_eq!(ops.len(), 1);
            log.append_entries(&slots, "j", saved).await.unwrap();
            assert_eq!(backend.len(), 3);
            let back: Journal<u32> = Journal::load_entries(&slots, "j", None, None)
                .await
                .unwrap();
            assert_eq!(back.entries(), &[1, 2, 3]);
        });
    }

    #[test]
    fn a_missing_entry_fails_the_load() {
        pollster::block_on(async {
            let slots = slots();
            let mut log: Journal<u32> = Journal::new();
            for n in 0..3 {
                log.append(n);
            }
            log.append_entries(&slots, "j", Seq(0)).await.unwrap();
            slots.delete(&entry_key("j", Seq(1))).await.unwrap();

            let error = Journal::<u32>::load_entries(&slots, "j", None, None)
                .await
                .unwrap_err();
            assert!(
                error
                    .to_string()
                    .contains("where j/0000000000000001 belongs"),
                "{error}"
            );
        });
    }

    #[test]
    fn neighbouring_prefixes_stay_apart() {
        pollster::block_on(async {
            let slots = slots();
            let mut a: Journal<u32> = Journal::new();
            a.append(1);
            a.append_entries(&slots, "j", Seq(0)).await.unwrap();
            let mut b: Journal<u32> = Journal::new();
            b.append(2);
            b.append_entries(&slots, "j2", Seq(0)).await.unwrap();

            let back: Journal<u32> = Journal::load_entries(&slots, "j", None, None)
                .await
                .unwrap();
            assert_eq!(back.entries(), &[1]);
        });
    }
}
