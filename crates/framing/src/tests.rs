// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;

const MAGIC: [u8; 8] = *b"TESTWIRE";
const OTHER: [u8; 8] = *b"OTHERSCH";

#[test]
fn framed_values_round_trip() {
    let bytes = frame(MAGIC, 0, &(7u32, "hello".to_string())).unwrap();
    let back: (u32, String) = unframe(MAGIC, 0, &bytes).unwrap();
    assert_eq!(back, (7, "hello".to_string()));
}

#[test]
fn the_layout_is_wing_formats() {
    // Magic, little-endian version, then the postcard payload: the bytes
    // wing-formats writes, so its records read here unchanged.
    let bytes = frame(MAGIC, 3, &7u32).unwrap();
    assert_eq!(bytes, [&MAGIC[..], &[3, 0], &[7]].concat());
}

#[test]
fn a_version_mismatch_is_refused_with_its_number() {
    let mut bytes = MAGIC.to_vec();
    bytes.extend_from_slice(&9u16.to_le_bytes());
    bytes.extend_from_slice(&[0xff; 24]);
    assert_eq!(
        unframe::<u32>(MAGIC, 0, &bytes),
        Err(FrameError::UnknownVersion {
            found: 9,
            expected: 0,
        })
    );
}

#[test]
fn a_truncated_header_is_refused() {
    assert_eq!(
        unframe::<u32>(MAGIC, 0, b"TEST"),
        Err(FrameError::TooShort { got: 4 })
    );
    // The legacy path too: the magic with no version after it is a cut-off
    // header, not an old payload.
    assert_eq!(
        unframe_or_legacy::<u32>(MAGIC, 0, &[&MAGIC[..], &[1]].concat()),
        Err(FrameError::TooShort { got: 9 })
    );
}

#[test]
fn another_schema_is_named_rather_than_mis_decoded() {
    let bytes = frame(OTHER, 0, &1u32).unwrap();
    assert_eq!(
        unframe::<u32>(MAGIC, 0, &bytes),
        Err(FrameError::WrongSchema {
            found: OTHER,
            expected: MAGIC,
        })
    );
}

#[test]
fn a_truncated_payload_is_malformed_not_a_version_problem() {
    let bytes = frame(MAGIC, 0, &"a reasonably long string".to_string()).unwrap();
    assert_eq!(
        unframe::<String>(MAGIC, 0, &bytes[..HEADER_LEN + 2]),
        Err(FrameError::Malformed)
    );
}

#[test]
fn trailing_bytes_are_tolerated_as_wing_formats_tolerates_them() {
    let mut bytes = frame(MAGIC, 0, &7u32).unwrap();
    bytes.push(0);
    assert_eq!(unframe::<u32>(MAGIC, 0, &bytes), Ok(7));
}

#[test]
fn peek_reads_the_version_without_decoding_the_payload() {
    // A payload no type could decode: peek must not try.
    let mut bytes = MAGIC.to_vec();
    bytes.extend_from_slice(&3u16.to_le_bytes());
    bytes.extend_from_slice(&[0xff; 16]);
    assert_eq!(peek(MAGIC, &bytes), Ok(3));
    assert_eq!(
        peek(OTHER, &bytes),
        Err(FrameError::WrongSchema {
            found: MAGIC,
            expected: OTHER,
        })
    );
}

#[test]
fn unheadered_bytes_read_as_version_zero() {
    let old = postcard::to_allocvec(&(7u32, "hello".to_string())).unwrap();
    assert_eq!(split_or_legacy(MAGIC, &old), Ok((0, &old[..])));
    let back: (u32, String) = unframe_or_legacy(MAGIC, 0, &old).unwrap();
    assert_eq!(back, (7, "hello".to_string()));
    // Shorter than a header is still a payload when it lacks the magic.
    assert_eq!(unframe_or_legacy::<u32>(MAGIC, 0, &[7]), Ok(7));
    // Framed bytes take the framed path.
    let framed = frame(MAGIC, 0, &(7u32, "hello".to_string())).unwrap();
    assert_eq!(unframe_or_legacy(MAGIC, 0, &framed), Ok(back));
}

#[test]
fn a_later_reader_refuses_legacy_bytes_as_version_zero() {
    // So the caller knows to decode the old shape and migrate it.
    let old = postcard::to_allocvec(&7u32).unwrap();
    assert_eq!(
        unframe_or_legacy::<u32>(MAGIC, 1, &old),
        Err(FrameError::UnknownVersion {
            found: 0,
            expected: 1,
        })
    );
}

#[test]
fn the_documented_limit_holds() {
    // An unheadered payload that happens to begin with the magic is read as
    // framed. This pins the limit the crate docs state; each record's own
    // test pins that its leading fields cannot spell its magic.
    let mut old = MAGIC.to_vec();
    old.extend_from_slice(&[0, 0, 7]);
    assert_eq!(split_or_legacy(MAGIC, &old), Ok((0, &[7][..])));
}
