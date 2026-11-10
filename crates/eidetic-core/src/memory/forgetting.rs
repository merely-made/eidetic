// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0


//! Forgetting's apply half: dropping the cached content a forget proposal
//! names (eidetic R0: cached content only, never graph truth or codicils).
//! The graph-reading propose half stays in pandect's furnace (engine split
//! D45).

use crate::{Result, Store, content_store};

/// A proposal to forget (evict the cached content of) a set of short-term nodes.
///
/// Athanor emits this; the host decides whether to apply it (the R0 propose/apply
/// split). It names urls, not nodes: forgetting drops cached **content**, leaving the
/// graph node in place.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ForgetProposal {
    /// The urls whose cached content is eligible for eviction.
    pub urls: Vec<String>,
}

impl ForgetProposal {
    pub fn is_empty(&self) -> bool {
        self.urls.is_empty()
    }

    pub fn len(&self) -> usize {
        self.urls.len()
    }
}

/// Apply a forget proposal: drop the cached content for each url. Returns how many
/// urls had content actually removed (genuine feedback, never a placebo).
///
/// Drops only short-term cached blobs via [`content_store::evict_content`] (R0: not
/// graph truth, not codicils). An accepted proposal the host hands back here.
pub async fn apply_forgetting(store: &mut dyn Store, proposal: &ForgetProposal) -> Result<usize> {
    let mut dropped = 0;
    for url in &proposal.urls {
        if content_store::evict_content(store, url).await? {
            dropped += 1;
        }
    }
    Ok(dropped)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content_store::{StoredContent, save_content};
    use muniment::MemoryBackend as MemStore;

    #[test]
    fn apply_drops_proposed_content_and_leaves_the_rest() {
        pollster::block_on(async {
            let mut store = MemStore::default();
            let page = |b: &str| StoredContent {
                content_type: None,
                body: b.as_bytes().to_vec(),
            };
            save_content(&mut store, "https://stale.example/", &page("old"))
                .await
                .unwrap();
            save_content(&mut store, "https://fresh.example/", &page("new"))
                .await
                .unwrap();

            let proposal = ForgetProposal {
                urls: vec!["https://stale.example/".to_string()],
            };
            let dropped = apply_forgetting(&mut store, &proposal).await.unwrap();
            assert_eq!(dropped, 1, "one url's content removed");

            assert!(
                content_store::load_content(&mut store, "https://stale.example/")
                    .await
                    .unwrap()
                    .is_none(),
                "the forgotten content is gone",
            );
            assert!(
                content_store::load_content(&mut store, "https://fresh.example/")
                    .await
                    .unwrap()
                    .is_some(),
                "un-proposed content is untouched",
            );
        });
    }
}
