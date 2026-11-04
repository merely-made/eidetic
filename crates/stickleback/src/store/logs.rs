// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl<B, E, L> LogStore<Operation<E>, VerifyingKey, L, u32, Hash> for MunimentStore<B, E>
where
    B: Backend + Clone + Send + 'static,
    E: Extensions + Send + 'static,
    L: LogId + Send + 'static,
{
    type Error = StoreError;

    async fn get_latest_entry(
        &self,
        author: &VerifyingKey,
        log_id: &L,
    ) -> Result<Option<Operation<E>>, StoreError> {
        let prefix = log_prefix(author, log_id)?;
        let keys = self.backend.scan(&prefix, &scan_end(&prefix)).await?;
        match keys.last() {
            Some(key) => match self.backend.get(key).await? {
                Some(blob) => Ok(Some(decode_op(&blob)?)),
                None => Ok(None),
            },
            None => Ok(None),
        }
    }

    async fn get_latest_entry_tx(
        &self,
        author: &VerifyingKey,
        log_id: &L,
    ) -> Result<Option<Operation<E>>, StoreError> {
        self.get_latest_entry(author, log_id).await
    }

    async fn get_log_heights(
        &self,
        author: &VerifyingKey,
        logs: &[L],
    ) -> Result<Option<BTreeMap<L, u32>>, StoreError> {
        let mut heights = BTreeMap::new();
        for log_id in logs {
            let prefix = log_prefix(author, log_id)?;
            let keys = self.backend.scan(&prefix, &scan_end(&prefix)).await?;
            if let Some(key) = keys.last() {
                heights.insert(log_id.clone(), seq_from_key(key, &prefix)?);
            }
        }
        if heights.is_empty() {
            Ok(None)
        } else {
            Ok(Some(heights))
        }
    }

    async fn get_log_size(
        &self,
        author: &VerifyingKey,
        log_id: &L,
        after: Option<u32>,
        until: Option<u32>,
    ) -> Result<Option<(u32, u32)>, StoreError> {
        let prefix = log_prefix(author, log_id)?;
        let keys = self.backend.scan(&prefix, &scan_end(&prefix)).await?;
        let mut count: u32 = 0;
        let mut bytes: u32 = 0;
        for key in &keys {
            if !in_range(seq_from_key(key, &prefix)?, after, until) {
                continue;
            }
            if let Some(blob) = self.backend.get(key).await? {
                let (header, _) = decode_blob::<E>(&blob[..])?;
                bytes += header.encode().len() as u32 + header.payload_size;
                count += 1;
            }
        }
        Ok(Some((count, bytes)))
    }

    fn log_entries(
        &self,
        author: &VerifyingKey,
        log_id: &L,
        after: Option<u32>,
        until: Option<u32>,
    ) -> Result<BoxStream<'static, Result<LogEntry<Operation<E>, L>, StoreError>>, StoreError> {
        let store = self.clone();
        let author = *author;
        let log_id = log_id.clone();
        let stream = stream::once(async move {
            let prefix = log_prefix(&author, &log_id)?;
            let keys = store.backend.scan(&prefix, &scan_end(&prefix)).await?;
            Ok::<_, StoreError>((store, prefix, log_id, keys))
        })
        .flat_map(move |result| {
            let stream: BoxStream<'static, Result<LogEntry<Operation<E>, L>, StoreError>> =
                match result {
                    Ok((store, prefix, log_id, keys)) => {
                        Box::pin(stream::iter(keys).filter_map(move |key| {
                            let store = store.clone();
                            let prefix = prefix.clone();
                            let log_id = log_id.clone();
                            async move {
                                let result: Result<Option<LogEntry<Operation<E>, L>>, StoreError> =
                                    async move {
                                        if !in_range(seq_from_key(&key, &prefix)?, after, until) {
                                            return Ok(None);
                                        }
                                        let blob =
                                            store.backend.get(&key).await?.ok_or_else(|| {
                                                codec("log entry disappeared during scan")
                                            })?;
                                        let op = decode_op::<E>(&blob)?;
                                        Ok(Some(LogEntry {
                                            bytes: op.header.encode(),
                                            entry: op,
                                            log_id,
                                        }))
                                    }
                                    .await;
                                match result {
                                    Ok(item) => item.map(Ok),
                                    Err(error) => Some(Err(error)),
                                }
                            }
                        }))
                    }
                    Err(err) => Box::pin(stream::once(async move { Err(err) })),
                };
            stream
        });
        Ok(Box::pin(stream))
    }

    async fn prune_entries(
        &self,
        author: &VerifyingKey,
        log_id: &L,
        until: &u32,
    ) -> Result<u64, StoreError> {
        let (writes, pruned) = self.prefix_prune_writes(author, log_id, *until).await?;
        self.backend.apply(&writes).await?;
        Ok(pruned)
    }
}

