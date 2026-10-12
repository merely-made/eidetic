// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0
//! Explicit owner-scoped copies, separate from caches and hosting promises.
//! The host permits the copy and supplies its backend, limits and wallet sealer.
//! This module does not select permission, fetch, erase or publish anything.
use crate::seal::{PayloadSealer, is_private_lane, seal_marker, seal_payload_for_store};

#[cfg(not(target_arch = "wasm32"))]
pub trait CustodyStore: Store + Sync {}
#[cfg(not(target_arch = "wasm32"))]
impl<B: Store + Sync + ?Sized> CustodyStore for B {}
#[cfg(target_arch = "wasm32")]
pub trait CustodyStore: Store {}
#[cfg(target_arch = "wasm32")]
impl<B: Store + ?Sized> CustodyStore for B {}
use crate::{BlobManifest, BlobSource, Error, Hash, ManifestId, NoFetcher, Result, Store};
use muniment::WriteOp;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub max_payload_bytes: u64,
    /// Stored payload bytes for this owner, including encryption overhead.
    /// Manifest/receipt metadata and other store namespaces are outside this quota.
    pub max_stored_bytes: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetainedCopy {
    pub version: u32,
    pub owner: [u8; 32],
    pub manifest: ManifestId,
    pub policy_revision: [u8; 32],
    pub retained_at_ms: u64,
    pub payload_bytes: u64,
    pub stored_bytes: u64,
    pub stored_hash: Hash,
}
fn hex(bytes: [u8; 32]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn prefix(owner: [u8; 32]) -> String {
    format!("retained/{}/", hex(owner))
}
fn slot(owner: [u8; 32], id: ManifestId, part: &str) -> String {
    format!("{}{part}/{}", prefix(owner), hex(*id.0.as_bytes()))
}
fn verify(manifest: &BlobManifest, bytes: &[u8]) -> Result<()> {
    if manifest.manifest_version != BlobManifest::CURRENT_VERSION
        || manifest.id.0 != manifest.content_hash
        || Hash::of(bytes) != manifest.id.0
        || bytes.len() as u64 != manifest.byte_size
    {
        return Err(Error::new("Exact retained manifest, hash or size mismatch"));
    }
    Ok(())
}
/// Copy verified cleartext into one owner's namespace. Private payloads require
/// a wallet sealer, including when the input came from an unsealed encounter.
/// Quota, existing copy, body, manifest and receipt commit in one transaction.
/// Unsupported transactions refuse; no read-then-apply fallback exists.
pub async fn retain(
    store: &mut dyn CustodyStore,
    owner: [u8; 32],
    manifest: &BlobManifest,
    bytes: &[u8],
    limits: Limits,
    policy_revision: [u8; 32],
    at_ms: u64,
    sealer: Option<&dyn PayloadSealer>,
) -> Result<RetainedCopy> {
    verify(manifest, bytes)?;
    if manifest.byte_size > limits.max_payload_bytes {
        return Err(Error::new("Retention payload quota exceeded"));
    }
    if is_private_lane(manifest.privacy) && sealer.is_none() {
        return Err(Error::new("Private retention requires a wallet sealer"));
    }
    // Inputs describe cleartext. An existing seal marker cannot be carried over
    // while choosing a different local key or namespace.
    if seal_marker(manifest)?.is_some() {
        return Err(Error::new("Supply an unsealed copy manifest"));
    }
    let prior = read(store, owner, manifest.id, sealer).await?;
    let mut local = manifest.clone();
    let stored = seal_payload_for_store(sealer, &mut local, bytes)?;
    let id = manifest.id;
    let body_key = slot(owner, id, "body");
    local.sources = vec![BlobSource::Local {
        key: body_key.clone(),
    }];
    let receipt = RetainedCopy {
        version: 1,
        owner,
        manifest: id,
        policy_revision,
        retained_at_ms: at_ms,
        payload_bytes: manifest.byte_size,
        stored_bytes: stored.len() as u64,
        stored_hash: Hash::of(&stored),
    };
    let receipt_key = slot(owner, id, "copy");
    let manifest_key = slot(owner, id, "manifest");
    let receipt_prefix = format!("{}copy/", prefix(owner));
    let encoded_manifest = serde_json::to_vec(&local).map_err(|e| Error::new(e.to_string()))?;
    let encoded_receipt = serde_json::to_vec(&receipt).map_err(|e| Error::new(e.to_string()))?;
    let result = Arc::new(Mutex::new(Err(
        "Retention transaction did not run".to_string()
    )));
    let result_in = result.clone();
    store
        .transact(Box::new(move |reader| {
            let checked = (|| -> std::result::Result<(RetainedCopy, Vec<WriteOp>), String> {
                let mut used = 0u64;
                let mut existing = None;
                for key in reader.list(&receipt_prefix).map_err(|e| e.to_string())? {
                    let raw = reader
                        .get(&key)
                        .map_err(|e| e.to_string())?
                        .ok_or("Missing copy receipt")?;
                    let held: RetainedCopy =
                        serde_json::from_slice(&raw).map_err(|e| e.to_string())?;
                    if held.version != 1
                        || held.owner != owner
                        || key != slot(owner, held.manifest, "copy")
                    {
                        return Err("Malformed owner copy ledger".into());
                    }
                    let body = reader
                        .get(&slot(owner, held.manifest, "body"))
                        .map_err(|e| e.to_string())?
                        .ok_or("Retained payload missing")?;
                    if body.len() as u64 != held.stored_bytes || Hash::of(&body) != held.stored_hash
                    {
                        return Err("Retained accounting or stored integrity mismatch".into());
                    }
                    used = used
                        .checked_add(held.stored_bytes)
                        .ok_or("Retention accounting overflow")?;
                    if held.manifest == id {
                        existing = Some(held);
                    }
                }
                if let Some(held) = existing {
                    // Do not rotate a seal, rewrite evidence or charge the same copy twice.
                    if used > limits.max_stored_bytes {
                        return Err("Retention stored quota exceeded".into());
                    }
                    let Some((verified, verified_manifest, _)) = &prior else {
                        return Err("Copy changed during retention; retry".into());
                    };
                    if verified != &held
                        || reader.get(&manifest_key).map_err(|e| e.to_string())?
                            != Some(
                                serde_json::to_vec(verified_manifest).map_err(|e| e.to_string())?,
                            )
                    {
                        return Err("Copy changed during retention; retry".into());
                    }
                    return Ok((held, vec![]));
                }
                if used
                    .checked_add(receipt.stored_bytes)
                    .is_none_or(|n| n > limits.max_stored_bytes)
                {
                    return Err("Retention stored quota exceeded".into());
                }
                Ok((
                    receipt,
                    vec![
                        WriteOp::Put {
                            key: body_key,
                            value: stored,
                        },
                        WriteOp::Put {
                            key: manifest_key,
                            value: encoded_manifest,
                        },
                        WriteOp::Put {
                            key: receipt_key,
                            value: encoded_receipt,
                        },
                    ],
                ))
            })();
            match checked {
                Ok((held, writes)) => {
                    *result_in.lock().unwrap() = Ok(held);
                    writes
                },
                Err(reason) => {
                    *result_in.lock().unwrap() = Err(reason);
                    vec![]
                },
            }
        }))
        .await
        .map_err(|e| Error::new(e.to_string()))?;
    let outcome = result.lock().unwrap().clone();
    outcome.map_err(Error::new)
}
/// Resolve an actual held copy. A receipt without verified bytes is not custody.
pub async fn read(
    store: &mut dyn Store,
    owner: [u8; 32],
    id: ManifestId,
    sealer: Option<&dyn PayloadSealer>,
) -> Result<Option<(RetainedCopy, BlobManifest, Vec<u8>)>> {
    let Some(raw) = store.get(&slot(owner, id, "copy")).await? else {
        return Ok(None);
    };
    let held: RetainedCopy = serde_json::from_slice(&raw).map_err(|e| Error::new(e.to_string()))?;
    if held.version != 1 || held.owner != owner || held.manifest != id {
        return Err(Error::new("Malformed retained copy"));
    }
    let raw = store
        .get(&slot(owner, id, "manifest"))
        .await?
        .ok_or_else(|| Error::new("Retained manifest missing"))?;
    let manifest: BlobManifest =
        serde_json::from_slice(&raw).map_err(|e| Error::new(e.to_string()))?;
    if manifest.id != id
        || manifest.sources
            != vec![BlobSource::Local {
                key: slot(owner, id, "body"),
            }]
    {
        return Err(Error::new("Retained source identity mismatch"));
    }
    if is_private_lane(manifest.privacy) && seal_marker(&manifest)?.is_none() {
        return Err(Error::new("Private retained payload is not sealed"));
    }
    if !is_private_lane(manifest.privacy) && seal_marker(&manifest)?.is_some() {
        return Err(Error::new("Retained privacy/seal metadata mismatch"));
    }
    let stored = store
        .get(&slot(owner, id, "body"))
        .await?
        .ok_or_else(|| Error::new("Retained payload missing"))?;
    if stored.len() as u64 != held.stored_bytes || Hash::of(&stored) != held.stored_hash {
        return Err(Error::new("Retained stored integrity mismatch"));
    }
    let bytes = crate::seal::resolve_sealed_blob(store, &mut NoFetcher, sealer, &manifest).await?;
    verify(&manifest, &bytes)?;
    if held.payload_bytes != bytes.len() as u64 {
        return Err(Error::new("Retained receipt size mismatch"));
    }
    Ok(Some((held, manifest, bytes)))
}

#[cfg(test)]
#[path = "retention_tests.rs"]
mod tests;
