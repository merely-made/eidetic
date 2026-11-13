// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Declared-lens §3.1 metrics for the `mere.pack/2` header: byte-identity round
//! trips, one refusal per RFC 8949 §4.2.1 requirement and per D5 rule, the
//! field checks of §3.2's table, and known-answer digests under both functions.

use std::collections::BTreeMap;

use ciborium::value::Value;
use serde::{Deserialize, Serialize};

use super::*;
use crate::schema::{Hash, HashFn};

pub(super) fn fixture() -> PackHeader {
    let mut body = BTreeMap::new();
    let ns = "https://mere.computer/ns/";
    let vocabulary = VocabularyBody {
        terms: vec![Term {
            iri: format!("{ns}rel#cites"),
            kind: TermKind::Predicate,
            family: Some(format!("{ns}family#semantic")),
            handler: None,
            axes: BTreeMap::from([(
                format!("{ns}axis#stratum"),
                vec![format!("{ns}stratum#resource")],
            )]),
            labels: BTreeMap::from([("en".to_string(), "cites".to_string())]),
        }],
        defaults: BTreeMap::from([(format!("{ns}axis#stratum"), format!("{ns}stratum#resource"))]),
    };
    body.insert(PackKind::Vocabulary, vocabulary.to_value().unwrap());
    body.insert(PackKind::Participant, BodyValue::Bytes(vec![0, 1, 2]));
    PackHeader {
        format: PACK2_FORMAT.to_string(),
        id: "https://mere.computer/packs/core".to_string(),
        version: "1.2.3-rc.1+build.5".to_string(),
        kind: vec![PackKind::Vocabulary, PackKind::Participant],
        licence: "MPL-2.0 OR Apache-2.0".to_string(),
        credit: vec!["Mark".to_string(), "A Contributor".to_string()],
        requires: vec![
            Requirement::Pack {
                pack: "https://mere.computer/packs/rdfs".to_string(),
                version: "0.1.0".to_string(),
            },
            Requirement::Capability {
                capability: "https://mere.computer/ns/cap/clock".to_string(),
            },
        ],
        provides: vec![
            "https://mere.computer/ns/".to_string(),
            "https://example.org/a#".to_string(),
        ],
        contributes: vec![Contribution {
            kind: ContributionKind::Presentation,
            id: "https://mere.computer/ns/reader/note".to_string(),
        }],
        scopes: vec!["scenario/".to_string(), "app/".to_string()],
        parts: vec![Part {
            name: "main.wasm".to_string(),
            role: "wasm-component".to_string(),
            media: "application/wasm".to_string(),
            digest: Multihash(Hash::of(b"\0asm")),
            bytes: 4,
        }],
        body,
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn raw(value: &Value) -> Vec<u8> {
    let mut out = Vec::new();
    ciborium::ser::into_writer(value, &mut out).unwrap();
    out
}

/// The fixture as a raw ciborium map, in canonical order, to tamper with.
fn fixture_map() -> Vec<(Value, Value)> {
    let bytes = fixture().to_canonical().unwrap();
    let value: Value = ciborium::de::from_reader(bytes.as_slice()).unwrap();
    value.into_map().unwrap()
}

fn entry<'a>(map: &'a mut [(Value, Value)], key: &str) -> &'a mut Value {
    &mut map
        .iter_mut()
        .find(|(k, _)| k.as_text() == Some(key))
        .unwrap()
        .1
}

#[test]
fn a_header_round_trips_byte_identical() {
    let header = fixture();
    let bytes = header.to_canonical().unwrap();
    let decoded = PackHeader::from_canonical(&bytes).unwrap();
    assert_eq!(
        decoded.to_canonical().unwrap(),
        bytes,
        "re-encoded bytes are equal"
    );
    // Sets come back sorted; everything else as written.
    assert_eq!(
        decoded.credit, header.credit,
        "credit keeps the author's order"
    );
    assert_eq!(decoded.body, header.body);
    let mut provides = header.provides.clone();
    provides.sort_by_key(|p| to_canonical(p).unwrap());
    assert_eq!(decoded.provides, provides);
}

#[test]
fn top_level_keys_are_in_canonical_order() {
    let keys: Vec<String> = fixture_map()
        .into_iter()
        .map(|(k, _)| k.into_text().unwrap())
        .collect();
    assert_eq!(
        keys,
        [
            "id",
            "body",
            "kind",
            "parts",
            "credit",
            "format",
            "scopes",
            "licence",
            "version",
            "provides",
            "requires",
            "contributes"
        ],
        "RFC 8949 §4.2.1: shorter keys first, then bytewise"
    );
}

