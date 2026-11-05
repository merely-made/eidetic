// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Portable evidence: the signed operations of a log as one byte string.
//!
//! Framing only. Trust comes from [`verify_edition_chain`], never from the
//! bytes arriving by any route.

use p2panda_core::{Body, Header};

use super::verify_edition_chain;
use super::{EditionChain, EditionError, EditionExt, EditionLimits, EditionOperation};

const MAGIC: &[u8; 4] = b"SEV1";
const MAX_HEADER_BYTES: usize = 4096;

/// Frame operations for storage or carriage.
pub fn encode_evidence(operations: &[EditionOperation]) -> Vec<u8> {
    let mut out = MAGIC.to_vec();
    out.extend_from_slice(&(operations.len() as u32).to_be_bytes());
    for operation in operations {
        let header = operation.header.encode();
        let body = operation
            .body
            .as_ref()
            .map(Body::to_bytes)
            .unwrap_or_default();
        out.extend_from_slice(&(header.len() as u32).to_be_bytes());
        out.extend_from_slice(&header);
        out.extend_from_slice(&(body.len() as u32).to_be_bytes());
        out.extend_from_slice(&body);
    }
    out
}

/// Unframe operations, bounded by `limits`. The result is unverified.
///
/// # Errors
/// Refuses truncated, oversized, trailing or non-canonical input.
pub fn decode_evidence(
    bytes: &[u8],
    limits: EditionLimits,
) -> Result<Vec<EditionOperation>, EditionError> {
    let refuse = |detail: &str| EditionError::Evidence(detail.into());
    let mut cursor = bytes;
    let mut take = |len: usize| -> Result<&[u8], EditionError> {
        if cursor.len() < len {
            return Err(refuse("truncated"));
        }
        let (head, rest) = cursor.split_at(len);
        cursor = rest;
        Ok(head)
    };
    if take(4)? != MAGIC {
        return Err(refuse("unrecognised framing"));
    }
    let count = u32::from_be_bytes(take(4)?.try_into().expect("four bytes")) as usize;
    if count > limits.max_editions {
        return Err(refuse("too many editions"));
    }
    let mut operations = Vec::new();
    for _ in 0..count {
        let header_len = u32::from_be_bytes(take(4)?.try_into().expect("four bytes")) as usize;
        if header_len > MAX_HEADER_BYTES {
            return Err(refuse("header too large"));
        }
        let header_bytes = take(header_len)?.to_vec();
        let body_len = u32::from_be_bytes(take(4)?.try_into().expect("four bytes")) as usize;
        if body_len > limits.max_body_bytes {
            return Err(refuse("body too large"));
        }
        let body = take(body_len)?.to_vec();
        let header: Header<EditionExt> =
            Header::decode(&header_bytes[..]).map_err(|e| EditionError::Evidence(e.to_string()))?;
        if header.encode() != header_bytes {
            return Err(refuse("header is not canonically encoded"));
        }
        operations.push(EditionOperation {
            hash: header.hash(),
            header,
            body: (!body.is_empty()).then(|| Body::from(body)),
        });
    }
    if !cursor.is_empty() {
        return Err(refuse("trailing bytes"));
    }
    Ok(operations)
}

/// Verify remembered and supplied evidence as one log. Older evidence merges
/// into the newer rather than replacing it; two editions at one position
/// refuse as equivocation.
///
/// # Errors
/// Any failure of decoding or [`verify_edition_chain`] over the union.
pub fn merge_evidence(
    key: &[u8; 32],
    remembered: Option<&[u8]>,
    supplied: &[u8],
    limits: EditionLimits,
) -> Result<EditionChain, EditionError> {
    let mut operations = match remembered {
        Some(bytes) => decode_evidence(bytes, limits)?,
        None => Vec::new(),
    };
    operations.extend(decode_evidence(supplied, limits)?);
    verify_edition_chain(key, &operations, limits)
}
