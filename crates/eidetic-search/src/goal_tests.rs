// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;
use eidetic::models::recall::RecallSourceKind;

fn id(source: &str) -> RecallSourceId {
    RecallSourceId {
        kind: RecallSourceKind::Fleece,
        owner: Some("session-a".into()),
        source: source.into(),
        node: None,
    }
}

fn retained(source: &str, title: Option<&str>, text: &str) -> RetainedSourceText {
    RetainedSourceText {
        source: id(source),
        version: format!("{source}@v1"),
        title: title.map(str::to_owned),
        text: text.into(),
    }
}

fn label(source: &str, label: Option<&str>) -> CandidateLabel {
    CandidateLabel {
        source: id(source),
        label: label.map(str::to_owned),
    }
}

#[test]
fn goal_terms_are_distinct_and_sorted() {
    let assessor = GoalAssessor::new("Tide tables, tide clocks", GoalSettings::default()).unwrap();
    assert_eq!(assessor.terms(), ["clocks", "tables", "tide"]);
}

#[test]
fn unusable_goals_and_settings_are_refused() {
    assert!(GoalAssessor::new("  ,. ", GoalSettings::default()).is_err());
    let tight = GoalSettings {
        max_goal_chars: 3,
        ..GoalSettings::default()
    };
    assert!(GoalAssessor::new("tides", tight).is_err());
    for bad in [
        GoalSettings {
            min_coverage_permille: 0,
            ..GoalSettings::default()
        },
        GoalSettings {
            min_coverage_permille: 1001,
            ..GoalSettings::default()
        },
        GoalSettings {
            satisfy_after: 0,
            ..GoalSettings::default()
        },
    ] {
        assert!(GoalAssessor::new("tides", bad).is_err());
    }
}

#[test]
fn assessment_cites_version_hash_terms_and_scalar_selectors() {
    let mut assessor = GoalAssessor::new("harbour tide", GoalSettings::default()).unwrap();
    let source = retained("a", None, "Ébb: the harbour tide turns.");
    assert_eq!(assessor.assess(&source).unwrap(), GoalVerdict::Match);
    let record = assessor.record();
    let assessed = &record.assessments[0];
    assert_eq!(assessed.version, "a@v1");
    assert_eq!(
        assessed.text_hash,
        eidetic::Hash::of(source.text.as_bytes())
    );
    assert_eq!(assessed.matched_terms, ["harbour", "tide"]);
    assert!(assessed.missing_terms.is_empty());
    assert_eq!(assessed.coverage_permille, 1000);
    // "Ébb: the " is nine scalars but ten bytes; selectors count scalars.
    assert_eq!(
        assessed.matches,
        [
            PassageSelector { start: 9, end: 16 },
            PassageSelector { start: 17, end: 21 },
        ]
    );
    let chars: Vec<char> = source.text.chars().collect();
    let first: String = chars[9..16].iter().collect();
    assert_eq!(first, "harbour");
}

#[test]
fn coverage_below_threshold_continues_and_title_counts() {
    let mut assessor = GoalAssessor::new("harbour tide clock", GoalSettings::default()).unwrap();
    assert_eq!(
        assessor.assess(&retained("a", None, "a clock")).unwrap(),
        GoalVerdict::Continue
    );
    assert_eq!(
        assessor
            .assess(&retained("b", Some("Harbour clocks"), "the tide"))
            .unwrap(),
        GoalVerdict::Match
    );
    let record = assessor.record();
    assert_eq!(record.assessments[0].coverage_permille, 333);
    assert_eq!(record.assessments[0].missing_terms, ["harbour", "tide"]);
    assert_eq!(record.assessments[1].matched_terms, ["harbour", "tide"]);
    // Title matches count for coverage; selectors address the body only.
    assert_eq!(
        record.assessments[1].matches,
        [PassageSelector { start: 4, end: 8 }]
    );
    assert_eq!(record.matched, 1);
}

