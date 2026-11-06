// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Title overlay for a derived corpus.
//!
//! Graph and recycle-bin state stay authoritative for page labels. A recall
//! index fills titleless pages in a *clone* of the trace corpus while it is
//! minted; the stored [`BrowsingTrace`]s are never rewritten.

use super::{BrowsingMemory, BrowsingTrace, PageRef};
use std::collections::HashMap;

impl PageRef {
    /// Give a titleless page the label the overlay knows for its address.
    pub fn fill_title(&mut self, titles: &HashMap<&str, &str>) {
        if self.title.is_none()
            && let Some(title) = titles.get(self.url.as_str())
        {
            self.title = Some((*title).to_string());
        }
    }
}

impl BrowsingTrace {
    /// A copy of this trace with missing titles (on `to`, `from` and every
    /// candidate) filled from `titles`, URL to label.
    #[must_use]
    pub fn with_titles(&self, titles: &HashMap<&str, &str>) -> Self {
        let mut trace = self.clone();
        for event in &mut trace.events {
            event.to.fill_title(titles);
            if let Some(from) = &mut event.from {
                from.fill_title(titles);
            }
            for candidate in &mut event.candidates {
                candidate.fill_title(titles);
            }
        }
        trace
    }
}

impl BrowsingMemory {
    /// Clone the authoritative trace corpus with missing page titles filled
    /// from `titles`. Only the clone reaches a derived index.
    #[must_use]
    pub fn traces_with_titles(&self, titles: &HashMap<&str, &str>) -> Vec<BrowsingTrace> {
        self.traces()
            .map(|trace| trace.with_titles(titles))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{TraceEvent, TraceTransition};

    fn page(url: &str, title: Option<&str>) -> PageRef {
        PageRef {
            url: url.into(),
            title: title.map(Into::into),
        }
    }

    #[test]
    fn titles_fill_only_missing_labels_in_a_clone() {
        let mut memory = BrowsingMemory::new(8);
        memory.adopt(BrowsingTrace::from_events(
            "",
            vec![TraceEvent {
                from: Some(page("https://a.example/", None)),
                to: page("https://b.example/", Some("Kept")),
                transition: TraceTransition::LinkClick,
                at_ms: 1,
                dwell_ms: None,
                candidates: vec![page("https://c.example/", None)],
            }],
        ));
        let titles = HashMap::from([
            ("https://a.example/", "Alpha"),
            ("https://b.example/", "Ignored"),
            ("https://c.example/", "Gamma"),
        ]);
        let filled = memory.traces_with_titles(&titles);
        let event = &filled[0].events[0];
        assert_eq!(event.from.as_ref().unwrap().title.as_deref(), Some("Alpha"));
        assert_eq!(event.to.title.as_deref(), Some("Kept"));
        assert_eq!(event.candidates[0].title.as_deref(), Some("Gamma"));
        let stored = memory.traces().next().unwrap();
        assert_eq!(stored.events[0].from.as_ref().unwrap().title, None);
    }
}
