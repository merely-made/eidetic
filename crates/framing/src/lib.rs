// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! A version header for postcard records that are stored or sent.
//!
//! postcard carries no field names, so a record without a version cannot
//! change shape without breaking the bytes already written. Each record kind
//! picks an 8-byte magic; [`frame`] writes the magic and a little-endian `u16`
//! version before the postcard payload, [`unframe`] checks both before
//! decoding, and [`peek`] reads the version without decoding.
//!
//! The layout is wing-formats', byte for byte (data formats brief, F6), so a
//! record wing-formats framed reads here unchanged.
//!
//! # Records written before their header
//!
//! [`unframe_or_legacy`] and [`split_or_legacy`] read bytes that do not begin
//! with the record's magic as an unheadered version-0 payload, so a record
//! kind can gain its header without losing what it already wrote. The limit:
//! an unheadered payload whose first 8 bytes equal the magic is read as
//! framed. Choose each magic so the record's leading fields cannot spell it,
//! and pin that with a test beside the record.

use serde::{Serialize, de::DeserializeOwned};

/// Magic plus a little-endian `u16` version.
pub const HEADER_LEN: usize = 10;

/// Why framed bytes were refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FrameError {
    /// Fewer bytes than a header.
    TooShort { got: usize },
    /// Another record kind's magic.
    WrongSchema { found: [u8; 8], expected: [u8; 8] },
    /// A version this reader does not decode.
    UnknownVersion { found: u16, expected: u16 },
    /// The payload did not decode.
    Malformed,
    /// The value did not encode.
    Encode,
}

/// Writes a postcard payload after its header.
pub fn frame<T: Serialize>(magic: [u8; 8], version: u16, value: &T) -> Result<Vec<u8>, FrameError> {
    let payload = postcard::to_allocvec(value).map_err(|_| FrameError::Encode)?;
    let mut bytes = Vec::with_capacity(HEADER_LEN + payload.len());
    bytes.extend_from_slice(&magic);
    bytes.extend_from_slice(&version.to_le_bytes());
    bytes.extend_from_slice(&payload);
    Ok(bytes)
}

/// Reads a framed payload, checking magic and version first.
pub fn unframe<T: DeserializeOwned>(
    magic: [u8; 8],
    version: u16,
    bytes: &[u8],
) -> Result<T, FrameError> {
    let (found, payload) = split(magic, bytes)?;
    decode(found, version, payload)
}

/// Reads a header's version without decoding its payload.
pub fn peek(magic: [u8; 8], bytes: &[u8]) -> Result<u16, FrameError> {
    split(magic, bytes).map(|(version, _)| version)
}

/// The version and payload of framed bytes.
pub fn split(magic: [u8; 8], bytes: &[u8]) -> Result<(u16, &[u8]), FrameError> {
    if bytes.len() < HEADER_LEN {
        return Err(FrameError::TooShort { got: bytes.len() });
    }
    let found: [u8; 8] = bytes[..8].try_into().expect("checked header length");
    if found != magic {
        return Err(FrameError::WrongSchema {
            found,
            expected: magic,
        });
    }
    Ok((
        u16::from_le_bytes([bytes[8], bytes[9]]),
        &bytes[HEADER_LEN..],
    ))
}

/// The version and payload of a record that may predate its header: bytes
/// that do not begin with `magic` are an unheadered version-0 payload. Bytes
/// that begin with it but stop short of a version are refused.
pub fn split_or_legacy(magic: [u8; 8], bytes: &[u8]) -> Result<(u16, &[u8]), FrameError> {
    if bytes.starts_with(&magic) {
        split(magic, bytes)
    } else {
        Ok((0, bytes))
    }
}

/// Reads a record that may predate its header, at `version`. A version-0
/// reader takes legacy bytes; a later one refuses them as version 0, so the
/// caller can decode the old shape and migrate it.
pub fn unframe_or_legacy<T: DeserializeOwned>(
    magic: [u8; 8],
    version: u16,
    bytes: &[u8],
) -> Result<T, FrameError> {
    let (found, payload) = split_or_legacy(magic, bytes)?;
    decode(found, version, payload)
}

fn decode<T: DeserializeOwned>(found: u16, expected: u16, payload: &[u8]) -> Result<T, FrameError> {
    if found != expected {
        return Err(FrameError::UnknownVersion { found, expected });
    }
    // postcard decodes one value without requiring the bytes be exhausted,
    // as wing-formats' readers always have. Tightening that is a format
    // decision of its own.
    postcard::from_bytes(payload).map_err(|_| FrameError::Malformed)
}

#[cfg(test)]
mod tests;
