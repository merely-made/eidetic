use super::*;
use crate::models::recall::RecallSourceKind;

fn subject() -> RecallSourceId {
    RecallSourceId {
        kind: RecallSourceKind::Clip,
        owner: Some("app".into()),
        source: "clip-1".into(),
        node: None,
    }
}
fn tag(owner: &str, concept: &str) -> TagRef {
    TagRef {
        owner: owner.into(),
        concept: concept.into(),
    }
}
fn sug(run: &str) -> TagSuggestionV1 {
    TagSuggestionV1 {
        subject: subject(),
        subject_version: "rev-1".into(),
        input_text_hash: Hash::of(b"text"),
        concept: tag("app", "tag:ferry"),
        producer: SuggestionProducer {
            kind: SuggestionProducerKind::Model {
                model: "m".into(),
                revision: "7".into(),
            },
            operation_version: "tags/v1".into(),
        },
        run: run.into(),
        settings_digest: Hash::of(b"settings"),
        evidence: vec![PassageSelector { start: 0, end: 4 }],
        score: Some(0.5),
        keeping: KeepingTier::Retained,
    }
}
fn rv(s: &TagSuggestionV1, d: ReviewDecision, who: &str, at: i64) -> TagSuggestionReviewV1 {
    TagSuggestionReviewV1 {
        suggestion: s.id(),
        decision: d,
        reviewer: who.into(),
        at_unix_micros: at,
    }
}

#[test]
fn id_is_stable_across_serde_and_every_record_round_trips() {
    let s = sug("run-1");
    let back: TagSuggestionV1 = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
    assert_eq!(back, s);
    assert_eq!(back.id(), s.id());
    let (r, d) = accept(&s, "me", 5);
    let r2: TagSuggestionReviewV1 =
        serde_json::from_str(&serde_json::to_string(&r).unwrap()).unwrap();
    let d2: PersonalTagAssertionDraftV1 =
        serde_json::from_str(&serde_json::to_string(&d).unwrap()).unwrap();
    assert_eq!((r2, d2), (r, d));
    let rj = reject(&s, "me", 6, Some("no".into()));
    let rj2: TagSuggestionReviewV1 =
        serde_json::from_str(&serde_json::to_string(&rj).unwrap()).unwrap();
    assert_eq!(rj2, rj);
}

#[test]
fn regeneration_is_a_new_id_and_touches_nothing_earlier() {
    let old = sug("run-1");
    let (review, draft) = accept(&old, "me", 5);
    let new = sug("run-2");
    assert_ne!(old.id(), new.id());
    let reviews = [review.clone()];
    assert_eq!(review_state(&new.id(), &reviews), ReviewState::Unreviewed);
    assert!(matches!(
        review_state(&old.id(), &reviews),
        ReviewState::Accepted { .. }
    ));
    assert_eq!(draft.cites_suggestion, old.id());
}

#[test]
fn accept_cites_the_suggestion_and_reject_makes_no_assertion() {
    let s = sug("run-1");
    let (r, d) = accept(&s, "me", 1);
    assert_eq!(r.suggestion, s.id());
    assert_eq!(d.cites_suggestion, s.id());
    assert_eq!(
        (d.concept.clone(), d.subject_version.as_str()),
        (s.concept.clone(), "rev-1")
    );
    assert_eq!(d.asserter, "me");
    // A later rejection changes only review state; the earlier draft is unchanged.
    let before = d.clone();
    let rj = reject(&s, "me", 2, None);
    assert_eq!(rj.decision, ReviewDecision::Rejected { reason: None });
    assert_eq!(before, d);
    let reviews = [r, rj];
    assert!(matches!(
        review_state(&s.id(), &reviews),
        ReviewState::Rejected { .. }
    ));
}

#[test]
fn equal_concepts_with_different_owners_are_distinct() {
    let a = sug("run-1");
    let mut b = sug("run-1");
    b.concept = tag("other", "tag:ferry");
    assert_ne!(a.concept, b.concept);
    assert_ne!(a.id(), b.id());
    let (_, da) = accept(&a, "me", 1);
    let (_, db) = accept(&b, "me", 1);
    assert_ne!(da.concept, db.concept);
}