#[test]
fn satisfied_after_the_configured_number_of_matches() {
    let settings = GoalSettings {
        satisfy_after: 2,
        ..GoalSettings::default()
    };
    let mut assessor = GoalAssessor::new("tide", settings).unwrap();
    assert_eq!(
        assessor.assess(&retained("a", None, "tide")).unwrap(),
        GoalVerdict::Match
    );
    assert_eq!(
        assessor.assess(&retained("b", None, "none")).unwrap(),
        GoalVerdict::Continue
    );
    assert_eq!(
        assessor.assess(&retained("c", None, "a tide")).unwrap(),
        GoalVerdict::Satisfied
    );
}

#[test]
fn oversized_source_is_refused_without_changing_state() {
    let settings = GoalSettings {
        max_text_chars: 8,
        ..GoalSettings::default()
    };
    let mut assessor = GoalAssessor::new("tide", settings).unwrap();
    assert!(
        assessor
            .assess(&retained("a", Some("tide"), "tide tide"))
            .is_err()
    );
    let record = assessor.record();
    assert!(record.assessments.is_empty());
    assert_eq!(record.matched, 0);
}

#[test]
fn selector_cap_is_recorded() {
    let settings = GoalSettings {
        max_selectors: 2,
        ..GoalSettings::default()
    };
    let mut assessor = GoalAssessor::new("tide", settings).unwrap();
    assessor
        .assess(&retained("a", None, "tide tide tide"))
        .unwrap();
    let assessed = &assessor.record().assessments[0];
    assert_eq!(assessed.matches.len(), 2);
    assert!(assessed.matches_truncated);
}

#[test]
fn selection_prefers_most_shared_terms_then_oldest() {
    let mut assessor = GoalAssessor::new("harbour tide clock", GoalSettings::default()).unwrap();
    let pending = [
        label("a", None),
        label("b", Some("tide")),
        label("c", Some("Tide clock")),
        label("d", Some("clock and tide")),
    ];
    assert_eq!(assessor.select(&pending), Some(2));
    let choice = &assessor.record().selections[0];
    assert_eq!(choice.rule, SelectionRule::GoalTermOverlap);
    assert_eq!(choice.matched_terms, ["clock", "tide"]);
    assert_eq!(choice.source, id("c"));
    assert_eq!(choice.pending, 4);
}

#[test]
fn selection_falls_back_to_oldest_and_records_it() {
    let mut assessor = GoalAssessor::new("tide", GoalSettings::default()).unwrap();
    assert_eq!(assessor.select(&[]), None);
    assert_eq!(
        assessor.select(&[label("a", Some("weather")), label("b", None)]),
        Some(0)
    );
    let record = assessor.record();
    assert_eq!(record.selections.len(), 1);
    assert_eq!(record.selections[0].rule, SelectionRule::OldestFallback);
}

#[test]
fn record_round_trips_and_names_its_operation() {
    let mut assessor = GoalAssessor::new("tide", GoalSettings::default()).unwrap();
    assessor.select(&[label("a", Some("tide"))]);
    assessor.assess(&retained("a", None, "tide")).unwrap();
    let record = assessor.record();
    assert_eq!(record.operation_version, GOAL_OPERATION_VERSION);
    assert!(record.tokenizer.starts_with(TOKENIZER_NAME));
    let json = serde_json::to_string(&record).unwrap();
    let back: GoalRecord = serde_json::from_str(&json).unwrap();
    assert_eq!(back, record);
}

fn scalars(text: &str, p: &PassageSelector) -> String {
    text.chars().skip(p.start).take(p.end - p.start).collect()
}

fn no_context() -> GoalSettings {
    GoalSettings {
        passage_context_chars: 0,
        ..GoalSettings::default()
    }
}

#[test]
fn stop_list_is_sorted_for_binary_search() {
    assert!(STOP_WORDS.windows(2).all(|w| w[0] < w[1]));
}

#[test]
fn stop_words_are_excluded_from_terms_coverage_and_selection() {
    let mut a = GoalAssessor::new("keep a starter alive", GoalSettings::default()).unwrap();
    assert_eq!(a.terms(), ["alive", "keep", "starter"]);
    assert_eq!(a.record().stop_words.as_deref(), Some(STOP_WORDS_ID));
    let s = retained("s", None, "a a a keep the starter alive");
    a.assess(&s).unwrap();
    let got = a.last_assessment().unwrap();
    assert_eq!(got.coverage_permille, 1000);
    assert_eq!(got.matches.len(), 3);
    let picked = a.select(&[label("x", Some("a a a")), label("y", Some("alive"))]);
    assert_eq!(picked, Some(1));
    let off = GoalSettings {
        ignore_stop_words: false,
        ..GoalSettings::default()
    };
    let b = GoalAssessor::new("keep a starter", off).unwrap();
    assert_eq!(b.terms(), ["a", "keep", "starter"]);
    assert_eq!(b.record().stop_words, None);
}

