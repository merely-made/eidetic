// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
use crate::models::recall::{RecallSourceId, RecallSourceKind};
#[test]
fn candidate_review_binds_all_decisions_to_status_and_links_without_claiming_truth() {
    let a = passage("a", "The ship departs on Monday.");
    let b = passage("b", "The ship departs on Tuesday.");
    let irrelevant = passage("c", "The museum opens at ten.");
    let ps = vec![(7, &a), (2, &b), (9, &irrelevant)];
    let raw = serde_json::json!({"claims":[{"review":{"candidates":[{"handle":7,"answers":true},{"handle":2,"answers":true},{"handle":9,"answers":false}],"agreement":"different"},"status":"conflicting","text":"The sources disagree: Monday or Tuesday.","evidence":[{"handle":7,"quote":a.text},{"handle":2,"quote":b.text}]}]});
    let check = |v: &serde_json::Value| AnswerSupport::assess(&v.to_string(), &ps);
    let support = check(&raw);
    assert!(support.error.is_none());
    assert_eq!(support.semantic_review, SemanticReview::Unreviewed);
    assert_eq!(
        support.claims[0].review.as_ref().unwrap().candidates.len(),
        3
    );
    let restored: AnswerSupport =
        serde_json::from_slice(&serde_json::to_vec(&support).unwrap()).unwrap();
    assert_eq!(restored, support);
    for mutation in 0..7 {
        let mut bad = raw.clone();
        let c = &mut bad["claims"][0];
        match mutation {
            0 => {
                c["review"]["candidates"].as_array_mut().unwrap().pop();
            }
            1 => {
                c["review"]["candidates"].as_array_mut().unwrap().swap(0, 1);
            }
            2 => c["review"]["agreement"] = serde_json::json!("same"),
            3 => c["status"] = serde_json::json!("supported"),
            4 => c["review"]["candidates"][0]["answers"] = serde_json::json!(false),
            5 => c["evidence"][1] = serde_json::json!({"handle":9,"quote":irrelevant.text}),
            _ => c["evidence"][0]["quote"] = serde_json::json!("The ship departs on Friday."),
        }
        let failed = check(&bad);
        assert!(failed.error.is_some(), "mutation {mutation}");
        assert!(failed.claims.is_empty());
    }
    // The structural checker must not silently certify a false relevance
    // decision: the model can still label an unrelated passage as relevant.
    let false_judgment = serde_json::json!({"claims":[{"review":{"candidates":[{"handle":7,"answers":false},{"handle":2,"answers":false},{"handle":9,"answers":true}],"agreement":"single"},"status":"supported","text":irrelevant.text,"evidence":[{"handle":9,"quote":irrelevant.text}]}]});
    let checked = check(&false_judgment);
    assert!(checked.error.is_none());
    assert_eq!(checked.semantic_review, SemanticReview::Unreviewed);
    let none = serde_json::json!({"claims":[{"review":{"candidates":[{"handle":7,"answers":false},{"handle":2,"answers":false},{"handle":9,"answers":false}],"agreement":"none"},"status":"insufficient","text":"The supplied evidence does not state the fare.","evidence":[]}]});
    assert!(check(&none).error.is_none());
    let mut duplicate = ps.clone();
    duplicate[1].0 = 7;
    assert!(
        AnswerSupport::assess(&raw.to_string(), &duplicate)
            .error
            .is_some()
    );
}
#[test]
fn quoted_candidate_decisions_preserve_exact_text_and_refuse_a_foreign_quote() {
    let p = passage("source", "The orchard crop is pears.");
    let ps = vec![(4, &p)];
    let mut raw = serde_json::json!({"claims":[{"review":{"candidates":[{"handle":4,"quote":p.text,"answers":true}],"agreement":"single"},"status":"supported","text":p.text,"evidence":[{"handle":4,"quote":p.text}]}]});
    let support = AnswerSupport::assess(&raw.to_string(), &ps);
    assert!(support.error.is_none());
    assert_eq!(
        support.claims[0].review.as_ref().unwrap().candidates[0]
            .quote
            .as_deref(),
        Some(p.text.as_str())
    );
    let reopened: AnswerSupport =
        serde_json::from_slice(&serde_json::to_vec(&support).unwrap()).unwrap();
    assert_eq!(reopened, support);
    raw["claims"][0]["review"]["candidates"][0]["quote"] =
        serde_json::json!("The orchard crop is apples.");
    assert!(AnswerSupport::assess(&raw.to_string(), &ps).error.is_some());
    let facts = RequestedFactSet::derive("What is the orchard crop?").unwrap();
    raw["claims"][0]["fact"] = serde_json::json!(facts.facts[0]);
    raw["claims"][0]["review"]["candidates"][0]["quote"] = serde_json::json!(p.text);
    raw["claims"][0]["review"]["candidates"][0]["requested"] =
        serde_json::json!(facts.facts[0].text);
    assert!(
        AnswerSupport::assess_requested(&raw.to_string(), &ps, Some(&facts))
            .error
            .is_none()
    );
    raw["claims"][0]["review"]["candidates"][0]["requested"] =
        serde_json::json!("the orchard owner");
    assert!(
        AnswerSupport::assess_requested(&raw.to_string(), &ps, Some(&facts))
            .error
            .is_some()
    );
}
fn passage(source: &str, text: &str) -> InferencePassage {
    InferencePassage {
        source: RecallSourceId {
            kind: RecallSourceKind::Clip,
            owner: Some("owner".into()),
            source: source.into(),
            node: None,
        },
        version: "v1".into(),
        text_hash: crate::Hash::of(text.as_bytes()),
        selector: PassageSelector {
            start: 7,
            end: 7 + text.chars().count(),
        },
        chunker: "test/v1".into(),
        text: text.into(),
    }
}
#[test]
fn requested_facts_refuse_missing_extra_and_reordered_claims() {
    let q =
        "What are the opening time and the ticket cost? Give one claim for each, in that order.";
    let facts = RequestedFactSet::derive(q).unwrap();
    let claims:Vec<_>=facts.facts.iter().map(|f|serde_json::json!({"fact":f,"status":"insufficient","text":format!("No supplied {}.",f.text),"evidence":[]})).collect();
    let raw = serde_json::json!({"claims":claims}).to_string();
    let support = AnswerSupport::assess_requested(&raw, &[], Some(&facts));
    assert!(support.error.is_none());
    assert_eq!(support.requested_facts, Some(facts.id()));
    assert_eq!(support.claims[0].fact.as_ref(), Some(&facts.facts[0]));
    assert!(AnswerSupport::assess(&raw, &[]).error.is_some());
    for bad in [
        vec![claims[0].clone()],
        vec![claims[1].clone(), claims[0].clone()],
        vec![claims[0].clone(), claims[1].clone(), claims[1].clone()],
    ] {
        let s = AnswerSupport::assess_requested(
            &serde_json::json!({"claims":bad}).to_string(),
            &[],
            Some(&facts),
        );
        assert!(s.error.is_some());
        assert!(s.claims.is_empty());
    }
}

