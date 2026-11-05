// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl<B, E, L> TopicStore<Topic, VerifyingKey, L> for MunimentStore<B, E>
where
    B: Backend,
    E: Extensions,
    L: LogId,
{
    type Error = StoreError;

    async fn associate_tx(
        &self,
        topic: &Topic,
        author: &VerifyingKey,
        data_id: &L,
    ) -> Result<bool, StoreError> {
        let key = topic_key(topic, author, data_id)?;
        if self.backend.get(&key).await?.is_some() {
            return Ok(false);
        }
        self.backend.put(&key, b"").await?;
        Ok(true)
    }

    async fn remove_tx(
        &self,
        topic: &Topic,
        author: &VerifyingKey,
        data_id: &L,
    ) -> Result<bool, StoreError> {
        let key = topic_key(topic, author, data_id)?;
        if self.backend.get(&key).await?.is_none() {
            return Ok(false);
        }
        self.backend.delete(&key).await?;
        Ok(true)
    }

    async fn resolve(&self, topic: &Topic) -> Result<BTreeMap<VerifyingKey, Vec<L>>, StoreError> {
        let prefix = format!("topic/{}/", topic.to_hex());
        let keys = self.backend.list(&prefix).await?;
        let mut out: BTreeMap<VerifyingKey, Vec<L>> = BTreeMap::new();
        for key in keys {
            let rest = key
                .strip_prefix(&prefix)
                .ok_or_else(|| StoreError::Codec("topic key missing its prefix".into()))?;
            let (author_hex, log_hex) = rest
                .split_once('/')
                .ok_or_else(|| StoreError::Codec("malformed topic key".into()))?;
            let author = VerifyingKey::try_from(hex::decode(author_hex).map_err(codec)?.as_slice())
                .map_err(codec)?;
            let log_id: L =
                decode_cbor(&hex::decode(log_hex).map_err(codec)?[..]).map_err(codec)?;
            out.entry(author).or_default().push(log_id);
        }
        Ok(out)
    }

    async fn resolve_topics(
        &self,
        author: &VerifyingKey,
        data_id: &L,
    ) -> Result<Vec<Topic>, StoreError> {
        let keys = self.backend.list("topic/").await?;
        let mut topics = Vec::new();
        for key in keys {
            let Some(rest) = key.strip_prefix("topic/") else {
                continue;
            };
            let Some((topic_hex, rest)) = rest.split_once('/') else {
                continue;
            };
            let Some((author_hex, log_hex)) = rest.split_once('/') else {
                continue;
            };
            let key_author =
                VerifyingKey::try_from(hex::decode(author_hex).map_err(codec)?.as_slice())
                    .map_err(codec)?;
            let key_log: L =
                decode_cbor(&hex::decode(log_hex).map_err(codec)?[..]).map_err(codec)?;
            if &key_author != author || &key_log != data_id {
                continue;
            }
            let topic_bytes: [u8; 32] = hex::decode(topic_hex)
                .map_err(codec)?
                .try_into()
                .map_err(|_| codec("topic key has an invalid length"))?;
            let topic = Topic::from(topic_bytes);
            if !topics.contains(&topic) {
                topics.push(topic);
            }
        }
        Ok(topics)
    }

    async fn topics(&self) -> Result<Vec<Topic>, StoreError> {
        let keys = self.backend.list("topic/").await?;
        let mut topics = Vec::new();
        for key in keys {
            let Some(rest) = key.strip_prefix("topic/") else {
                continue;
            };
            let Some((topic_hex, _)) = rest.split_once('/') else {
                continue;
            };
            let topic_bytes: [u8; 32] = hex::decode(topic_hex)
                .map_err(codec)?
                .try_into()
                .map_err(|_| codec("topic key has an invalid length"))?;
            let topic = Topic::from(topic_bytes);
            if !topics.contains(&topic) {
                topics.push(topic);
            }
        }
        Ok(topics)
    }
}