#[test]
fn set_order_does_not_change_the_bytes() {
    let base = fixture().to_canonical().unwrap();
    let provides = fixture().provides;
    let mut more = provides.clone();
    more.extend(["https://a.example/".to_string(), "urn:x:y".to_string()]);
    let mut grown = fixture();
    grown.provides = more.clone();
    let expected = grown.to_canonical().unwrap();
    // Every permutation of the four provides, and every set reversed.
    let mut count = 0;
    permute(&mut more, 0, &mut |order| {
        let mut h = grown.clone();
        h.provides = order.to_vec();
        h.scopes.reverse();
        h.requires.reverse();
        h.kind.reverse();
        assert_eq!(h.to_canonical().unwrap(), expected);
        count += 1;
    });
    assert_eq!(count, 24);
    let mut h = fixture();
    h.scopes.reverse();
    assert_eq!(h.to_canonical().unwrap(), base);
}

fn permute<T: Clone>(items: &mut [T], k: usize, f: &mut impl FnMut(&[T])) {
    if k == items.len() {
        f(items);
        return;
    }
    for i in k..items.len() {
        items.swap(k, i);
        permute(items, k + 1, f);
        items.swap(k, i);
    }
}

// ---- RFC 8949 §4.2.1 and D5 refusals, on a two-field probe -----------------

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Probe {
    a: u64,
    bb: Vec<String>,
}

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}

#[test]
fn probe_controls() {
    // Positive control: the hand-written canonical bytes decode and re-encode.
    let canonical = unhex("a2616101626262816178");
    let probe: Probe = from_canonical(&canonical).unwrap();
    assert_eq!(
        probe,
        Probe {
            a: 1,
            bb: vec!["x".to_string()]
        }
    );
    assert_eq!(to_canonical(&probe).unwrap(), canonical);
}

type Expect = fn(&CanonicalError) -> bool;

#[test]
fn each_deterministic_encoding_rule_is_refused() {
    use CanonicalError::{Malformed, NotCanonical, TrailingBytes};
    let cases: [(&str, &str, Expect); 13] = [
        ("unsorted keys", "a2626262816178616101", |e| {
            *e == NotCanonical
        }),
        ("indefinite map", "bf616101626262816178ff", |e| {
            *e == NotCanonical
        }),
        ("indefinite array", "a26161016262629f6178ff", |e| {
            *e == NotCanonical
        }),
        ("indefinite text", "a2616101626262817f6178ff", |e| {
            *e == NotCanonical
        }),
        ("tag on a text", "a261610162626281d8206178", |e| {
            *e == NotCanonical
        }),
        ("bignum-tagged integer", "a26161c24101626262816178", |e| {
            *e == NotCanonical
        }),
        ("non-shortest integer", "a261611801626262816178", |e| {
            *e == NotCanonical
        }),
        ("non-shortest length", "a261610162626281780178", |e| {
            *e == NotCanonical
        }),
        ("float", "a26161f93c00626262816178", |e| {
            matches!(e, Malformed(_))
        }),
        ("duplicate key", "a3616101616101626262816178", |e| {
            matches!(e, Malformed(_))
        }),
        ("unknown key", "a3616101616301626262816178", |e| {
            matches!(e, Malformed(_))
        }),
        ("null", "a2616101626262f6", |e| matches!(e, Malformed(_))),
        ("trailing bytes", "a261610162626281617800", |e| {
            *e == TrailingBytes
        }),
    ];
    for (name, hexed, expected) in cases {
        let bytes = unhex(hexed);
        let err = from_canonical::<Probe>(&bytes).expect_err(name);
        assert!(expected(&err), "{name}: {err:?}");
    }
}

#[test]
fn the_encoder_refuses_what_the_profile_excludes() {
    let map = |k: Value, v: Value| Value::Map(vec![(k, v)]);
    let text = |s: &str| Value::Text(s.to_string());
    assert_eq!(
        to_canonical(&map(text("a"), Value::Float(1.0))),
        Err(CanonicalError::Float)
    );
    assert_eq!(
        to_canonical(&map(text("a"), Value::Tag(32, Box::new(text("x"))))),
        Err(CanonicalError::Tag)
    );
    assert_eq!(
        to_canonical(&map(text("a"), Value::Null)),
        Err(CanonicalError::Null)
    );
    assert_eq!(
        to_canonical(&map(Value::Integer(1.into()), text("x"))),
        Err(CanonicalError::NonTextKey)
    );
    let dup = Value::Map(vec![(text("a"), text("x")), (text("a"), text("y"))]);
    assert!(matches!(
        to_canonical(&dup),
        Err(CanonicalError::DuplicateKey(_))
    ));
}

// ---- the same refusals on a real header -------------------------------------

