// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// The `op/<hash>` pointer key for an operation id.
pub(super) fn op_ptr(hash: &Hash) -> String {
    format!("op/{}", hash.to_hex())
}

/// The log id key segment: `hex(CBOR(log_id))`, canonical for a given value.
pub(super) fn log_seg<L: LogId>(log_id: &L) -> Result<String, StoreError> {
    let bytes = encode_cbor(log_id).map_err(codec)?;
    Ok(hex::encode(bytes))
}

/// The `log/<author>/<log>/` prefix a single log's entries share.
pub(super) fn log_prefix<L: LogId>(
    author: &VerifyingKey,
    log_id: &L,
) -> Result<String, StoreError> {
    Ok(format!("log/{}/{}/", author.to_hex(), log_seg(log_id)?))
}

/// An exclusive upper bound for scanning a log prefix. Sequence suffixes are
/// lowercase hex (`0`..`f`); `g` sorts above them all and below any longer
/// sibling prefix (whose next char would be a hex digit, not `/`).
pub(super) fn scan_end(prefix: &str) -> String {
    format!("{prefix}g")
}

/// Parse the sequence number back out of a `log/.../<seq>` key.
pub(super) fn seq_from_key(key: &str, prefix: &str) -> Result<u32, StoreError> {
    let seq = key
        .strip_prefix(prefix)
        .ok_or_else(|| StoreError::Codec("log key missing its prefix".into()))?;
    u32::from_str_radix(seq, 16).map_err(codec)
}

/// The `topic/<topic>/<author>/<log>` association key.
pub(super) fn topic_key<L: LogId>(
    topic: &Topic,
    author: &VerifyingKey,
    log_id: &L,
) -> Result<String, StoreError> {
    Ok(format!(
        "topic/{}/{}/{}",
        topic.to_hex(),
        author.to_hex(),
        log_seg(log_id)?
    ))
}

pub(super) fn payload_ref_prefix(payload_hash: &Hash) -> String {
    format!("payload-ref/{}/", payload_hash.to_hex())
}

pub(super) fn payload_ref_key(payload_hash: &Hash, operation_hash: &Hash) -> String {
    format!(
        "{}{}",
        payload_ref_prefix(payload_hash),
        operation_hash.to_hex()
    )
}

pub(crate) fn pending_payload_key(payload_hash: &Hash) -> String {
    format!("pending-payload/{}", payload_hash.to_hex())
}

/// Whether `seq` falls in the half-open sync range. `after` is exclusive (and,
/// when absent, includes sequence 0); `until` is inclusive.
pub(super) fn in_range(seq: u32, after: Option<u32>, until: Option<u32>) -> bool {
    after.is_none_or(|a| seq > a) && until.is_none_or(|u| seq <= u)
}

pub(super) fn codec(err: impl std::fmt::Display) -> StoreError {
    StoreError::Codec(err.to_string())
}
