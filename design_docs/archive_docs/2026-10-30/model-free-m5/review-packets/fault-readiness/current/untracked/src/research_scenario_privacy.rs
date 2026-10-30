// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
//! Scope-limited scenario projections; never change source admission.
use crate::research_search::Member;
use crate::saved_sources::{Admission, Entry, Key};

pub(crate) fn capture_selected(selected: &Option<Vec<Member>>, key: &Key) -> bool {
    selected
        .as_ref()
        .is_none_or(|members| members.contains(&Member::Capture(key.clone())))
}
/// The rendered cache is keyed only by node, not full session/graph identity.
/// With an explicit subset, omit these bodies rather than guess their owner.
pub(crate) fn clip_node_selected(selected: &Option<Vec<Member>>, _node: uuid::Uuid) -> bool {
    selected.is_none()
}
pub(crate) fn admissions(selected: &Option<Vec<Member>>, input: Vec<Admission>) -> Vec<Admission> {
    input
        .into_iter()
        .map(|mut item| {
            if !capture_selected(selected, &item.key) {
                item.title.clear();
                item.url.clear();
                item.error = item.error.map(|_| "unselected source unavailable".into());
            }
            item
        })
        .collect()
}
pub(crate) fn entries<'a>(selected: &Option<Vec<Member>>, input: &'a [Entry]) -> Vec<&'a Entry> {
    input
        .iter()
        .filter(|entry| capture_selected(selected, &entry.admission.key))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn model_free_saved_source_projection_hides_unselected_canaries() {
        let make = |label: &str| {
            let key = Key {
                session: crate::panes::SessionId::new(),
                graph: crate::panes::GraphId::new(),
                node: uuid::Uuid::new_v4(),
                annotation_manifest: format!("sealed-v1:{label}"),
            };
            let admission = Admission {
                key,
                occurrence: 0,
                title: format!("{label}-private-title"),
                url: format!("{label}-private-url"),
                error: None,
            };
            let content = crate::saved_sources::Content {
                title: admission.title.clone(),
                origin: admission.url.clone(),
                captured_at_ms: 1,
                text: format!("{label}-private-body"),
                text_hash: "hash".into(),
                raw_manifest: "raw".into(),
            };
            Entry {
                admission,
                content: Some(content),
                error: None,
            }
        };
        let inputs = vec![make("first"), make("second")];
        let all_admissions: Vec<_> = inputs.iter().map(|entry| entry.admission.clone()).collect();
        let check = |selection: Option<Vec<Member>>| {
            serde_json::json!({
            "admissions":admissions(&selection,all_admissions.clone()), "entries":entries(&selection,&inputs)
        }).to_string()
        };
        let empty = check(Some(vec![]));
        assert!(
            !empty.contains("private-title")
                && !empty.contains("private-url")
                && !empty.contains("private-body")
        );
        let subset = check(Some(vec![Member::Capture(inputs[0].admission.key.clone())]));
        assert!(subset.contains("first-private-body"));
        assert!(
            !subset.contains("second-private-title")
                && !subset.contains("second-private-url")
                && !subset.contains("second-private-body")
        );
        assert!(check(None).contains("second-private-body"));
        assert!(clip_node_selected(&None, uuid::Uuid::new_v4()));
        assert!(!clip_node_selected(&Some(vec![]), uuid::Uuid::new_v4()));
    }
}
