use super::*;

fn stamp() -> SourceStamp {
    SourceStamp {
        document: "memory:document".into(),
        instance: 17,
        revision: 4,
    }
}
fn edit(start: usize, end: usize, text: &str) -> TextEdit {
    TextEdit {
        start,
        end,
        text: text.into(),
    }
}

#[test]
fn unicode_crlf_batches_keep_both_coordinate_spaces_and_exact_source() {
    let source = "α\r\nold 雪 tail";
    let base = stamp();
    let proposal = Proposal::prepare(
        base.clone(),
        source,
        vec![edit(12, 16, ""), edit(4, 7, "newer"), edit(0, 0, "# ")],
    )
    .unwrap();
    assert_eq!(proposal.preview(), "# α\r\nnewer 雪 ");
    assert_eq!(source, "α\r\nold 雪 tail");
    assert_eq!(base, stamp());
    let app = proposal.application(&base, source).unwrap();
    assert_eq!(app.receipt.result.revision, 5);
    assert_eq!(app.receipt.base, base);
    for (edit, range) in proposal.edits().iter().zip(&app.receipt.ranges) {
        assert_eq!(&app.text[range.result_start..range.result_end], edit.text);
        assert_eq!((range.base_start, range.base_end), (edit.start, edit.end));
    }
}

#[test]
fn bad_boundaries_and_ambiguous_batches_refuse() {
    for edits in [
        vec![],
        vec![edit(1, 2, "x")],
        vec![edit(0, 99, "x")],
        vec![edit(2, 0, "x")],
        vec![edit(0, 2, "a"), edit(0, 0, "b")],
        vec![edit(0, 4, "a"), edit(2, 2, "b")],
        vec![edit(0, 0, "a"), edit(0, 0, "b")],
    ] {
        assert!(Proposal::prepare(stamp(), "αβ", edits).is_err());
    }
    let p = Proposal::prepare(stamp(), "abcd", vec![edit(0, 2, ""), edit(2, 4, "")]).unwrap();
    assert_eq!(p.preview(), "");
}

#[test]
fn document_instance_revision_and_source_are_independent_guards() {
    let base = stamp();
    let p = Proposal::prepare(base.clone(), "before", vec![edit(0, 6, "after")]).unwrap();
    let mut current = base.clone();
    current.document = "memory:another".into();
    assert_eq!(
        p.application(&current, "before"),
        Err(EditError::WrongDocument)
    );
    current = base.clone();
    current.instance += 1;
    assert_eq!(
        p.application(&current, "before"),
        Err(EditError::WrongDocument)
    );
    current = base.clone();
    current.revision += 2;
    // Undo to equal text must not revive a proposal from an older revision.
    assert_eq!(
        p.application(&current, "before"),
        Err(EditError::StaleRevision)
    );
    assert_eq!(
        p.application(&base, "changed"),
        Err(EditError::SourceChanged)
    );
    assert!(p.application(&base, "before").is_ok());
}

#[test]
fn exhaustion_is_refused_and_noop_edits_still_have_explicit_receipts() {
    let mut base = stamp();
    base.revision = u64::MAX;
    assert_eq!(
        Proposal::prepare(base, "a", vec![edit(0, 1, "b")]).unwrap_err(),
        EditError::RevisionExhausted
    );
    let p = Proposal::prepare(stamp(), "a", vec![edit(0, 1, "a")]).unwrap();
    let a = p.application(&stamp(), "a").unwrap();
    assert_eq!(a.text, "a");
    assert_eq!(a.receipt.result.revision, 5);
}

#[test]
fn untrusted_edits_use_strict_wire_shape_but_cannot_supply_a_preview() {
    let input: TextEdit = serde_json::from_str(r#"{"start":0,"end":0,"text":"literal"}"#).unwrap();
    assert_eq!(input, edit(0, 0, "literal"));
    assert!(
        serde_json::from_str::<TextEdit>(r#"{"start":0,"end":0,"text":"x","preview":"forged"}"#)
            .is_err()
    );
    let p = Proposal::prepare(stamp(), "text", vec![input]).unwrap();
    let json = serde_json::to_value(p.application(&stamp(), "text").unwrap().receipt).unwrap();
    assert_eq!(json["base"]["instance"], 17);
    assert_eq!(json["ranges"][0]["result_end"], 7);
}

#[test]
fn captured_base_is_the_single_observation_for_checking_and_later_proposals() {
    let base = SourceSnapshot::capture(stamp(), "α old\r\n");
    let first = base.propose(vec![edit(3, 6, "one")]).unwrap();
    let second = base.clone().propose(vec![edit(3, 6, "two")]).unwrap();
    assert_eq!(base.source(), "α old\r\n");
    assert_eq!(first.preview(), "α one\r\n");
    assert_eq!(second.preview(), "α two\r\n");
    assert_eq!(first.base(), base.stamp());
    assert!(base.check(&stamp(), base.source()).is_ok());
    assert_eq!(
        base.check(&stamp(), "α changed\r\n"),
        Err(EditError::SourceChanged)
    );
    let mut later = stamp();
    later.revision += 2;
    assert_eq!(
        base.check(&later, base.source()),
        Err(EditError::StaleRevision)
    );
    assert_eq!(
        second.application(&later, base.source()),
        Err(EditError::StaleRevision)
    );
    let mut other = stamp();
    other.instance += 1;
    assert_eq!(
        base.check(&other, base.source()),
        Err(EditError::WrongDocument)
    );
}