#[test]
fn header_level_refusals() {
    let canonical = fixture().to_canonical().unwrap();
    let not_canonical = |bytes: &[u8]| {
        matches!(
            PackHeader::from_canonical(bytes),
            Err(HeaderError::Canonical(CanonicalError::NotCanonical))
        )
    };
    let malformed = |bytes: &[u8]| {
        matches!(
            PackHeader::from_canonical(bytes),
            Err(HeaderError::Canonical(CanonicalError::Malformed(_)))
        )
    };

    let mut swapped = fixture_map();
    swapped.swap(0, 1);
    assert!(
        not_canonical(&raw(&Value::Map(swapped))),
        "non-canonical key order"
    );

    assert_eq!(canonical[0], 0xac, "a twelve-entry map");
    let indefinite = [&[0xbf][..], &canonical[1..], &[0xff]].concat();
    assert!(not_canonical(&indefinite), "an indefinite-length map");

    let mut tagged = fixture_map();
    let id = entry(&mut tagged, "id");
    *id = Value::Tag(32, Box::new(id.clone()));
    assert!(not_canonical(&raw(&Value::Map(tagged))), "a tag on the id");
    let self_described = [&[0xd9, 0xd9, 0xf7][..], &canonical].concat();
    assert!(
        not_canonical(&self_described),
        "a self-describe tag on the document"
    );

    let mut floated = fixture_map();
    let parts = entry(&mut floated, "parts").as_array_mut().unwrap();
    let part = parts[0].as_map_mut().unwrap();
    *entry(part, "bytes") = Value::Float(4.0);
    assert!(
        malformed(&raw(&Value::Map(floated))),
        "a float in parts.bytes"
    );

    let mut doubled = fixture_map();
    doubled.insert(
        1,
        (
            Value::Text("id".into()),
            Value::Text("https://x.example/".into()),
        ),
    );
    assert!(malformed(&raw(&Value::Map(doubled))), "a duplicate key");

    let trailing = [&canonical[..], &[0x00]].concat();
    assert_eq!(
        PackHeader::from_canonical(&trailing),
        Err(HeaderError::Canonical(CanonicalError::TrailingBytes))
    );

    let mut unsorted_set = fixture_map();
    entry(&mut unsorted_set, "scopes")
        .as_array_mut()
        .unwrap()
        .reverse();
    assert!(
        not_canonical(&raw(&Value::Map(unsorted_set))),
        "an unsorted set"
    );

    let mut float_body = fixture_map();
    let body = entry(&mut float_body, "body").as_map_mut().unwrap();
    *entry(body, "participant") = Value::Float(0.5);
    assert!(
        malformed(&raw(&Value::Map(float_body))),
        "a float in the body"
    );
}

#[test]
fn field_refusals() {
    let refuse = |f: &dyn Fn(&mut PackHeader)| {
        let mut h = fixture();
        f(&mut h);
        h.to_canonical().expect_err("refused")
    };
    assert!(matches!(
        refuse(&|h| h.id = "core".into()),
        HeaderError::Iri { field: "id", .. }
    ));
    assert!(matches!(
        refuse(&|h| h.id = "https://exa mple/".into()),
        HeaderError::Iri { .. }
    ));
    assert!(matches!(
        refuse(&|h| h.version = "1.0".into()),
        HeaderError::Version { .. }
    ));
    assert!(matches!(
        refuse(&|h| h.version = "01.0.0".into()),
        HeaderError::Version { .. }
    ));
    assert!(matches!(
        refuse(&|h| h.licence = "MIT AND".into()),
        HeaderError::Licence(_)
    ));
    assert!(matches!(
        refuse(&|h| h.licence = "Not-A-Licence".into()),
        HeaderError::Licence(_)
    ));
    assert!(matches!(
        refuse(&|h| h.format = "mere.pack/v1".into()),
        HeaderError::Format(_)
    ));
    assert_eq!(
        refuse(&|h| {
            h.kind.clear();
            h.body.clear();
        }),
        HeaderError::NoKind
    );
    assert_eq!(
        refuse(&|h| {
            h.body.remove(&PackKind::Participant);
        }),
        HeaderError::BodyKinds
    );
    assert!(matches!(
        refuse(&|h| h.kind.push(PackKind::Vocabulary)),
        HeaderError::Canonical(CanonicalError::DuplicateKey(_))
    ));
    assert!(matches!(
        refuse(&|h| h.requires.push(Requirement::Capability {
            capability: "cap".into()
        })),
        HeaderError::Iri {
            field: "requires.capability",
            ..
        }
    ));
    assert!(matches!(
        refuse(&|h| h.requires.push(Requirement::Pack {
            pack: "https://x.example/".into(),
            version: ">=1.0.0".into()
        })),
        HeaderError::Version {
            field: "requires.version",
            ..
        }
    ));
    assert_eq!(
        refuse(&|h| h.parts[0].media.clear()),
        HeaderError::EmptyPartField("media")
    );
}

