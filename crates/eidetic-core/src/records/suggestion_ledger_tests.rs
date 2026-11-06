use super::*;
use crate::MemoryBackend;
use crate::models::recall::PassageSelector;
use crate::models::recall::RecallSourceKind;
use crate::records::{
    ReviewDecision, ReviewState, ReviewStateKind, SuggestionProducer, SuggestionProducerKind,
    TagRef,
};

fn subject() -> RecallSourceId {
    RecallSourceId {
        kind: RecallSourceKind::Clip,
        owner: Some("app".into()),
        source: "clip-1".into(),
        node: None,
    }
}
fn sug(run: &str, concept: &str, keeping: KeepingTier) -> TagSuggestionV1 {
    TagSuggestionV1 {
        subject: subject(),
        subject_version: "rev-1".into(),
        input_text_hash: Hash::of(b"text"),
        concept: TagRef {
            owner: "app".into(),
            concept: concept.into(),
        },
        producer: SuggestionProducer {
            kind: SuggestionProducerKind::Lexical,
            operation_version: "tags/v1".into(),
        },
        run: run.into(),
        settings_digest: Hash::of(b"settings"),
        evidence: vec![PassageSelector { start: 0, end: 4 }],
        score: Some(1.0),
        keeping,
    }
}
fn open<B: Backend>(b: B) -> SuggestionLedger<B> {
    pollster::block_on(SuggestionLedger::open(b)).unwrap()
}
fn state_of<B: Backend>(l: &SuggestionLedger<B>, id: &Hash) -> ReviewState {
    l.suggestions_for(&subject(), &SuggestionFilter::default())
        .into_iter()
        .find(|v| &v.id == id)
        .unwrap()
        .review
}

#[test]
fn record_is_idempotent_and_survives_reopen_with_identical_state() {
    let backend = MemoryBackend::new();
    let mut l = open(backend.clone());
    let s = sug("r1", "tag:ferry", KeepingTier::Retained);
    let (id, first) = pollster::block_on(l.record_suggestion(s.clone())).unwrap();
    assert_eq!(first, Recorded::New);
    let (id2, again) = pollster::block_on(l.record_suggestion(s.clone())).unwrap();
    assert_eq!((id2, again), (id, Recorded::Existing));
    assert_eq!(l.suggestions().len(), 1);
    pollster::block_on(l.reject_suggestion(&id, "me", 5, Some("no".into()))).unwrap();
    let before = (l.suggestions().to_vec(), l.reviews().to_vec());
    drop(l);
    let l = open(backend);
    assert_eq!((l.suggestions().to_vec(), l.reviews().to_vec()), before);
    assert!(matches!(state_of(&l, &id), ReviewState::Rejected { .. }));
}

#[test]
fn accept_after_reopen_yields_draft_citing_the_suggestion() {
    let backend = MemoryBackend::new();
    let mut l = open(backend.clone());
    let (id, _) =
        pollster::block_on(l.record_suggestion(sug("r1", "tag:ferry", KeepingTier::Rebuildable)))
            .unwrap();
    drop(l);
    let mut l = open(backend.clone());
    let draft = pollster::block_on(l.accept_suggestion(&id, "me", 9)).unwrap();
    assert_eq!(draft.cites_suggestion, id);
    drop(l);
    let l = open(backend);
    assert_eq!(state_of(&l, &id).kind(), ReviewStateKind::Accepted);
}

#[test]
fn rejection_and_regeneration_leave_earlier_review_intact() {
    let backend = MemoryBackend::new();
    let mut l = open(backend.clone());
    let old = sug("r1", "tag:ferry", KeepingTier::Retained);
    let (old_id, _) = pollster::block_on(l.record_suggestion(old)).unwrap();
    pollster::block_on(l.accept_suggestion(&old_id, "me", 5)).unwrap();
    let (new_id, _) =
        pollster::block_on(l.record_suggestion(sug("r2", "tag:ferry", KeepingTier::Retained)))
            .unwrap();
    assert_ne!(old_id, new_id);
    pollster::block_on(l.reject_suggestion(&new_id, "me", 6, None)).unwrap();
    drop(l);
    let l = open(backend);
    assert!(matches!(
        state_of(&l, &old_id),
        ReviewState::Accepted { .. }
    ));
    assert!(matches!(
        state_of(&l, &new_id),
        ReviewState::Rejected { .. }
    ));
    assert_eq!(l.reviews().len(), 2);
}