/// Any-operation view required by p2panda-net 0.7.3's LogSync. The typed
/// implementation above remains for domain readers that need their extension
/// type; both views decode the same stored header/body bytes.
impl<B, E, L> LogStore<AnyOperation, VerifyingKey, L, u32, Hash> for MunimentStore<B, E>
where
    B: Backend + Clone + Send + 'static,
    E: Extensions + 'static,
    L: LogId + Send + 'static,
{
    type Error = StoreError;

    async fn get_latest_entry(
        &self,
        author: &VerifyingKey,
        log_id: &L,
    ) -> Result<Option<AnyOperation>, StoreError> {
        let prefix = log_prefix(author, log_id)?;
        let keys = self.backend.scan(&prefix, &scan_end(&prefix)).await?;
        match keys.last() {
            Some(key) => self
                .backend
                .get(key)
                .await?
                .map(|blob| decode_any_op(&blob))
                .transpose(),
            None => Ok(None),
        }
    }

    async fn get_latest_entry_tx(
        &self,
        author: &VerifyingKey,
        log_id: &L,
    ) -> Result<Option<AnyOperation>, StoreError> {
        <Self as LogStore<AnyOperation, VerifyingKey, L, u32, Hash>>::get_latest_entry(
            self, author, log_id,
        )
        .await
    }

    async fn get_log_heights(
        &self,
        author: &VerifyingKey,
        logs: &[L],
    ) -> Result<Option<BTreeMap<L, u32>>, StoreError> {
        let mut heights = BTreeMap::new();
        for log_id in logs {
            let prefix = log_prefix(author, log_id)?;
            let keys = self.backend.scan(&prefix, &scan_end(&prefix)).await?;
            if let Some(key) = keys.last() {
                heights.insert(log_id.clone(), seq_from_key(key, &prefix)?);
            }
        }
        if heights.is_empty() {
            Ok(None)
        } else {
            Ok(Some(heights))
        }
    }

    async fn get_log_size(
        &self,
        author: &VerifyingKey,
        log_id: &L,
        after: Option<u32>,
        until: Option<u32>,
    ) -> Result<Option<(u32, u32)>, StoreError> {
        let prefix = log_prefix(author, log_id)?;
        let keys = self.backend.scan(&prefix, &scan_end(&prefix)).await?;
        let mut count = 0;
        let mut bytes = 0;
        for key in &keys {
            if !in_range(seq_from_key(key, &prefix)?, after, until) {
                continue;
            }
            if let Some(blob) = self.backend.get(key).await? {
                let op = decode_any_op(&blob)?;
                bytes += op.header.size() + op.header.payload_size;
                count += 1;
            }
        }
        Ok(Some((count, bytes)))
    }

    fn log_entries(
        &self,
        author: &VerifyingKey,
        log_id: &L,
        after: Option<u32>,
        until: Option<u32>,
    ) -> Result<BoxStream<'static, Result<LogEntry<AnyOperation, L>, StoreError>>, StoreError> {
        let store = self.clone();
        let author = *author;
        let log_id = log_id.clone();
        let stream = stream::once(async move {
            let prefix = log_prefix(&author, &log_id)?;
            let keys = store.backend.scan(&prefix, &scan_end(&prefix)).await?;
            Ok::<_, StoreError>((store, prefix, log_id, keys))
        })
        .flat_map(move |result| {
            let stream: BoxStream<'static, Result<LogEntry<AnyOperation, L>, StoreError>> =
                match result {
                    Ok((store, prefix, log_id, keys)) => {
                        Box::pin(stream::iter(keys).filter_map(move |key| {
                            let store = store.clone();
                            let prefix = prefix.clone();
                            let log_id = log_id.clone();
                            async move {
                                let result: Result<Option<LogEntry<AnyOperation, L>>, StoreError> =
                                    async move {
                                        if !in_range(seq_from_key(&key, &prefix)?, after, until) {
                                            return Ok(None);
                                        }
                                        let blob =
                                            store.backend.get(&key).await?.ok_or_else(|| {
                                                codec("log entry disappeared during scan")
                                            })?;
                                        let op = decode_any_op(&blob)?;
                                        Ok(Some(LogEntry {
                                            bytes: op.header.encode(),
                                            entry: op,
                                            log_id,
                                        }))
                                    }
                                    .await;
                                match result {
                                    Ok(item) => item.map(Ok),
                                    Err(error) => Some(Err(error)),
                                }
                            }
                        }))
                    }
                    Err(err) => Box::pin(stream::once(async move { Err(err) })),
                };
            stream
        });
        Ok(Box::pin(stream))
    }

    async fn prune_entries(
        &self,
        author: &VerifyingKey,
        log_id: &L,
        until: &u32,
    ) -> Result<u64, StoreError> {
        let (writes, pruned) = self.prefix_prune_writes(author, log_id, *until).await?;
        self.backend.apply(&writes).await?;
        Ok(pruned)
    }
}
