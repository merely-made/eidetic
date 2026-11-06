// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;
use eidetic::models::recall::RecallSourceKind;

fn id(source: &str) -> RecallSourceId {
    RecallSourceId {
        kind: RecallSourceKind::Clip,
        owner: Some("app".into()),
        source: source.into(),
        node: None,
    }
}
fn src(source: &str, text: &str) -> RetainedSourceText {
    RetainedSourceText {
        source: id(source),
        version: format!("{source}@v1"),
        title: None,
        text: text.into(),
    }
}
fn tag(concept: &str) -> TagIdentity {
    TagIdentity {
        owner: "app".into(),
        concept: concept.into(),
    }
}
fn concept(c: &str, label: &str) -> TagConcept {
    TagConcept {
        tag: tag(c),
        label: label.into(),
    }
}
fn run(
    sources: &[RetainedSourceText],
    concepts: &[TagConcept],
) -> Vec<eidetic::records::TagSuggestionV1> {
    suggest_tags(
        sources,
        concepts,
        &[],
        "run-1",
        &TagSuggestSettings::default(),
    )
    .unwrap()
}
fn scalars(text: &str, s: &PassageSelector) -> String {
    text.chars().skip(s.start).take(s.end - s.start).collect()
}

#[test]
fn multi_word_label_is_suggested_with_provenance_and_scalar_evidence() {
    // Leading non-ASCII shifts byte offsets away from scalar offsets.
    let text = "Éé: the harbour Tide turns at dawn.";
    let out = run(&[src("a", text)], &[concept("tag:tide", "Tide, harbour")]);
    assert_eq!(out.len(), 1);
    let s = &out[0];
    assert_eq!(s.concept, tag("tag:tide"));
    assert_eq!(s.subject_version, "a@v1");
    assert_eq!(s.input_text_hash, Hash::of(text.as_bytes()));
    assert_eq!(s.score, Some(1.0));
    assert_eq!(s.run, "run-1");
    assert_eq!(s.keeping, KeepingTier::Retained);
    assert_eq!(
        s.settings_digest,
        settings_digest(&TagSuggestSettings::default())
    );
    assert_eq!(
        s.producer.kind,
        SuggestionProducerKind::Rule {
            name: TAG_SUGGEST_RULE.into()
        }
    );
    let words: Vec<String> = s.evidence.iter().map(|e| scalars(text, e)).collect();
    assert_eq!(words, ["harbour", "Tide"]);
    eidetic::records::validate_suggestion(s).unwrap();
}

#[test]
fn case_and_non_ascii_lowercasing_follow_the_tokenizer() {
    let out = run(&[src("a", "CAFÉ ÉTÉ")], &[concept("c", "café été")]);
    assert_eq!(out.len(), 1);
    // No diacritic folding: the unaccented label does not match.
    assert!(run(&[src("a", "CAFÉ")], &[concept("c", "cafe")]).is_empty());
}

#[test]
fn partial_label_yields_nothing_by_default_and_partial_score_when_allowed() {
    let sources = [src("a", "harbour lights")];
    let concepts = [concept("c", "harbour tide")];
    assert!(run(&sources, &concepts).is_empty());
    let loose = TagSuggestSettings {
        min_coverage_permille: 500,
        ..Default::default()
    };
    let out = suggest_tags(&sources, &concepts, &[], "r", &loose).unwrap();
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].score, Some(0.5));
    assert_eq!(scalars("harbour lights", &out[0].evidence[0]), "harbour");
    assert_ne!(
        out[0].settings_digest,
        settings_digest(&TagSuggestSettings::default())
    );
}

#[test]
fn output_is_deterministic_and_independent_of_concept_order() {
    let sources = [src("b", "tide and ferry"), src("a", "ferry tide")];
    let c1 = [
        concept("z", "ferry"),
        concept("a", "tide"),
        concept("a", "ignored"),
    ];
    let c2 = [concept("a", "tide"), concept("z", "ferry")];
    let r1 = run(&sources, &c1);
    assert_eq!(r1, run(&sources, &c1));
    assert_eq!(r1, run(&sources, &c2));
    let order: Vec<(&str, &str)> = r1
        .iter()
        .map(|s| (s.subject.source.as_str(), s.concept.concept.as_str()))
        .collect();
    assert_eq!(order, [("b", "a"), ("b", "z"), ("a", "a"), ("a", "z")]);
    let ids: Vec<_> = r1.iter().map(|s| s.id()).collect();
    assert_eq!(
        ids,
        run(&sources, &c1)
            .iter()
            .map(|s| s.id())
            .collect::<Vec<_>>()
    );
}

#[test]
fn already_tagged_concepts_are_skipped_for_that_source_only() {
    let sources = [src("a", "tide"), src("b", "tide")];
    let existing = [(id("a"), tag("c"))];
    let out = suggest_tags(
        &sources,
        &[concept("c", "tide")],
        &existing,
        "r",
        &TagSuggestSettings::default(),
    )
    .unwrap();
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].subject.source, "b");
}

#[test]
fn evidence_cap_keeps_each_term_first_and_refusals_hold() {
    let text = "tide tide tide ferry";
    let s = TagSuggestSettings {
        max_selectors: 2,
        ..Default::default()
    };
    let out = suggest_tags(
        &[src("a", text)],
        &[concept("c", "tide ferry")],
        &[],
        "r",
        &s,
    )
    .unwrap();
    let words: Vec<String> = out[0].evidence.iter().map(|e| scalars(text, e)).collect();
    assert_eq!(words, ["tide", "ferry"]);
    let default = TagSuggestSettings::default();
    assert!(suggest_tags(&[], &[], &[], " ", &default).is_err());
    let zero = TagSuggestSettings {
        max_selectors: 0,
        ..default
    };
    assert!(suggest_tags(&[], &[], &[], "r", &zero).is_err());
}