// ---- digests and pins -------------------------------------------------------

#[test]
fn digest_instrument_controls() {
    // FIPS 180-2 B.1 and the BLAKE3 reference empty-input digest.
    assert_eq!(
        Hash::digest(HashFn::Sha256, b"abc").to_hex(),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(
        Hash::digest(HashFn::Blake3, b"").to_hex(),
        "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262"
    );
    let mh = Hash::digest(HashFn::Sha256, b"abc").to_multihash_bytes();
    assert_eq!(&mh[..2], &[0x12, 0x20], "multihash sha2-256 prefix");
    assert_eq!(
        Hash::from_multihash_bytes(&mh).unwrap().func(),
        HashFn::Sha256
    );
    assert_eq!(Hash::of(b"x").to_multihash_bytes()[..2], [0x1e, 0x20]);
}

#[test]
fn known_answer_pack_digests() {
    let canonical = fixture().to_canonical().unwrap();
    let blake3 = PackHeader::pin_of(&canonical, HashFn::Blake3).unwrap();
    let sha256 = PackHeader::pin_of(&canonical, HashFn::Sha256).unwrap();
    assert_eq!(
        hex(&canonical[..4]),
        "ac626964",
        "map(12), then the key \"id\""
    );
    assert_eq!(canonical.len(), FIXTURE_LEN, "fixture length");
    assert_eq!(blake3.digest.0.to_hex(), FIXTURE_BLAKE3);
    assert_eq!(sha256.digest.0.to_hex(), FIXTURE_SHA256);
    assert!(blake3.matches(&canonical) && sha256.matches(&canonical));
    let mut other = fixture();
    other.version = "1.2.4".into();
    assert!(
        !blake3.matches(&other.to_canonical().unwrap()),
        "another document"
    );
    // The pin round-trips through the codec with its digest as a byte string.
    let pin_bytes = to_canonical(&sha256).unwrap();
    assert_eq!(from_canonical::<Pin>(&pin_bytes).unwrap(), sha256);
}

// Re-recorded 2026-11-13 when the fixture's vocabulary body became typed (L2);
// the SHA-256 was cross-checked with Python's hashlib over the same bytes.
const FIXTURE_LEN: usize = 898;
const FIXTURE_BLAKE3: &str = "b57c9f8e24f8407275f6f74222cf232ef6fd75db6c71913e9c0ea80bd149c378";
const FIXTURE_SHA256: &str = "144887bf0b85915c2aa5378d1a21786446b446649042fe09f3a164f7c5e346e7";

// ---- the CDDL is the schema the encoder writes --------------------------------

fn cddl_accepts(schema: &str, bytes: &[u8]) -> bool {
    cddl::validate_cbor_from_slice(schema, bytes, None).is_ok()
}

#[test]
fn the_canonical_bytes_validate_against_the_cddl() {
    let canonical = fixture().to_canonical().unwrap();
    assert!(cddl_accepts(PACK2_CDDL, &canonical), "the fixture");
    let mut lean = fixture();
    lean.kind = vec![PackKind::HostProfile];
    lean.body = BTreeMap::from([(PackKind::HostProfile, BodyValue::Array(vec![]))]);
    (lean.credit, lean.requires, lean.provides) = (vec![], vec![], vec![]);
    (lean.contributes, lean.scopes, lean.parts) = (vec![], vec![], vec![]);
    assert!(
        cddl_accepts(PACK2_CDDL, &lean.to_canonical().unwrap()),
        "empty sets"
    );
    let pin = PackHeader::pin_of(&canonical, HashFn::Sha256).unwrap();
    let pin_root = format!("root = pin\n{PACK2_CDDL}");
    assert!(
        cddl_accepts(&pin_root, &to_canonical(&pin).unwrap()),
        "a pin"
    );
    // Negative controls: the validator refuses what the schema excludes.
    let mut wrong_format = fixture_map();
    *entry(&mut wrong_format, "format") = Value::Text("mere.pack/3".into());
    assert!(!cddl_accepts(PACK2_CDDL, &raw(&Value::Map(wrong_format))));
    let mut floated = fixture_map();
    let parts = entry(&mut floated, "parts").as_array_mut().unwrap();
    *entry(parts[0].as_map_mut().unwrap(), "bytes") = Value::Float(4.0);
    assert!(!cddl_accepts(PACK2_CDDL, &raw(&Value::Map(floated))));
    let mut unknown_kind = fixture_map();
    let body = entry(&mut unknown_kind, "body").as_map_mut().unwrap();
    body[0].0 = Value::Text("vocabularz".into());
    assert!(!cddl_accepts(PACK2_CDDL, &raw(&Value::Map(unknown_kind))));
}
