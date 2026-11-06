// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;
use eidetic::models::embedding::{
    EmbeddingComponentV1, EmbeddingCorpusV1, EmbeddingMetric, EmbeddingSpaceV1,
};
use eidetic::models::recall::RecallSourceKind;
use std::cell::Cell;
use std::collections::BTreeMap;
use std::rc::Rc;

/// Deterministic stub: axes are word families, so "sea" is near "tide"
/// without sharing a token (which S1's term overlap cannot see).
struct Stub {
    revision: &'static str,
    fail: Rc<Cell<bool>>,
    calls: Rc<Cell<usize>>,
}

const FAMILIES: [&[&str]; 3] = [
    &["tide", "tides", "sea", "ocean"],
    &["moon", "lunar"],
    &["rust"],
];

impl Stub {
    fn new() -> (Self, Rc<Cell<bool>>, Rc<Cell<usize>>) {
        let fail = Rc::new(Cell::new(false));
        let calls = Rc::new(Cell::new(0));
        let stub = Self {
            revision: "stub-1",
            fail: fail.clone(),
            calls: calls.clone(),
        };
        (stub, fail, calls)
    }
}

impl GoalEmbedder for Stub {
    fn envelope(&self) -> EmbeddingEnvelopeV1 {
        let component = EmbeddingComponentV1 {
            name: "stub".into(),
            revision: self.revision.into(),
            parameters: BTreeMap::new(),
            artifacts: BTreeMap::new(),
        };
        EmbeddingEnvelopeV1 {
            version: 1,
            space: EmbeddingSpaceV1 {
                producer: component.clone(),
                tokenizer: component.clone(),
                preprocessing: component,
                pooling: "sum".into(),
                query_prefix: String::new(),
                document_prefix: String::new(),
                normalization: "none".into(),
                dimensions: 4,
                metric: EmbeddingMetric::Cosine,
            },
            corpus: EmbeddingCorpusV1 {
                chunker: "unset".into(),
                sources: Vec::new(),
            },
        }
    }

    fn embed(&mut self, texts: &[&str], _role: EmbedRole) -> Result<Vec<Vec<f32>>, String> {
        self.calls.set(self.calls.get() + 1);
        if self.fail.get() {
            return Err("device lost".into());
        }
        Ok(texts
            .iter()
            .map(|t| {
                let lower = t.to_lowercase();
                let mut v: Vec<f32> = FAMILIES
                    .iter()
                    .map(|f| {
                        lower
                            .split(|c: char| !c.is_alphanumeric())
                            .filter(|w| f.contains(w))
                            .count() as f32
                    })
                    .collect();
                v.push(0.01); // never the zero vector
                v
            })
            .collect())
    }
}

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

fn assessor(
    settings: ModelGoalSettings,
) -> (ModelGoalAssessor<Stub>, Rc<Cell<bool>>, Rc<Cell<usize>>) {
    let (stub, fail, calls) = Stub::new();
    let a = ModelGoalAssessor::new("tides", GoalSettings::default(), settings, stub).unwrap();
    (a, fail, calls)
}

#[test]
fn model_ranking_overrides_s1_selection_and_records_both() {
    let (mut a, _, _) = assessor(ModelGoalSettings::default());
    // S1 sees no shared term and would fall back to the oldest (index 0).
    let pending = [
        label("a", Some("moon phases")),
        label("b", Some("ocean sea")),
        label("c", None),
    ];
    assert_eq!(a.select(&pending), Some(1));
    let rec = a.record();
    let sel = &rec.selections[0];
    assert_eq!(sel.s1.index, 0);
    assert_eq!(sel.s1.rule, crate::goal::SelectionRule::OldestFallback);
    assert_eq!(sel.model_pick, Some(1));
    assert_eq!((sel.chosen, sel.winner), (1, SelectionWinner::Model));
    assert!(sel.scores_milli[1] > sel.scores_milli[0]);
    assert_eq!(sel.scores_milli[2], None);
    assert_eq!(rec.s1.selections[0], sel.s1);
    assert_eq!(sel.status, ModelStatus::Used);
}