#[test]
fn all_stop_word_goal_keeps_its_terms() {
    let a = GoalAssessor::new("to be or not to be", GoalSettings::default()).unwrap();
    assert_eq!(a.terms(), ["not"]);
    let b = GoalAssessor::new("to be", GoalSettings::default()).unwrap();
    assert_eq!(b.terms(), ["be", "to"]);
    let all = GoalAssessor::new("the of", GoalSettings::default()).unwrap();
    assert_eq!(all.terms(), ["of", "the"]);
    assert_eq!(all.record().stop_words, None);
}

#[test]
fn old_settings_json_deserializes_with_defaults() {
    let json = r#"{"min_coverage_permille":500,"satisfy_after":3,"max_goal_chars":10,
        "max_text_chars":100,"max_selectors":4}"#;
    let s: GoalSettings = serde_json::from_str(json).unwrap();
    assert!(s.ignore_stop_words);
    assert_eq!(s.passage_gap_chars, 40);
    assert_eq!(s.passage_context_chars, 60);
    let mut a = GoalAssessor::new("tide", GoalSettings::default()).unwrap();
    a.assess(&retained("a", None, "tide")).unwrap();
    let mut v = serde_json::to_value(a.record()).unwrap();
    v.as_object_mut().unwrap().remove("stop_words");
    v["assessments"][0]
        .as_object_mut()
        .unwrap()
        .remove("passages");
    v["assessments"][0]
        .as_object_mut()
        .unwrap()
        .remove("passages_truncated");
    let back: GoalRecord = serde_json::from_value(v).unwrap();
    assert_eq!(back.stop_words, None);
    assert!(back.assessments[0].passages.is_empty());
}

#[test]
fn passages_merge_nearby_hits_and_separate_distant_ones() {
    let text = format!("tide and harbour {} tide", "x".repeat(200));
    let mut a = GoalAssessor::new("tide harbour", no_context()).unwrap();
    a.assess(&retained("a", None, &text)).unwrap();
    let got = a.last_assessment().unwrap();
    assert_eq!(got.matches.len(), 3);
    assert_eq!(got.passages.len(), 2);
    assert_eq!(scalars(&text, &got.passages[0]), "tide and harbour");
    assert_eq!(scalars(&text, &got.passages[1]), "tide");
    assert!(!got.passages_truncated);
}

#[test]
fn passages_expand_with_context_on_multibyte_text_within_bounds() {
    let text = "ééé ééé tide ééé ééé";
    let settings = GoalSettings {
        passage_context_chars: 5,
        ..GoalSettings::default()
    };
    let mut a = GoalAssessor::new("tide", settings).unwrap();
    a.assess(&retained("a", None, text)).unwrap();
    let p = &a.last_assessment().unwrap().passages[0];
    assert_eq!(scalars(text, p), "ééé tide ééé");
    let big = GoalSettings {
        passage_context_chars: 1000,
        ..GoalSettings::default()
    };
    let mut b = GoalAssessor::new("tide", big).unwrap();
    b.assess(&retained("a", None, text)).unwrap();
    let p = &b.last_assessment().unwrap().passages[0];
    assert_eq!((p.start, p.end), (0, text.chars().count()));
}

#[test]
fn passage_cap_is_recorded() {
    let text = format!("tide{}", format!(" {} tide", "x".repeat(100)).repeat(3));
    let settings = GoalSettings {
        max_selectors: 2,
        ..no_context()
    };
    let mut a = GoalAssessor::new("tide", settings).unwrap();
    a.assess(&retained("a", None, &text)).unwrap();
    let got = a.last_assessment().unwrap();
    assert_eq!(got.passages.len(), 2);
    assert!(got.passages_truncated);
}
