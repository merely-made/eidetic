// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Declared-lens §3.1 for the authored form: authored, canonical, decoded and
//! re-encoded bytes are equal; CURIEs expand to what the typed body holds; a
//! prefix rename or any row order gives the same bytes (a proptest); each
//! authored-form rule refuses.

use proptest::prelude::*;

use super::v2_body_tests::{lens, vocabulary};
use super::*;

const HEAD: &str = r#"
id = "https://mere.computer/packs/core"
version = "1.2.3-rc.1+build.5"
kind = ["vocabulary", "lens"]
licence = "MPL-2.0 OR Apache-2.0"
credit = ["Mark", "A Contributor"]
provides = ["rel:"]

[prefixes]
rel = "https://mere.computer/ns/rel#"
axis = "https://mere.computer/ns/axis#"
st = "https://mere.computer/ns/stratum#"
pr = "https://mere.computer/ns/presentation#"
fam = "https://mere.computer/ns/family#"
ex = "https://example.org/packs/"

[vocabulary.defaults]
"axis:stratum" = "st:resource"
"#;

const TERMS: [&str; 3] = [
    r#"[[vocabulary.terms]]
iri = "rel:cites"
kind = "predicate"
family = "fam:semantic"
axes = { "axis:stratum" = "st:resource" }
labels = { en = "cites" }
"#,
    r#"[[vocabulary.terms]]
iri = "rel:user-grouped"
kind = "predicate"
family = "[fam:semantic]"
axes = { "axis:stratum" = ["st:surface"] }
labels = { en = "user-grouped" }
"#,
    r#"[[vocabulary.terms]]
iri = "rel:quotes"
kind = "predicate"
family = "fam:semantic"
handler = "https://mere.computer/ns/handler#quote"
axes = { "axis:stratum" = "st:resource", "axis:presentation" = ["pr:quote", "pr:generic"] }
labels = { en = "quotes" }
"#,
];

const LENS: &str = r#"
[lens]
select = [{ pack = "ex:b", version = "1.0.0" }, { pack = "ex:a", version = "0.2.0" }]
settle = [{ term = "rel:cites", pack = "ex:a" }, { term = "rel:quotes", pack = "ex:b" }]
entail = { rdfs = true, owl2-rl = false }
present = [{ term = "rel:cites", class = "pr:generic" }]
"#;

fn authored(order: &[usize]) -> String {
    let terms: String = order
        .iter()
        .map(|&i| TERMS[i])
        .collect::<Vec<_>>()
        .join("\n");
    format!("{HEAD}\n{terms}\n{LENS}")
}

/// The same pack written with the typed bodies (`v2_body_tests`).
fn typed_bytes() -> Vec<u8> {
    let mut h = super::v2_codec_tests::fixture();
    h.kind = vec![PackKind::Vocabulary, PackKind::Lens];
    (h.requires, h.contributes, h.scopes, h.parts) = (vec![], vec![], vec![], vec![]);
    h.provides = vec!["https://mere.computer/ns/rel#".into()];
    h.body = [
        (PackKind::Vocabulary, vocabulary().to_value().unwrap()),
        (PackKind::Lens, lens().to_value().unwrap()),
    ]
    .into();
    h.to_canonical().unwrap()
}

#[test]
fn authored_canonical_decoded_and_reencoded_are_equal() {
    let (header, bytes) = compile_authored(&authored(&[0, 1, 2])).unwrap();
    assert_eq!(bytes, typed_bytes(), "CURIEs expand to the typed rows");
    assert_eq!(header.to_canonical().unwrap(), bytes);
    let decoded = PackHeader::from_canonical(&bytes).unwrap();
    assert_eq!(decoded.to_canonical().unwrap(), bytes);
}

#[test]
fn a_prefix_rename_does_not_move_the_bytes() {
    let renamed = authored(&[0, 1, 2])
        .replace("rel = ", "r = ")
        .replace("\"rel:", "\"r:");
    assert_eq!(compile_authored(&renamed).unwrap().1, typed_bytes());
}

#[test]
fn each_authored_rule_is_refused() {
    let base = authored(&[0, 1, 2]);
    let err = |text: &str| compile_authored(text).unwrap_err();
    assert!(matches!(
        err(&base.replace("\"fam:semantic\"", "\"fma:semantic\"")),
        AuthoredError::UnknownPrefix {
            field: "terms.family",
            ..
        }
    ));
    assert!(matches!(
        err(&base.replace("[fam:semantic]", "[https://x/]")),
        AuthoredError::UnknownPrefix { .. }
    ));
    assert!(matches!(
        err(&base.replace("st = ", "1st = ")),
        AuthoredError::Prefix(_)
    ));
    assert!(matches!(
        err(&base.replace("ex = \"https://example.org/packs/\"", "ex = \"packs/\"")),
        AuthoredError::Prefix(_)
    ));
    assert!(matches!(
        err(&base.replace(
            "kind = [\"vocabulary\", \"lens\"]",
            "kind = [\"vocabulary\", \"lens\", \"rules\"]"
        )),
        AuthoredError::Unsupported(_)
    ));
    assert!(matches!(
        err(&format!("{base}\n[[parts]]\nname = \"a\"\n")),
        AuthoredError::Toml(_) | AuthoredError::Unsupported("parts")
    ));
    assert!(matches!(
        err(&base.replace(
            "labels = { en = \"cites\" }",
            "labels = { en = \"cites\" }\nslug = \"c\""
        )),
        AuthoredError::Toml(_)
    ));
    assert!(matches!(
        err(&base.replace(
            "kind = [\"vocabulary\", \"lens\"]",
            "kind = [\"vocabulary\"]"
        )),
        AuthoredError::Header(HeaderError::BodyKinds)
    ));
    let bare = "id = \"urn:x:p\"
version = \"1.0.0\"
kind = [\"vocabulary\", \"lens\"]
licence = \"MIT\"
";
    let (h, _) = compile_authored(bare).unwrap();
    assert_eq!(
        h.vocabulary().unwrap(),
        Some(VocabularyBody::default()),
        "a kind without a section"
    );
    assert_eq!(h.lens().unwrap(), Some(LensBody::default()));
    assert!(matches!(
        err(&base.replace(
            "axis:presentation\" = [\"pr:quote\", \"pr:generic\"]",
            "axis:presentation\" = []"
        )),
        AuthoredError::Body {
            error: BodyError::EmptyChain { .. },
            ..
        }
    ));
    assert!(matches!(err("id = 3"), AuthoredError::Toml(_)));
    // Plain IRIs pass through untouched.
    let plain = base.replace("\"rel:cites\"\nkind", "\"urn:x:cites\"\nkind");
    let (h, _) = compile_authored(&plain).unwrap();
    let rows = h.vocabulary().unwrap().unwrap().terms;
    assert!(rows.iter().any(|t| t.iri == "urn:x:cites"));
}

proptest! {
    /// Any order of the authored rows compiles to the same bytes.
    #[test]
    fn row_order_never_changes_the_bytes(order in Just(vec![0usize, 1, 2]).prop_shuffle()) {
        let (_, bytes) = compile_authored(&authored(&order)).unwrap();
        prop_assert_eq!(bytes, typed_bytes());
    }
}
