// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use eidetic::models::recall::{
    InferencePassage, PassageSelector, RecallSourceId, RecallSourceKind, RetainedSourceText,
};
use eidetic_search::recall::{RecallIndex, RecallIndexConfig, RetainedSourceOwner};
use std::sync::{Arc, Mutex};

struct Owner {
    sources: Vec<RetainedSourceText>,
    heads: Mutex<Vec<Result<Option<String>, String>>>,
    revisions: Mutex<Vec<String>>,
}
impl RetainedSourceOwner for Owner {
    fn snapshot(&self) -> Result<Vec<RetainedSourceText>, String> {
        Ok(self.sources.clone())
    }
    fn scope_revision(&self) -> Result<String, String> {
        let mut revisions = self.revisions.lock().unwrap();
        Ok(if revisions.len() > 1 {
            revisions.remove(0)
        } else {
            revisions[0].clone()
        })
    }
    fn current_version(&self, _: &RecallSourceId) -> Result<Option<String>, String> {
        let mut heads = self.heads.lock().unwrap();
        if heads.len() > 1 {
            heads.remove(0)
        } else {
            heads[0].clone()
        }
    }
}
fn source(name: &str, text: &str) -> RetainedSourceText {
    RetainedSourceText {
        source: RecallSourceId {
            kind: RecallSourceKind::Clip,
            source: name.into(),
            owner: Some("owner".into()),
            node: None,
        },
        version: "v1".into(),
        title: None,
        text: text.into(),
    }
}
fn build(
    sources: Vec<RetainedSourceText>,
    heads: Vec<Result<Option<String>, String>>,
    chunk_chars: usize,
    max_corpus_chars: usize,
) -> Result<RecallIndex, String> {
    RecallIndex::build(
        Arc::new(Owner {
            sources,
            heads: Mutex::new(heads),
            revisions: Mutex::new(vec!["session-1".into()]),
        }),
        RecallIndexConfig {
            chunk_chars,
            max_corpus_chars,
        },
    )
}
fn current() -> Vec<Result<Option<String>, String>> {
    vec![Ok(Some("v1".into()))]
}

#[test]
fn model_free_unicode_passage_id_matches_existing_chunk_contract() {
    let input = source("a", "élan 🦉 nebula");
    let index = build(vec![input.clone()], current(), 8, 100).unwrap();
    let result = index.search("élan", &[input.source.clone()]).unwrap();
    let expected = InferencePassage {
        source: input.source.clone(),
        version: "v1".into(),
        text_hash: eidetic::Hash::of(input.text.as_bytes()),
        selector: PassageSelector { start: 0, end: 8 },
        chunker: "mere.unicode-chunks/v1/8".into(),
        text: "élan 🦉 n".into(),
    };
    assert_eq!(
        serde_json::to_string(&result.passages[0]).unwrap(),
        r#"{"source":{"kind":"clip","owner":"owner","source":"a"},"version":"v1","text_hash":"blake3:267d1f659aa0165da0390a47cdafffe985816e1b557247924de36b42fa42da24","selector":{"start":0,"end":8},"chunker":"mere.unicode-chunks/v1/8","text":"élan 🦉 n"}"#
    );
    assert_eq!(
        result.passages[0].id().to_string(),
        "blake3:9044a40ff62bb4379030ca1b5afaef47c0401a4ab83c135929370dbb2a2b800d"
    );
    assert_eq!(result.passages[0], expected);
    assert_eq!(result.passages[0].id(), expected.id());
}
#[test]
fn model_free_scope_is_explicit_and_content_breaks_score_ties() {
    let a = source("a", "nebula apple");
    let b = source("b", "nebula zebra");
    let index = build(vec![a.clone(), b.clone()], current(), 100, 100).unwrap();
    assert!(index.search("nebula", &[]).unwrap().passages.is_empty());
    let only = index.search("nebula", &[a.source.clone()]).unwrap();
    assert_eq!(only.passages.len(), 1);
    assert_eq!(only.passages[0].source, a.source);
    let both = index
        .search("nebula", &[a.source.clone(), b.source.clone()])
        .unwrap();
    assert_eq!(
        both.passages
            .iter()
            .map(|p| p.text.as_str())
            .collect::<Vec<_>>(),
        ["nebula zebra", "nebula apple"]
    );
}
#[test]
fn model_free_rejects_duplicate_sources_and_invalid_bounds() {
    let a = source("a", "hello");
    let mut changed = a.clone();
    changed.version = "v2".into();
    assert!(build(vec![a.clone(), changed], current(), 8, 100).is_err());
    assert!(build(vec![a.clone(), a.clone()], current(), 8, 100).is_err());
    assert!(build(vec![a.clone()], current(), 0, 100).is_err());
    assert!(build(vec![a], current(), 8, 4).is_err());
}
#[test]
fn model_free_reports_stale_and_unavailable_heads() {
    let a = source("a", "nebula");
    for head in [
        Ok(Some("v2".into())),
        Ok(None),
        Err("owner unavailable".into()),
    ] {
        let index = build(vec![a.clone()], vec![head.clone()], 100, 100).unwrap();
        let result = index.search("nebula", &[a.source.clone()]).unwrap();
        assert!(result.passages.is_empty());
        assert_eq!(result.stale.len(), 1);
        assert_eq!(
            result.stale[0].current_version,
            head.as_ref().ok().cloned().flatten()
        );
        assert_eq!(result.stale[0].error, head.err());
    }
}
#[test]
fn model_free_withdraws_source_when_admission_changes_during_search() {
    let a = source("a", "nebula");
    let index = build(
        vec![a.clone()],
        vec![Ok(Some("v1".into())), Ok(None)],
        100,
        100,
    )
    .unwrap();
    let result = index.search("nebula", &[a.source.clone()]).unwrap();
    assert!(result.passages.is_empty());
    assert_eq!(result.stale.len(), 1);
    assert_eq!(result.stale[0].current_version, None);
}