#[test]
fn s1_overlap_mode_keeps_s1_when_its_rule_fired() {
    let settings = ModelGoalSettings {
        select_mode: ModelSelectMode::S1Overlap,
        ..ModelGoalSettings::default()
    };
    let (mut a, _, _) = assessor(settings);
    let pending = [
        label("a", Some("tides tables")),
        label("b", Some("ocean sea")),
    ];
    assert_eq!(a.select(&pending), Some(0));
    assert_eq!(a.record().selections[0].winner, SelectionWinner::S1);
    // With no overlap anywhere the model decides.
    let pending = [label("a", Some("moon")), label("b", Some("ocean sea"))];
    assert_eq!(a.select(&pending), Some(1));
    assert_eq!(a.record().selections[1].winner, SelectionWinner::Model);
}

#[test]
fn selection_floor_returns_the_decision_to_s1() {
    let settings = ModelGoalSettings {
        min_select_similarity_milli: 900,
        ..ModelGoalSettings::default()
    };
    let (mut a, _, _) = assessor(settings);
    let pending = [label("a", Some("moon")), label("b", Some("moon sea"))];
    assert_eq!(a.select(&pending), Some(0));
    let sel = &a.record().selections[0];
    assert_eq!((sel.model_pick, sel.winner), (None, SelectionWinner::S1));
}

#[test]
fn assessment_or_rule_matches_on_either_signal() {
    let (mut a, _, _) = assessor(ModelGoalSettings::default());
    // S1 misses (no "tides" token); the model sees the sea.
    let v = a
        .assess(&retained("sea", None, "ocean sea sea ocean"))
        .unwrap();
    assert_eq!(v, GoalVerdict::Match);
    // S1 matches, the model does not (tide and moon dilute below 0.5? no:
    // pure rust has no tide mass) so S1 carries it.
    let v = a
        .assess(&retained("s1", None, "tides rust rust rust rust"))
        .unwrap();
    assert_eq!(v, GoalVerdict::Match);
    // Neither.
    let v = a.assess(&retained("none", None, "rust rust")).unwrap();
    assert_eq!(v, GoalVerdict::Continue);
    let rec = a.record();
    let [sea, s1, none] = [
        &rec.assessments[0],
        &rec.assessments[1],
        &rec.assessments[2],
    ];
    assert_eq!((sea.s1_match, sea.model_match), (false, Some(true)));
    assert_eq!((s1.s1_match, s1.model_match), (true, Some(false)));
    assert_eq!((none.s1_match, none.model_match), (false, Some(false)));
    assert_eq!(rec.matched, 2);
    assert_eq!(
        rec.s1.matched, 1,
        "S1's own record is untouched by the model"
    );
    assert_eq!(rec.s1.assessments.len(), 3);
}

#[test]
fn model_replaces_and_record_only_rules() {
    let replace = ModelGoalSettings {
        assess_rule: ModelAssessRule::ModelReplaces,
        ..ModelGoalSettings::default()
    };
    let (mut a, _, _) = assessor(replace);
    let text = "tides rust rust rust rust";
    assert_eq!(
        a.assess(&retained("x", None, text)).unwrap(),
        GoalVerdict::Continue
    );
    let record_only = ModelGoalSettings {
        assess_rule: ModelAssessRule::RecordOnly,
        ..ModelGoalSettings::default()
    };
    let (mut b, _, _) = assessor(record_only);
    assert_eq!(
        b.assess(&retained("x", None, text)).unwrap(),
        GoalVerdict::Match
    );
    assert_eq!(
        b.assess(&retained("y", None, "sea ocean")).unwrap(),
        GoalVerdict::Continue
    );
    assert_eq!(b.record().assessments[1].model_match, Some(true));
}

