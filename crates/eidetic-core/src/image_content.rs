// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Content-addressed image bytes. Graph references, decoding and retention
//! decisions belong to the consuming adapter. Existing storage keys are stable.

use crate::{Hash, Result, Store};

const IMAGE_PREFIX: &str = "content/image/";

fn image_key(hex: &str) -> String {
    format!("{IMAGE_PREFIX}{hex}")
}

pub async fn save_image_bytes(store: &mut dyn Store, bytes: &[u8]) -> Result<Hash> {
    let hash = Hash::of(bytes);
    store.put(&image_key(&hash.to_hex()), bytes).await?;
    Ok(hash)
}

pub async fn load_image_bytes(store: &mut dyn Store, hex: &str) -> Result<Option<Vec<u8>>> {
    Ok(store.get(&image_key(hex)).await?)
}

/// Apply a caller-authorized removal. This function does not determine whether
/// an image is still referenced or choose a garbage-collection policy.
pub async fn delete_image_bytes(store: &mut dyn Store, hex: &str) -> Result<bool> {
    let key = image_key(hex);
    let existed = store.get(&key).await?.is_some();
    store.delete(&key).await?;
    Ok(existed)
}

pub async fn stored_image_hexes(store: &mut dyn Store) -> Result<Vec<String>> {
    Ok(store
        .list(IMAGE_PREFIX)
        .await?
        .into_iter()
        .filter_map(|key| key.strip_prefix(IMAGE_PREFIX).map(str::to_string))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MemoryBackend;

    #[test]
    fn existing_image_namespace_and_content_identity_are_preserved() {
        pollster::block_on(async {
            let mut store = MemoryBackend::new();
            let bytes = b"retained image bytes";
            let hash = save_image_bytes(&mut store, bytes).await.unwrap();
            assert_eq!(hash, save_image_bytes(&mut store, bytes).await.unwrap());
            assert_eq!(
                store.list(IMAGE_PREFIX).await.unwrap(),
                [image_key(&hash.to_hex())]
            );
            assert_eq!(
                store
                    .get(&image_key(&hash.to_hex()))
                    .await
                    .unwrap()
                    .as_deref(),
                Some(bytes.as_slice())
            );
            let mut reopened = store.clone();
            assert_eq!(
                load_image_bytes(&mut reopened, &hash.to_hex())
                    .await
                    .unwrap()
                    .as_deref(),
                Some(bytes.as_slice())
            );
            assert!(
                delete_image_bytes(&mut reopened, &hash.to_hex())
                    .await
                    .unwrap()
            );
            assert!(
                !delete_image_bytes(&mut reopened, &hash.to_hex())
                    .await
                    .unwrap()
            );
        });
    }
}
