// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The stored forms: serde JSON (F125, F126, F131) and framed postcard
//! (F125).

use state_witness::{Entry, FrameError, Trace, Witness, hash_value};

const WITNESS_MAGIC: [u8; 8] = *b"MEREWTN\0";
const TRACE_MAGIC: [u8; 8] = *b"MERETRC\0";

fn witness() -> Witness {
    let mut w = Witness::new();
    w.insert("site:10", &1u64).unwrap();
    w.insert("site:9", &2u64).unwrap();
    w.insert_bytes("big", &[0xff; 3]).unwrap();
    w
}

fn trace() -> Trace {
    let mut t = Trace::new();
    t.push(4, witness()).unwrap();
    t.push(8, Witness::new()).unwrap();
    t
}

#[test]
fn the_json_shape_is_pinned() {
    let w = witness();
    let json = serde_json::to_string(&w).unwrap();
    let d = |l: &str| w.get(l).unwrap();
    assert_eq!(
        json,
        format!(
            r#"[{{"label":"big","digest":{}}},{{"label":"site:9","digest":{}}},{{"label":"site:10","digest":{}}}]"#,
            d("big"),
            d("site:9"),
            d("site:10")
        )
    );
    // Digests are u64 numbers (F126), past 2^53 included.
    assert!(w.entries().iter().any(|e| e.digest > 1 << 53));
    assert_eq!(serde_json::from_str::<Witness>(&json).unwrap(), w);
    let t = trace();
    let json = serde_json::to_string(&t).unwrap();
    assert!(json.starts_with(r#"[[4,[{"label":"big""#) && json.ends_with(r#"],[8,[]]]"#));
    assert_eq!(serde_json::from_str::<Trace>(&json).unwrap(), t);
}

#[test]
fn loading_refuses_bad_lists() {
    let e = |label: &str| Entry {
        label: label.into(),
        digest: hash_value(&0u8).unwrap(),
    };
    for (entries, why) in [
        (vec![e("site:10"), e("site:9")], "out of natural order"),
        (vec![e("site:9"), e("site:9")], "duplicate"),
        (vec![e("a\0")], "NUL"),
    ] {
        let json = serde_json::to_string(&entries).unwrap();
        assert!(
            serde_json::from_str::<Witness>(&json).is_err(),
            "JSON took a list {why}"
        );
        let framed = framing::frame(WITNESS_MAGIC, 1, &entries).unwrap();
        assert_eq!(
            Witness::from_framed(&framed),
            Err(FrameError::Malformed),
            "framed took a list {why}"
        );
    }
    let ticks = vec![(8u64, Witness::new()), (4, Witness::new())];
    assert!(serde_json::from_str::<Trace>(&serde_json::to_string(&ticks).unwrap()).is_err());
    let framed = framing::frame(TRACE_MAGIC, 1, &ticks).unwrap();
    assert_eq!(Trace::from_framed(&framed), Err(FrameError::Malformed));
}

#[test]
fn framed_records_round_trip_in_wing_formats_layout() {
    let (w, t) = (witness(), trace());
    let bytes = w.to_framed().unwrap();
    assert_eq!(
        bytes,
        [
            &WITNESS_MAGIC[..],
            &[1, 0],
            &postcard::to_allocvec(&w).unwrap()
        ]
        .concat()
    );
    assert_eq!(Witness::from_framed(&bytes).unwrap(), w);
    let bytes = t.to_framed().unwrap();
    assert_eq!(&bytes[..10], [&TRACE_MAGIC[..], &[1, 0]].concat());
    assert_eq!(Trace::from_framed(&bytes).unwrap(), t);
}

#[test]
fn wrong_magic_and_newer_versions_are_refused() {
    let w = witness().to_framed().unwrap();
    let t = trace().to_framed().unwrap();
    assert_eq!(
        Trace::from_framed(&w),
        Err(FrameError::WrongSchema {
            found: WITNESS_MAGIC,
            expected: TRACE_MAGIC
        })
    );
    assert_eq!(
        Witness::from_framed(&t),
        Err(FrameError::WrongSchema {
            found: TRACE_MAGIC,
            expected: WITNESS_MAGIC
        })
    );
    let mut newer = w.clone();
    newer[8..10].copy_from_slice(&2u16.to_le_bytes());
    assert_eq!(
        Witness::from_framed(&newer),
        Err(FrameError::UnknownVersion {
            found: 2,
            expected: 1
        })
    );
    let mut newer = t.clone();
    newer[8..10].copy_from_slice(&2u16.to_le_bytes());
    assert_eq!(
        Trace::from_framed(&newer),
        Err(FrameError::UnknownVersion {
            found: 2,
            expected: 1
        })
    );
    // An unframed payload is not taken as a record either.
    let bare = postcard::to_allocvec(&witness()).unwrap();
    assert!(matches!(
        Witness::from_framed(&bare),
        Err(FrameError::WrongSchema { .. } | FrameError::TooShort { .. })
    ));
}

/// framing's rule: a record's leading payload bytes must not spell its magic.
/// The closest a witness gets is 77 entries (`M`) whose first label has 69
/// bytes (`E`) and begins `REWTN`; the eighth byte would have to be a NUL,
/// which no label holds. A trace needs 77 ticks, tick 69, 82 entries and a
/// 69-byte label beginning `TRC`, and stops at the same NUL.
#[test]
fn payloads_cannot_spell_their_magic() {
    let mut w = Witness::new();
    w.insert_bytes(format!("REWTN{}", "x".repeat(64)), b"")
        .unwrap();
    for i in 0..76 {
        w.insert_bytes(format!("z{i}"), b"").unwrap();
    }
    let payload = postcard::to_allocvec(&w).unwrap();
    assert_eq!(&payload[..7], &WITNESS_MAGIC[..7]);
    assert_ne!(payload[..8], WITNESS_MAGIC);
    assert!(w.insert_bytes("REWTN\0", b"").is_err());

    let mut first = Witness::new();
    first
        .insert_bytes(format!("TRC{}", "x".repeat(66)), b"")
        .unwrap();
    for i in 0..81 {
        first.insert_bytes(format!("z{i}"), b"").unwrap();
    }
    let mut t = Trace::new();
    t.push(69, first).unwrap();
    for tick in 70..146 {
        t.push(tick, Witness::new()).unwrap();
    }
    let payload = postcard::to_allocvec(&t).unwrap();
    assert_eq!(&payload[..7], &TRACE_MAGIC[..7]);
    assert_ne!(payload[..8], TRACE_MAGIC);
}