#[test]
fn satisfaction_counts_combined_matches() {
    let (mut a, _, _) = assessor(ModelGoalSettings::default());
    let verdicts: Vec<_> = ["a", "b", "c"]
        .iter()
        .map(|s| a.assess(&retained(s, None, "sea ocean")).unwrap())
        .collect();
    assert_eq!(
        verdicts,
        [
            GoalVerdict::Match,
            GoalVerdict::Match,
            GoalVerdict::Satisfied
        ]
    );
}

#[test]
fn embedder_failure_falls_back_to_s1_and_is_recorded() {
    let (mut a, fail, _) = assessor(ModelGoalSettings::default());
    fail.set(true);
    let pending = [label("a", Some("moon")), label("b", Some("tides tables"))];
    assert_eq!(a.select(&pending), Some(1), "S1 overlap choice");
    assert_eq!(
        a.assess(&retained("hit", None, "tides here")).unwrap(),
        GoalVerdict::Match
    );
    assert_eq!(
        a.assess(&retained("miss", None, "sea ocean")).unwrap(),
        GoalVerdict::Continue
    );
    fail.set(false);
    assert_eq!(
        a.assess(&retained("back", None, "sea ocean")).unwrap(),
        GoalVerdict::Match
    );
    let rec = a.record();
    let reason = ModelStatus::Unavailable {
        reason: "device lost".into(),
    };
    assert_eq!(rec.selections[0].status, reason);
    assert_eq!(rec.selections[0].winner, SelectionWinner::S1);
    assert_eq!(rec.assessments[0].status, reason);
    assert_eq!(rec.assessments[0].similarity_milli, None);
    assert_eq!(rec.assessments[2].status, ModelStatus::Used);
}

#[test]
fn malformed_embedder_answers_count_as_unavailable() {
    struct Short(Stub);
    impl GoalEmbedder for Short {
        fn envelope(&self) -> EmbeddingEnvelopeV1 {
            self.0.envelope()
        }
        fn embed(&mut self, texts: &[&str], r: EmbedRole) -> Result<Vec<Vec<f32>>, String> {
            let mut v = self.0.embed(texts, r)?;
            v.pop();
            Ok(v)
        }
    }
    let mut a = ModelGoalAssessor::new(
        "tides",
        GoalSettings::default(),
        ModelGoalSettings::default(),
        Short(Stub::new().0),
    )
    .unwrap();
    assert_eq!(
        a.assess(&retained("x", None, "tides")).unwrap(),
        GoalVerdict::Match
    );
    assert!(matches!(
        a.record().assessments[0].status,
        ModelStatus::Unavailable { .. }
    ));
}

#[test]
fn envelope_binds_chunker_and_versions_and_refuses_foreign_runs() {
    let (mut a, _, _) = assessor(ModelGoalSettings::default());
    a.assess(&retained("b", None, "sea")).unwrap();
    a.assess(&retained("a", None, "sea")).unwrap();
    let rec = a.record();
    let sources: Vec<_> = rec
        .envelope
        .corpus
        .sources
        .iter()
        .map(|s| &s.source.source)
        .collect();
    assert_eq!(sources, ["a", "b"]);
    assert_eq!(
        rec.envelope.corpus.chunker,
        ModelGoalSettings::default().chunker_id()
    );
    assert_eq!(
        rec.assessments[0].text_hash,
        rec.envelope.corpus.sources[1].text_hash
    );
    rec.ensure_compatible(&rec.envelope).unwrap();

    // A later run of the same model and chunker over other sources.
    let (mut b, _, _) = assessor(ModelGoalSettings::default());
    b.assess(&retained("z", None, "sea")).unwrap();
    let other = b.record();
    assert!(
        rec.ensure_compatible(&other.envelope).is_err(),
        "corpus differs"
    );
    rec.ensure_compatible_space(&other.envelope).unwrap();

    // A foreign space is refused either way.
    let (mut stub, _, _) = Stub::new();
    stub.revision = "stub-2";
    let mut c = ModelGoalAssessor::new(
        "tides",
        GoalSettings::default(),
        ModelGoalSettings::default(),
        stub,
    )
    .unwrap();
    c.assess(&retained("a", None, "sea")).unwrap();
    let foreign = c.record();
    assert!(
        rec.ensure_compatible_space(&foreign.envelope)
            .unwrap_err()
            .contains("foreign")
    );
    // So is a changed chunker.
    let rechunked = ModelGoalSettings {
        max_chunk_chars: 500,
        ..ModelGoalSettings::default()
    };
    let (mut d, _, _) = assessor(rechunked);
    d.assess(&retained("a", None, "sea")).unwrap();
    assert!(
        rec.ensure_compatible_space(&d.record().envelope)
            .unwrap_err()
            .contains("chunker")
    );
}