#[test]
fn review_of_unknown_suggestion_is_refused_and_writes_nothing() {
    let backend = MemoryBackend::new();
    let mut l = open(backend.clone());
    let ghost = sug("r9", "tag:ghost", KeepingTier::Retained);
    let review = crate::records::reject(&ghost, "me", 1, None);
    let err = pollster::block_on(l.record_review(review)).unwrap_err();
    assert!(matches!(err, LedgerError::UnknownSuggestion(h) if h == ghost.id()));
    assert!(l.reviews().is_empty());
    assert!(backend.is_empty());
}

#[test]
fn runtime_tier_is_queryable_now_but_not_persisted() {
    let backend = MemoryBackend::new();
    let mut l = open(backend.clone());
    let (rt, _) =
        pollster::block_on(l.record_suggestion(sug("r1", "tag:a", KeepingTier::Runtime))).unwrap();
    let (kept, _) =
        pollster::block_on(l.record_suggestion(sug("r1", "tag:b", KeepingTier::Retained))).unwrap();
    pollster::block_on(l.accept_suggestion(&rt, "me", 2)).unwrap();
    let filter = SuggestionFilter {
        keeping: Some(KeepingTier::Runtime),
        ..Default::default()
    };
    assert_eq!(l.suggestions_for(&subject(), &filter).len(), 1);
    drop(l);
    let l = open(backend);
    assert!(l.get(&rt).is_none());
    assert!(l.get(&kept).is_some());
    assert!(l.reviews().is_empty());
}

#[test]
fn invalid_records_are_refused() {
    let mut l = open(MemoryBackend::new());
    let mut bad = sug("", "tag:a", KeepingTier::Retained);
    assert!(pollster::block_on(l.record_suggestion(bad.clone())).is_err());
    bad.run = "r".into();
    let (id, _) = pollster::block_on(l.record_suggestion(bad.clone())).unwrap();
    let blank = TagSuggestionReviewV1 {
        suggestion: id,
        decision: ReviewDecision::Accepted,
        reviewer: " ".into(),
        at_unix_micros: 1,
    };
    assert!(pollster::block_on(l.record_review(blank)).is_err());
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn state_survives_a_real_reopen_on_redb_and_directory_backends() {
    let dir = tempfile::tempdir().unwrap();
    let redb_path = dir.path().join("l.redb");
    reopen_cycle(|| muniment::RedbBackend::open(&redb_path).unwrap());
    let root = dir.path().join("d");
    reopen_cycle(|| muniment::DirectoryBackend::open(&root).unwrap());
}

/// Write through one handle, drop it entirely, reopen a fresh handle on the
/// same storage and require identical state and a working accept.
#[cfg(not(target_arch = "wasm32"))]
fn reopen_cycle<B: Backend>(opener: impl Fn() -> B) {
    let mut l = open(opener());
    let (id, _) =
        pollster::block_on(l.record_suggestion(sug("run-1", "tag:ferry", KeepingTier::Retained)))
            .unwrap();
    pollster::block_on(l.reject_suggestion(&id, "me", 3, None)).unwrap();
    let snap = (l.suggestions().to_vec(), l.reviews().to_vec());
    drop(l);
    let mut l = open(opener());
    assert_eq!((l.suggestions().to_vec(), l.reviews().to_vec()), snap);
    pollster::block_on(l.accept_suggestion(&id, "me", 4)).unwrap();
    drop(l);
    let l = open(opener());
    assert_eq!(state_of(&l, &id).kind(), ReviewStateKind::Accepted);
    assert_eq!(l.reviews().len(), 2);
}