#[test]
fn exact_links_use_unicode_offsets_and_do_not_certify_entailment() {
    let p = passage("a", "🦉 élan: the launch is on Thursday.");
    let raw = r#"{"claims":[{"status":"supported","text":"An unrelated interpretation","evidence":[{"handle":1,"quote":"the launch is on Thursday."}]}]}"#;
    let result = AnswerSupport::assess(raw, &[(1, &p)]);
    assert_eq!(result.semantic_review, SemanticReview::Unreviewed);
    assert!(result.claims[0].issues.is_empty());
    assert_eq!(
        result.claims[0].evidence[0]
            .selector
            .as_ref()
            .unwrap()
            .start,
        15
    );
    assert_eq!(result.raw_output, ManifestId::of_blob(raw.as_bytes()));
}
#[test]
fn conflict_requires_distinct_sources_and_keeps_both_versions() {
    let a = passage("a", "The launch is on Thursday.");
    let b = passage("b", "The launch is on Saturday.");
    let raw = r#"{"claims":[{"status":"conflicting","text":"Dates disagree","evidence":[{"handle":1,"quote":"The launch is on Thursday."},{"handle":2,"quote":"The launch is on Saturday."}]}]}"#;
    let result = AnswerSupport::assess(raw, &[(1, &a), (2, &b)]);
    assert!(result.claims[0].issues.is_empty());
    assert_ne!(
        result.claims[0].evidence[0].passage,
        result.claims[0].evidence[1].passage
    );
    assert!(
        !AnswerSupport::assess(raw, &[(1, &a), (2, &a)]).claims[0]
            .issues
            .is_empty()
    );
}
#[test]
fn malformed_unknown_and_invented_support_fail_closed() {
    let p = passage("a", "The launch is on Thursday.");
    for raw in [
        r#"prefix {"claims":[]}"#,
        r#"{"claims":[]}"#,
        r#"{"claims":["#,
    ] {
        assert!(AnswerSupport::assess(raw, &[]).error.is_some());
    }
    let raw = r#"{"claims":[{"status":"supported","text":"Saturday","evidence":[{"handle":99,"quote":"The launch is on Saturday."},{"handle":1,"quote":"The launch is on Saturday."}]}]}"#;
    let result = AnswerSupport::assess(raw, &[(1, &p)]);
    assert_eq!(result.claims[0].issues.len(), 3);
    assert!(
        result.claims[0]
            .evidence
            .iter()
            .all(|e| e.selector.is_none())
    );
}
#[test]
fn chunk_quotes_preserve_the_full_source_hash_and_absolute_range() {
    let mut p = passage("chunk", "The launch is on Thursday.");
    p.text_hash = crate::Hash::of(b"A larger canonical source containing the retained chunk.");
    let raw = r#"{"claims":[{"status":"supported","text":"Thursday launch","evidence":[{"handle":1,"quote":"The launch is on Thursday."}]}]}"#;
    let result = AnswerSupport::assess(raw, &[(1, &p)]);
    assert!(result.claims[0].issues.is_empty());
    assert_eq!(
        result.claims[0].evidence[0]
            .selector
            .as_ref()
            .unwrap()
            .start,
        7
    );
    assert_eq!(result.claims[0].evidence[0].passage, Some(p.id()));
}
#[test]
fn corrupt_passage_ranges_and_repeated_claims_are_reported() {
    let mut p = passage("a", "The launch is on Thursday.");
    p.selector.start = usize::MAX;
    let claim = r#"{"status":"supported","text":"Thursday launch","evidence":[{"handle":1,"quote":"The launch is on Thursday."}]}"#;
    let raw = format!("{{\"claims\":[{claim},{claim}]}}");
    let result = AnswerSupport::assess(&raw, &[(1, &p)]);
    assert!(result.claims[0].evidence[0].selector.is_none());
    assert!(
        result.claims[1]
            .issues
            .iter()
            .any(|i| i.contains("repeated"))
    );
}
#[test]
fn insufficient_is_a_retained_model_judgment_not_an_absence_proof() {
    let raw =
        r#"{"claims":[{"status":"insufficient","text":"No budget was supplied.","evidence":[]}]}"#;
    let result = AnswerSupport::assess(raw, &[]);
    assert!(result.claims[0].issues.is_empty());
    let reopened: AnswerSupport =
        serde_json::from_slice(&serde_json::to_vec(&result).unwrap()).unwrap();
    assert_eq!(reopened, result);
    assert_eq!(reopened.id(), result.id());
}