#[test]
fn record_round_trips_through_json() {
    let (mut a, fail, _) = assessor(ModelGoalSettings::default());
    a.select(&[label("a", Some("sea")), label("b", None)]);
    a.assess(&retained("a", Some("Tides"), "sea ocean"))
        .unwrap();
    fail.set(true);
    a.assess(&retained("b", None, "tides")).unwrap();
    let rec = a.record();
    let json = serde_json::to_string(&rec).unwrap();
    let back: ModelGoalRecord = serde_json::from_str(&json).unwrap();
    assert_eq!(back, rec);
    assert_eq!(back.operation_version, GOAL_MODEL_OPERATION_VERSION);
}

#[test]
fn chunking_is_bounded_overlapping_and_recorded() {
    let settings = ModelGoalSettings {
        max_chunk_chars: 4,
        chunk_overlap_chars: 1,
        max_chunks: 3,
        ..ModelGoalSettings::default()
    };
    let (chunks, truncated) = chunk_text(None, "abcdefghij", &settings);
    assert_eq!(chunks, ["abcd", "defg", "ghij"]);
    assert!(!truncated, "the last window reaches the end");
    let (chunks, truncated) = chunk_text(Some("T"), "abcdefghij", &settings);
    assert_eq!(chunks, ["T", "abcd", "defg"]);
    assert!(truncated);
    // Multi-byte text splits on scalar boundaries.
    let (chunks, _) = chunk_text(None, "éééééé", &settings);
    assert_eq!(chunks, ["éééé", "ééé"]);
    assert!(chunk_text(None, "  ", &settings).0.is_empty());

    let (mut a, _, calls) = assessor(settings);
    a.assess(&retained("long", None, &"sea ".repeat(500)))
        .unwrap();
    let rec = a.record();
    assert_eq!(rec.assessments[0].chunks_embedded, 3);
    assert!(rec.assessments[0].text_truncated);
    assert_eq!(calls.get(), 2, "one goal call, one batch for all chunks");
}

#[test]
fn empty_text_is_unavailable_not_a_crash() {
    let (mut a, _, _) = assessor(ModelGoalSettings::default());
    assert_eq!(
        a.assess(&retained("e", None, "")).unwrap(),
        GoalVerdict::Continue
    );
    assert!(matches!(
        a.record().assessments[0].status,
        ModelStatus::Unavailable { .. }
    ));
}

#[test]
fn unusable_settings_are_refused() {
    for bad in [
        ModelGoalSettings {
            max_chunk_chars: 0,
            ..Default::default()
        },
        ModelGoalSettings {
            chunk_overlap_chars: 1000,
            ..Default::default()
        },
        ModelGoalSettings {
            max_chunks: 0,
            ..Default::default()
        },
        ModelGoalSettings {
            match_similarity_milli: 1001,
            ..Default::default()
        },
    ] {
        let (stub, _, _) = Stub::new();
        assert!(ModelGoalAssessor::new("tides", GoalSettings::default(), bad, stub).is_err());
    }
}
