// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Transient lexical recall over caller-owned documents.
//!
//! [`DocumentIndex`] is the neutral half beneath application-specific search
//! projections. A caller groups captures, contributions, or pages into one
//! [`SearchDocument`] before adding it; the index only ranks that document and
//! returns its opaque key. It never owns a store or writes a projection.

use std::collections::HashMap;

use crate::bm25::{Bm25Config, Bm25Index, idf};
use crate::index::FieldWeights;
use crate::tokenize::Tokenizer;

const FIELD_ADDRESS: usize = 0;
const FIELD_TITLE: usize = 1;
const FIELD_BODY: usize = 2;
const FIELD_COUNT: usize = 3;

/// Settings for a transient [`DocumentIndex`].
#[derive(Clone, Copy, Debug, Default)]
pub struct DocumentIndexConfig {
    pub bm25: Bm25Config,
    pub weights: FieldWeights,
    pub tokenizer: Tokenizer,
}

/// One caller-owned document to add to a [`DocumentIndex`].
///
/// `key` is deliberately opaque to this crate. `primary_address` is the
/// address shown back to callers; `aliases` participate in both term recall
/// and absolute-address lookup.
#[derive(Clone, Debug)]
pub struct SearchDocument<K> {
    pub key: K,
    pub primary_address: String,
    pub aliases: Vec<String>,
    pub title: Option<String>,
    pub body: Option<String>,
}

/// A transient document-search result.
#[derive(Clone, Debug, PartialEq)]
pub struct DocumentHit<K> {
    pub key: K,
    pub primary_address: String,
    pub title: Option<String>,
    pub score: f32,
}

struct IndexedDocument<K> {
    key: K,
    primary_address: String,
    title: Option<String>,
}

/// An in-memory weighted-BM25 index over caller-owned documents.
///
/// The index has no filesystem path, serialization, or source-of-truth
/// contract. Rebuild it from the caller's projection whenever that projection
/// changes.
pub struct DocumentIndex<K> {
    config: DocumentIndexConfig,
    postings: Bm25Index,
    docs: Vec<IndexedDocument<K>>,
    /// Every primary address and alias, in insertion order for stable ties.
    by_address: HashMap<String, Vec<u32>>,
}

impl<K: Clone> DocumentIndex<K> {
    /// An empty transient index under explicit settings.
    pub fn new(config: DocumentIndexConfig) -> Self {
        Self {
            postings: Bm25Index::new(config.bm25, config.weights.to_vec()),
            config,
            docs: Vec::new(),
            by_address: HashMap::new(),
        }
    }

    /// Build a transient index from documents the caller has already grouped.
    pub fn from_documents(
        config: DocumentIndexConfig,
        documents: impl IntoIterator<Item = SearchDocument<K>>,
    ) -> Self {
        let mut index = Self::new(config);
        for document in documents {
            index.add(document);
        }
        index
    }

    /// Add one document. Each call is one searchable result, regardless of
    /// how many aliases it carries.
    pub fn add(&mut self, document: SearchDocument<K>) {
        let doc_id = self.docs.len() as u32;
        let mut fields = vec![Vec::new(); FIELD_COUNT];
        let mut addresses = Vec::with_capacity(document.aliases.len() + 1);
        addresses.push(document.primary_address.as_str());
        for alias in &document.aliases {
            if !addresses.contains(&alias.as_str()) {
                addresses.push(alias);
            }
        }
        for address in &addresses {
            self.config
                .tokenizer
                .tokens_into(address, &mut fields[FIELD_ADDRESS]);
            self.by_address
                .entry((*address).to_string())
                .or_default()
                .push(doc_id);
        }
        if let Some(title) = &document.title {
            self.config
                .tokenizer
                .tokens_into(title, &mut fields[FIELD_TITLE]);
        }
        if let Some(body) = &document.body {
            self.config
                .tokenizer
                .tokens_into(body, &mut fields[FIELD_BODY]);
        }
        self.postings.add_document(&fields);
        self.docs.push(IndexedDocument {
            key: document.key,
            primary_address: document.primary_address,
            title: document.title,
        });
    }

    /// The number of caller-grouped documents, not their aliases.
    pub fn len(&self) -> usize {
        self.docs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.docs.is_empty()
    }

    /// Search the address, title, and body fields. A syntactically absolute
    /// address takes the exact lane and therefore finds an alias directly.
    pub fn search(&self, query: &str, limit: usize) -> Vec<DocumentHit<K>> {
        let query = query.trim();
        let limit = limit.max(1);
        let ranked: Vec<(u32, f32)> = if is_absolute_address_query(query) {
            match self.by_address.get(query) {
                Some(docs) => {
                    let score = idf(self.docs.len(), docs.len());
                    docs.iter().take(limit).map(|doc| (*doc, score)).collect()
                },
                None => Vec::new(),
            }
        } else {
            self.postings
                .search(&self.config.tokenizer.tokens(query), limit)
        };
        ranked
            .into_iter()
            .map(|(doc, score)| {
                let doc = &self.docs[doc as usize];
                DocumentHit {
                    key: doc.key.clone(),
                    primary_address: doc.primary_address.clone(),
                    title: doc.title.clone(),
                    score,
                }
            })
            .collect()
    }
}

/// Keep the exact lane deliberately narrow: it is for identifiers, not prose
/// that happens to contain a colon and slashes.
fn is_absolute_address_query(query: &str) -> bool {
    let Some((scheme, rest)) = query.split_once("://") else {
        return false;
    };
    !scheme.is_empty()
        && !rest.is_empty()
        && !query.chars().any(char::is_whitespace)
        && scheme
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'-' | b'.'))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn document(key: u8, address: &str, title: &str, body: &str) -> SearchDocument<u8> {
        SearchDocument {
            key,
            primary_address: address.to_string(),
            aliases: Vec::new(),
            title: Some(title.to_string()),
            body: Some(body.to_string()),
        }
    }

    #[test]
    fn body_only_text_recalls_the_opaque_key() {
        let index = DocumentIndex::from_documents(
            DocumentIndexConfig::default(),
            [document(
                7,
                "https://example.test/ordinary",
                "Ordinary page",
                "mycelial reciprocity field notes",
            )],
        );

        let hits = index.search("mycelial", 5);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].key, 7);
        assert_eq!(hits[0].primary_address, "https://example.test/ordinary");
    }

    #[test]
    fn an_absolute_alias_query_returns_its_primary_document() {
        let mut document = document(3, "https://origin.test/essay", "An essay", "body");
        document
            .aliases
            .push("https://mirror.test/essay".to_string());
        let index = DocumentIndex::from_documents(DocumentIndexConfig::default(), [document]);

        let hits = index.search("https://mirror.test/essay", 5);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].key, 3);
        assert_eq!(hits[0].primary_address, "https://origin.test/essay");
        assert_eq!(hits[0].title.as_deref(), Some("An essay"));
    }

    #[test]
    fn one_grouped_input_document_stays_one_result_despite_aliases() {
        let mut document = document(11, "https://origin.test/note", "Shared note", "cicadas");
        document.aliases = vec![
            "https://mirror-one.test/note".to_string(),
            "https://mirror-two.test/note".to_string(),
            "https://origin.test/note".to_string(),
        ];
        let index = DocumentIndex::from_documents(DocumentIndexConfig::default(), [document]);

        assert_eq!(index.len(), 1);
        let hits = index.search("note", 5);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].key, 11);
    }
}
