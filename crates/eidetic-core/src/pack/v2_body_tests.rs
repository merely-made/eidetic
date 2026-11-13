// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Declared-lens L2 metrics for the typed `vocabulary` and `lens` bodies:
//! byte-identity round trips, set-order independence, one refusal per body
//! rule, and the CDDL's `vocabulary-body` and `lens-body` rules.

use std::collections::BTreeMap;

use ciborium::value::Value;

use super::*;

const NS: &str = "https://mere.computer/ns/";

fn term(local: &str, stratum: &str) -> Term {
    Term {
        iri: format!("{NS}rel#{local}"),
        kind: TermKind::Predicate,
        family: Some(format!("{NS}family#semantic")),
        handler: None,
        axes: BTreeMap::from([(
            format!("{NS}axis#stratum"),
            vec![format!("{NS}stratum#{stratum}")],
        )]),
        labels: BTreeMap::from([("en".to_string(), local.to_string())]),
    }
}

pub(super) fn vocabulary() -> VocabularyBody {
    let mut chained = term("quotes", "resource");
    chained.axes.insert(
        format!("{NS}axis#presentation"),
        vec![
            format!("{NS}presentation#quote"),
            format!("{NS}presentation#generic"),
        ],
    );
    chained.handler = Some(format!("{NS}handler#quote"));
    VocabularyBody {
        terms: vec![
            term("cites", "resource"),
            term("user-grouped", "surface"),
            chained,
        ],
        defaults: BTreeMap::from([(format!("{NS}axis#stratum"), format!("{NS}stratum#resource"))]),
    }
}

pub(super) fn lens() -> LensBody {
    LensBody {
        select: vec![
            Selection {
                pack: "https://example.org/packs/b".into(),
                version: "1.0.0".into(),
            },
            Selection {
                pack: "https://example.org/packs/a".into(),
                version: "0.2.0".into(),
            },
        ],
        settle: vec![
            Settlement {
                term: format!("{NS}rel#cites"),
                pack: "https://example.org/packs/a".into(),
            },
            Settlement {
                term: format!("{NS}rel#quotes"),
                pack: "https://example.org/packs/b".into(),
            },
        ],
        rules: vec![],
        shapes: vec![],
        entail: Entail {
            rdfs: true,
            owl2_rl: false,
        },
        present: vec![Presentation {
            term: format!("{NS}rel#cites"),
            class: format!("{NS}presentation#generic"),
            opens: None,
        }],
    }
}

fn header() -> PackHeader {
    let mut h = super::v2_codec_tests::fixture();
    h.kind = vec![PackKind::Vocabulary, PackKind::Lens];
    h.body = BTreeMap::from([
        (PackKind::Vocabulary, vocabulary().to_value().unwrap()),
        (PackKind::Lens, lens().to_value().unwrap()),
    ]);
    h
}

fn raw(value: &Value) -> Vec<u8> {
    let mut out = Vec::new();
    ciborium::ser::into_writer(value, &mut out).unwrap();
    out
}

/// The header's body map as raw ciborium values, to tamper with.
fn tamper_body(kind: &str, f: impl FnOnce(&mut Vec<(Value, Value)>)) -> Vec<u8> {
    let bytes = header().to_canonical().unwrap();
    let mut doc: Value = ciborium::de::from_reader(bytes.as_slice()).unwrap();
    let body = field(doc.as_map_mut().unwrap(), "body")
        .as_map_mut()
        .unwrap();
    f(field(body, kind).as_map_mut().unwrap());
    raw(&doc)
}

fn field<'a>(map: &'a mut [(Value, Value)], key: &str) -> &'a mut Value {
    &mut map
        .iter_mut()
        .find(|(k, _)| k.as_text() == Some(key))
        .unwrap()
        .1
}

fn body_error(bytes: &[u8]) -> Option<BodyError> {
    match PackHeader::from_canonical(bytes) {
        Err(HeaderError::Body { error, .. }) => Some(error),
        _ => None,
    }
}

#[test]
fn typed_bodies_round_trip_byte_identical() {
    let h = header();
    let bytes = h.to_canonical().unwrap();
    let decoded = PackHeader::from_canonical(&bytes).unwrap();
    assert_eq!(decoded.to_canonical().unwrap(), bytes);
    let v = decoded.vocabulary().unwrap().unwrap();
    let mut terms = vocabulary().terms;
    terms.sort_by_key(|t| to_canonical(t).unwrap());
    assert_eq!(v.terms, terms, "rows come back as a sorted set");
    let l = decoded.lens().unwrap().unwrap();
    assert_eq!(l.select, lens().select, "select keeps its order");
    assert_eq!(l.entail, lens().entail);
}

#[test]
fn set_order_in_bodies_does_not_change_the_bytes() {
    let expected = header().to_canonical().unwrap();
    let mut v = vocabulary();
    v.terms.reverse();
    let mut l = lens();
    l.settle.reverse();
    let mut h = header();
    h.body = BTreeMap::from([
        (PackKind::Vocabulary, v.to_value().unwrap()),
        (PackKind::Lens, l.to_value().unwrap()),
    ]);
    assert_eq!(h.to_canonical().unwrap(), expected);
    // `select` is ordered: reversing it is another document.
    let mut l = lens();
    l.select.reverse();
    h.body.insert(PackKind::Lens, l.to_value().unwrap());
    assert_ne!(h.to_canonical().unwrap(), expected);
}