#[test]
fn review_state_latest_wins_with_deterministic_ties() {
    let s = sug("run-1");
    let acc = rv(&s, ReviewDecision::Accepted, "a", 10);
    let rej = rv(
        &s,
        ReviewDecision::Rejected {
            reason: Some("x".into()),
        },
        "a",
        10,
    );
    let early = rv(&s, ReviewDecision::Rejected { reason: None }, "z", 3);
    for order in [
        [&acc, &rej, &early],
        [&early, &rej, &acc],
        [&rej, &acc, &early],
    ] {
        let v: Vec<_> = order.iter().map(|r| (*r).clone()).collect();
        assert_eq!(review_state(&s.id(), &v).kind(), ReviewStateKind::Rejected);
    }
    let later = rv(&s, ReviewDecision::Accepted, "a", 11);
    let v = [rej.clone(), later, early];
    assert_eq!(review_state(&s.id(), &v).kind(), ReviewStateKind::Accepted);
    // Other suggestions' reviews are ignored.
    let other = sug("run-2");
    assert_eq!(review_state(&other.id(), &v), ReviewState::Unreviewed);
}

#[test]
fn query_filters_by_every_axis_without_personal_tags() {
    let a = sug("run-1");
    let mut b = sug("run-2");
    b.keeping = KeepingTier::Runtime;
    b.producer.kind = SuggestionProducerKind::Lexical;
    b.concept = tag("app", "tag:boat");
    let mut elsewhere = sug("run-1");
    elsewhere.subject.source = "clip-2".into();
    let all = [a.clone(), b.clone(), elsewhere];
    let reviews = [accept(&a, "me", 1).0];
    let q = |f: SuggestionFilter| suggestions_for(&subject(), &all, &reviews, &f);
    assert_eq!(q(SuggestionFilter::default()).len(), 2);
    let f = SuggestionFilter {
        review: Some(ReviewStateKind::Accepted),
        ..Default::default()
    };
    assert_eq!(q(f)[0].id, a.id());
    let f = SuggestionFilter {
        keeping: Some(KeepingTier::Runtime),
        ..Default::default()
    };
    assert_eq!(q(f)[0].suggestion.run, "run-2");
    let f = SuggestionFilter {
        producer: Some(ProducerClass::Lexical),
        ..Default::default()
    };
    assert_eq!(q(f)[0].id, b.id());
    let f = SuggestionFilter {
        run: Some("run-1".into()),
        ..Default::default()
    };
    assert_eq!(q(f).len(), 1);
    let f = SuggestionFilter {
        concept: Some(tag("app", "tag:boat")),
        ..Default::default()
    };
    assert_eq!(q(f)[0].id, b.id());
    let f = SuggestionFilter {
        review: Some(ReviewStateKind::Unreviewed),
        ..Default::default()
    };
    assert_eq!(q(f)[0].id, b.id());
}

#[test]
fn validation_refusals() {
    assert!(validate_suggestion(&sug("r")).is_ok());
    let bad: Vec<Box<dyn Fn(&mut TagSuggestionV1)>> = vec![
        Box::new(|s| s.concept.owner = " ".into()),
        Box::new(|s| s.concept.concept.clear()),
        Box::new(|s| s.run.clear()),
        Box::new(|s| s.subject_version.clear()),
        Box::new(|s| s.score = Some(f32::NAN)),
        Box::new(|s| s.score = Some(1.5)),
        Box::new(|s| s.score = Some(-0.1)),
        Box::new(|s| s.evidence = vec![PassageSelector { start: 3, end: 3 }]),
        Box::new(|s| s.producer.operation_version.clear()),
        Box::new(|s| s.producer.kind = SuggestionProducerKind::Rule { name: "".into() }),
    ];
    for f in bad {
        let mut s = sug("r");
        f(&mut s);
        assert!(validate_suggestion(&s).is_err());
    }
    let (mut r, mut d) = accept(&sug("r"), "me", 1);
    assert!(validate_review(&r).is_ok() && validate_assertion_draft(&d).is_ok());
    r.reviewer.clear();
    d.asserter.clear();
    assert!(validate_review(&r).is_err() && validate_assertion_draft(&d).is_err());
}