#[test]
fn model_free_refuses_a_changed_scope_generation_even_when_heads_match() {
    let a = source("a", "nebula");
    let owner = Owner {
        sources: vec![a.clone()],
        heads: Mutex::new(current()),
        revisions: Mutex::new(vec![
            "session-1".into(),
            "session-1".into(),
            "session-1".into(),
            "session-2".into(),
        ]),
    };
    let index = RecallIndex::build(
        Arc::new(owner),
        RecallIndexConfig {
            chunk_chars: 100,
            max_corpus_chars: 100,
        },
    )
    .unwrap();
    assert!(index.search("nebula", &[a.source.clone()]).is_err());
}
#[test]
fn model_free_reports_selected_sources_absent_from_the_index() {
    let a = source("a", "nebula");
    let missing = source("missing", "text");
    let index = build(vec![a], current(), 100, 100).unwrap();
    let result = index.search("nebula", &[missing.source.clone()]).unwrap();
    assert!(result.passages.is_empty());
    assert_eq!(result.unavailable_sources, vec![missing.source.clone()]);
}

#[test]
fn model_free_refuses_scope_change_before_search() {
    let a = source("a", "nebula");
    let owner = Owner {
        sources: vec![a.clone()],
        heads: Mutex::new(current()),
        revisions: Mutex::new(vec!["one".into(), "one".into(), "two".into()]),
    };
    let index = RecallIndex::build(
        Arc::new(owner),
        RecallIndexConfig {
            chunk_chars: 100,
            max_corpus_chars: 100,
        },
    )
    .unwrap();
    assert!(index.search("nebula", &[a.source.clone()]).is_err());
}

#[test]
fn model_free_refuses_scope_change_during_snapshot() {
    let owner = Owner {
        sources: vec![source("a", "nebula")],
        heads: Mutex::new(current()),
        revisions: Mutex::new(vec!["one".into(), "two".into()]),
    };
    assert!(
        RecallIndex::build(
            Arc::new(owner),
            RecallIndexConfig {
                chunk_chars: 100,
                max_corpus_chars: 100
            }
        )
        .is_err()
    );
}

#[test]
fn model_free_refuses_scope_change_before_return() {
    let a = source("a", "nebula");
    let owner = Owner {
        sources: vec![a.clone()],
        heads: Mutex::new(current()),
        revisions: Mutex::new(vec![
            "one".into(),
            "one".into(),
            "one".into(),
            "one".into(),
            "one".into(),
            "two".into(),
        ]),
    };
    let index = RecallIndex::build(
        Arc::new(owner),
        RecallIndexConfig {
            chunk_chars: 100,
            max_corpus_chars: 100,
        },
    )
    .unwrap();
    assert!(index.search("nebula", &[a.source.clone()]).is_err());
}

#[test]
fn model_free_whitespace_text_counts_toward_bounds_and_still_observes_head() {
    let a = source("a", "   ");
    assert!(build(vec![a.clone()], current(), 2, 2).is_err());
    let index = build(vec![a.clone()], vec![Err("unavailable".into())], 2, 3).unwrap();
    let result = index.search("anything", &[a.source.clone()]).unwrap();
    assert!(result.passages.is_empty());
    assert_eq!(result.stale[0].error.as_deref(), Some("unavailable"));
}