#[test]
fn each_body_rule_is_refused() {
    let refuse_v = |f: &dyn Fn(&mut VocabularyBody)| {
        let mut v = vocabulary();
        f(&mut v);
        v.to_value().unwrap_err()
    };
    assert!(matches!(
        refuse_v(&|v| v.terms.push(term("cites", "surface"))),
        BodyError::DuplicateTerm(_)
    ));
    assert!(matches!(
        refuse_v(&|v| v.terms.push(term("cites", "resource"))),
        BodyError::Set(_)
    ));
    assert!(matches!(
        refuse_v(&|v| v.terms[0].axes.values_mut().for_each(Vec::clear)),
        BodyError::EmptyChain { .. }
    ));
    assert!(matches!(
        refuse_v(&|v| v.terms[0].iri = "cites".into()),
        BodyError::Iri {
            field: "terms.iri",
            ..
        }
    ));
    assert!(matches!(
        refuse_v(&|v| {
            v.terms[0].axes.insert("stratum".into(), vec!["x".into()]);
        }),
        BodyError::Iri {
            field: "terms.axes",
            ..
        }
    ));
    assert!(matches!(
        refuse_v(&|v| {
            v.terms[0].labels.insert("en_GB".into(), "x".into());
        }),
        BodyError::LanguageTag(_)
    ));
    assert!(matches!(
        refuse_v(&|v| {
            v.defaults
                .insert(format!("{NS}axis#symmetry"), "directed".into());
        }),
        BodyError::Iri {
            field: "defaults.value",
            ..
        }
    ));
    let refuse_l = |f: &dyn Fn(&mut LensBody)| {
        let mut l = lens();
        f(&mut l);
        l.to_value().unwrap_err()
    };
    assert!(matches!(
        refuse_l(&|l| l.select.push(l.select[0].clone())),
        BodyError::DuplicateSelection(_)
    ));
    assert!(matches!(
        refuse_l(&|l| l.select[0].version = ">=1".into()),
        BodyError::Version(_)
    ));
    assert!(matches!(
        refuse_l(&|l| l.present[0].opens = Some("reader".into())),
        BodyError::Iri {
            field: "present.opens",
            ..
        }
    ));
}

#[test]
fn strict_decoding_refuses_malformed_bodies() {
    let unsorted = tamper_body("vocabulary", |v| {
        field(v, "terms").as_array_mut().unwrap().reverse();
    });
    assert_eq!(
        PackHeader::from_canonical(&unsorted),
        Err(HeaderError::Canonical(CanonicalError::NotCanonical)),
        "an unsorted term set"
    );
    let unknown = tamper_body("vocabulary", |v| {
        v.push((Value::Text("prefixes".into()), Value::Map(vec![])));
    });
    assert!(
        matches!(body_error(&unknown), Some(BodyError::Shape(_))),
        "prefixes are authored-form only"
    );
    let missing = tamper_body("lens", |l| l.retain(|(k, _)| k.as_text() != Some("entail")));
    assert!(
        matches!(body_error(&missing), Some(BodyError::Shape(_))),
        "no entail"
    );
    let wrong = tamper_body("lens", |l| {
        *field(l, "rules") = Value::Integer(3.into());
    });
    assert!(
        matches!(body_error(&wrong), Some(BodyError::Shape(_))),
        "rules not an array"
    );
    let one = tamper_body("vocabulary", |v| {
        let terms = field(v, "terms").as_array_mut().unwrap();
        let axes = field(terms[0].as_map_mut().unwrap(), "axes")
            .as_map_mut()
            .unwrap();
        axes[0].1 = Value::Text(format!("{NS}stratum#resource"));
    });
    assert!(
        matches!(body_error(&one), Some(BodyError::Shape(_))),
        "a bare value where the canonical form has a chain"
    );
    let float = tamper_body("lens", |l| {
        *field(l, "entail") = Value::Float(1.0);
    });
    assert!(
        PackHeader::from_canonical(&float).is_err(),
        "a float in the lens"
    );
}

#[test]
fn the_typed_bodies_validate_against_the_cddl() {
    let accepts = |bytes: &[u8]| cddl::validate_cbor_from_slice(PACK2_CDDL, bytes, None).is_ok();
    assert!(
        accepts(&header().to_canonical().unwrap()),
        "the typed fixture"
    );
    let mut empty = header();
    empty.body = BTreeMap::from([
        (
            PackKind::Vocabulary,
            VocabularyBody::default().to_value().unwrap(),
        ),
        (PackKind::Lens, LensBody::default().to_value().unwrap()),
    ]);
    assert!(accepts(&empty.to_canonical().unwrap()), "empty bodies");
    // Negative controls: shapes the encoder never writes.
    let untyped = tamper_body("vocabulary", |v| {
        *field(v, "terms") = Value::Array(vec![Value::Integer((-3).into())]);
    });
    assert!(!accepts(&untyped), "an integer term row");
    let bare = tamper_body("vocabulary", |v| {
        let terms = field(v, "terms").as_array_mut().unwrap();
        let axes = field(terms[0].as_map_mut().unwrap(), "axes")
            .as_map_mut()
            .unwrap();
        axes[0].1 = Value::Array(vec![]);
    });
    assert!(!accepts(&bare), "an empty chain");
    let no_entail = tamper_body("lens", |l| l.retain(|(k, _)| k.as_text() != Some("entail")));
    assert!(!accepts(&no_entail), "a lens without entail");
}
