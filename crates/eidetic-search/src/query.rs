// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. SPDX-License-Identifier: MPL-2.0
//! Deterministic search over a frozen, explicitly admitted research snapshot.
//! This pure computation grants no authority: hosts recheck admission before
//! publishing results and before reopening exact evidence.
use crate::index::FieldWeights;
use crate::tokenize::{SEGMENTATION_PROFILE, TOKENIZER_NAME, Tokenizer};
use crate::{DocumentIndex, DocumentIndexConfig, SearchDocument};
use eidetic::models::recall::{
    InferencePassage, PassageSelector, RecallSourceId, RetainedSourceText,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProducerKind {
    Capture,
    Selection,
    Note,
    Other,
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct TagId {
    pub owner: String,
    pub concept: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ResearchSource {
    pub retained: RetainedSourceText,
    pub producer: ProducerKind,
    pub address: Option<String>,
    pub date: Option<i64>,
    pub tags: Vec<TagId>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Coverage {
    Complete,
    Incomplete { exclusions: Vec<String> },
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ResearchSnapshot {
    pub generation: String,
    pub scope: Vec<RecallSourceId>,
    pub sources: Vec<ResearchSource>,
    pub coverage: Coverage,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum QueryMode {
    Browse,
    Terms(String),
    Phrase(String),
    Address(String),
    Prefix(String),
}
impl Default for QueryMode {
    fn default() -> Self {
        Self::Browse
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Alias {
    pub term: String,
    pub expansions: Vec<String>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchQuery {
    pub mode: QueryMode,
    pub sources: Option<Vec<RecallSourceId>>,
    pub kinds: Vec<ProducerKind>,
    pub date_from: Option<i64>,
    pub date_to: Option<i64>,
    pub tags: Vec<TagId>,
    pub aliases: Vec<Alias>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SearchSettings {
    pub weights: FieldWeights,
    pub snippet_context: usize,
    pub offset: usize,
    pub limit: usize,
    pub max_sources: usize,
    /// Maximum canonical body characters in the supplied snapshot.
    pub max_corpus_chars: usize,
    /// Maximum characters in snapshot metadata and identities.
    pub max_metadata_chars: usize,
    /// Maximum characters in query text, filters and aliases.
    pub max_query_chars: usize,
}
impl Default for SearchSettings {
    fn default() -> Self {
        Self {
            weights: FieldWeights::default(),
            snippet_context: 80,
            offset: 0,
            limit: 50,
            max_sources: 10_000,
            max_corpus_chars: 10_000_000,
            max_metadata_chars: 1_000_000,
            max_query_chars: 100_000,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct InputIdentity {
    pub source: RecallSourceId,
    pub version: String,
    pub text_hash: eidetic::Hash,
    pub producer: ProducerKind,
    pub title: Option<String>,
    pub address: Option<String>,
    pub date: Option<i64>,
    pub tags: Vec<TagId>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SearchHit {
    pub source: RecallSourceId,
    pub version: String,
    pub producer: ProducerKind,
    pub title: Option<String>,
    pub address: Option<String>,
    pub date: Option<i64>,
    pub tags: Vec<TagId>,
    pub score: f32,
    pub matches: Vec<PassageSelector>,
    pub passage: Option<InferencePassage>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SearchResult {
    pub generation: String,
    pub scope: Vec<RecallSourceId>,
    pub inputs: Vec<InputIdentity>,
    pub query: SearchQuery,
    pub settings: SearchSettings,
    pub resolved_terms: Vec<String>,
    pub operation_version: String,
    pub tokenizer: String,
    pub coverage: Coverage,
    pub total: usize,
    pub hits: Vec<SearchHit>,
}
pub type QueryError = String;

fn valid_tag(t: &TagId) -> bool {
    !t.owner.trim().is_empty() && !t.concept.trim().is_empty()
}
fn valid_id(id: &RecallSourceId) -> bool {
    !id.source.trim().is_empty()
        && id.owner.as_ref().is_none_or(|s| !s.trim().is_empty())
        && id.node.as_ref().is_none_or(|s| !s.trim().is_empty())
}
fn validate(
    snapshot: &ResearchSnapshot,
    query: &SearchQuery,
    settings: &SearchSettings,
) -> Result<(), QueryError> {
    if snapshot.generation.trim().is_empty()
        || settings.limit == 0
        || settings.max_sources == 0
        || settings.max_corpus_chars == 0
        || settings.max_metadata_chars == 0
        || settings.max_query_chars == 0
        || settings.offset.checked_add(settings.limit).is_none()
    {
        return Err("invalid generation or bounds".into());
    }
    if [
        settings.weights.url,
        settings.weights.title,
        settings.weights.text,
    ]
    .iter()
    .any(|w| !w.is_finite() || *w <= 0.0)
    {
        return Err("weights must be finite and positive".into());
    }
    if query.kinds.len() > 4
        || query
            .kinds
            .iter()
            .enumerate()
            .any(|(i, kind)| query.kinds[..i].contains(kind))
    {
        return Err("duplicate or excessive producer kinds".into());
    }
    if query
        .date_from
        .zip(query.date_to)
        .is_some_and(|(a, b)| a > b)
        || query.tags.iter().any(|t| !valid_tag(t))
    {
        return Err("invalid filters".into());
    }
    if snapshot.sources.len() > settings.max_sources || snapshot.scope.len() > settings.max_sources
    {
        return Err("source limit exceeded".into());
    }
    let mut scope = BTreeSet::new();
    for id in &snapshot.scope {
        if !valid_id(id) || !scope.insert(id) {
            return Err("invalid or duplicate scope identity".into());
        }
    }
    if let Some(ids) = &query.sources {
        if ids.len() > settings.max_sources {
            return Err("source filter limit exceeded".into());
        }
        let mut seen = BTreeSet::new();
        for id in ids {
            if !valid_id(id) || !seen.insert(id) {
                return Err("invalid or duplicate source filter".into());
            }
        }
    }
    let mut seen = BTreeSet::new();
    let mut total = 0usize;
    for source in &snapshot.sources {
        let r = &source.retained;
        if !valid_id(&r.source)
            || r.version.trim().is_empty()
            || !seen.insert(&r.source)
            || source.tags.iter().any(|t| !valid_tag(t))
            || source.address.as_ref().is_some_and(|a| a.trim().is_empty())
        {
            return Err("invalid or duplicate source metadata".into());
        }
        total = total
            .checked_add(r.text.chars().count())
            .ok_or("corpus size overflow")?;
        if total > settings.max_corpus_chars {
            return Err("corpus limit exceeded".into());
        }
    }
    fn add_size(total: &mut usize, text: &str, limit: usize) -> Result<(), QueryError> {
        *total = total
            .checked_add(text.chars().count().max(1))
            .ok_or("operation size overflow")?;
        if *total > limit {
            return Err("operation size limit exceeded".into());
        }
        Ok(())
    }
    fn id_size(total: &mut usize, id: &RecallSourceId, limit: usize) -> Result<(), QueryError> {
        add_size(total, &id.source, limit)?;
        if let Some(owner) = &id.owner {
            add_size(total, owner, limit)?;
        }
        if let Some(node) = &id.node {
            add_size(total, node, limit)?;
        }
        Ok(())
    }
    fn tag_size(total: &mut usize, tag: &TagId, limit: usize) -> Result<(), QueryError> {
        add_size(total, &tag.owner, limit)?;
        add_size(total, &tag.concept, limit)
    }
    let mut metadata_size = 0;
    add_size(
        &mut metadata_size,
        &snapshot.generation,
        settings.max_metadata_chars,
    )?;
    for id in &snapshot.scope {
        id_size(&mut metadata_size, id, settings.max_metadata_chars)?;
    }
    if let Coverage::Incomplete { exclusions } = &snapshot.coverage {
        for exclusion in exclusions {
            add_size(&mut metadata_size, exclusion, settings.max_metadata_chars)?;
        }
    }
    for source in &snapshot.sources {
        id_size(
            &mut metadata_size,
            &source.retained.source,
            settings.max_metadata_chars,
        )?;
        add_size(
            &mut metadata_size,
            &source.retained.version,
            settings.max_metadata_chars,
        )?;
        for value in [&source.retained.title, &source.address]
            .into_iter()
            .flatten()
        {
            add_size(&mut metadata_size, value, settings.max_metadata_chars)?;
        }
        for tag in &source.tags {
            tag_size(&mut metadata_size, tag, settings.max_metadata_chars)?;
        }
    }
    let mut query_size = 0;
    match &query.mode {
        QueryMode::Browse => {}
        QueryMode::Terms(v)
        | QueryMode::Phrase(v)
        | QueryMode::Address(v)
        | QueryMode::Prefix(v) => add_size(&mut query_size, v, settings.max_query_chars)?,
    }
    if let Some(ids) = &query.sources {
        for id in ids {
            id_size(&mut query_size, id, settings.max_query_chars)?;
        }
    }
    for tag in &query.tags {
        tag_size(&mut query_size, tag, settings.max_query_chars)?;
    }
    for alias in &query.aliases {
        add_size(&mut query_size, &alias.term, settings.max_query_chars)?;
        for value in &alias.expansions {
            add_size(&mut query_size, value, settings.max_query_chars)?;
        }
    }
    let tokenizer = Tokenizer::new();
    for alias in &query.aliases {
        if tokenizer.tokens(&alias.term).len() != 1
            || alias.expansions.is_empty()
            || alias
                .expansions
                .iter()
                .any(|e| tokenizer.tokens(e).len() != 1)
        {
            return Err("invalid alias".into());
        }
    }
    match &query.mode {
        QueryMode::Browse => {}
        QueryMode::Terms(v) if !tokenizer.tokens(v).is_empty() => {}
        QueryMode::Prefix(v) if tokenizer.tokens(v).len() == 1 => {}
        QueryMode::Phrase(v) | QueryMode::Address(v) if !v.is_empty() => {}
        _ => return Err("invalid query".into()),
    }
    Ok(())
}

// Visit matching byte spans in source order without retaining a token stream.
fn visit_matches(
    text: &str,
    mode: &QueryMode,
    terms: &[String],
    mut visit: impl FnMut(usize, usize) -> bool,
) {
    match mode {
        QueryMode::Phrase(phrase) => {
            for (i, _) in text.match_indices(phrase) {
                if !visit(i, i + phrase.len()) {
                    break;
                }
            }
        }
        QueryMode::Terms(_) | QueryMode::Prefix(_) => {
            let tokenizer = Tokenizer::new();
            for word in genet_text::words(text) {
                for piece in word
                    .split(|c: char| !c.is_alphanumeric())
                    .filter(|p| !p.is_empty())
                {
                    let tokens = tokenizer.tokens(piece);
                    if tokens.iter().any(|token| {
                        terms.iter().any(|term| {
                            if matches!(mode, QueryMode::Prefix(_)) {
                                token.starts_with(term)
                            } else {
                                token == term
                            }
                        })
                    }) {
                        let start = piece.as_ptr() as usize - text.as_ptr() as usize;
                        if !visit(start, start + piece.len()) {
                            return;
                        }
                    }
                }
            }
        }
        _ => {}
    }
}
fn has_match(text: &str, mode: &QueryMode, terms: &[String]) -> bool {
    let mut found = false;
    visit_matches(text, mode, terms, |_, _| {
        found = true;
        false
    });
    found
}
fn matches_for(text: &str, mode: &QueryMode, terms: &[String]) -> Vec<PassageSelector> {
    let mut ranges = Vec::new();
    let mut byte_cursor = 0;
    let mut scalar_cursor = 0;
    visit_matches(text, mode, terms, |start, end| {
        scalar_cursor += text[byte_cursor..start].chars().count();
        let scalar_start = scalar_cursor;
        scalar_cursor += text[start..end].chars().count();
        ranges.push(PassageSelector {
            start: scalar_start,
            end: scalar_cursor,
        });
        byte_cursor = end;
        true
    });
    ranges
}
fn metadata_match(source: &ResearchSource, mode: &QueryMode, terms: &[String]) -> bool {
    [&source.retained.title, &source.address]
        .into_iter()
        .flatten()
        .any(|s| has_match(s, mode, terms))
}

/// Evaluate only the explicitly scoped sources. A complete result is limited to
/// the supplied frozen inputs; callers must independently recheck authorization.
pub fn search(
    snapshot: &ResearchSnapshot,
    query: &SearchQuery,
    settings: &SearchSettings,
) -> Result<SearchResult, QueryError> {
    validate(snapshot, query, settings)?;
    let tokenizer = Tokenizer::new();
    let mut terms = match &query.mode {
        QueryMode::Terms(v) | QueryMode::Prefix(v) => tokenizer.tokens(v),
        _ => vec![],
    };
    if matches!(query.mode, QueryMode::Terms(_)) {
        let initial = terms.clone();
        for alias in &query.aliases {
            let key = tokenizer.tokens(&alias.term);
            if initial.contains(&key[0]) {
                for expansion in &alias.expansions {
                    let value = tokenizer.tokens(expansion).remove(0);
                    if !terms.contains(&value) {
                        terms.push(value);
                    }
                }
            }
        }
    }
    let scoped: BTreeSet<_> = snapshot.scope.iter().collect();
    let supplied: BTreeSet<_> = snapshot
        .sources
        .iter()
        .map(|s| &s.retained.source)
        .collect();
    let mut coverage = snapshot.coverage.clone();
    for missing in scoped.difference(&supplied) {
        if matches!(coverage, Coverage::Complete) {
            coverage = Coverage::Incomplete { exclusions: vec![] };
        }
        if let Coverage::Incomplete { exclusions } = &mut coverage {
            exclusions.push(format!("missing scoped source: {:?}", missing));
        }
    }
    let mut inputs: Vec<_> = snapshot
        .sources
        .iter()
        .filter(|s| scoped.contains(&s.retained.source))
        .map(|s| InputIdentity {
            source: s.retained.source.clone(),
            version: s.retained.version.clone(),
            text_hash: eidetic::Hash::of(s.retained.text.as_bytes()),
            producer: s.producer.clone(),
            title: s.retained.title.clone(),
            address: s.address.clone(),
            date: s.date,
            tags: s.tags.clone(),
        })
        .collect();
    inputs.sort_by(|a, b| a.source.cmp(&b.source));
    let selected: Vec<_> = snapshot
        .sources
        .iter()
        .filter(|s| {
            scoped.contains(&s.retained.source)
                && query
                    .sources
                    .as_ref()
                    .is_none_or(|v| v.contains(&s.retained.source))
                && (query.kinds.is_empty() || query.kinds.contains(&s.producer))
                && query
                    .date_from
                    .is_none_or(|from| s.date.is_some_and(|d| d >= from))
                && query
                    .date_to
                    .is_none_or(|to| s.date.is_some_and(|d| d <= to))
                && query.tags.iter().all(|t| s.tags.contains(t))
        })
        .collect();
    let mut scores = HashMap::new();
    if matches!(query.mode, QueryMode::Terms(_)) {
        let docs = selected.iter().enumerate().map(|(i, s)| SearchDocument {
            key: i,
            primary_address: s.address.clone().unwrap_or_default(),
            aliases: vec![],
            title: s.retained.title.clone(),
            body: Some(s.retained.text.clone()),
        });
        let index = DocumentIndex::from_documents(
            DocumentIndexConfig {
                weights: settings.weights,
                ..Default::default()
            },
            docs,
        );
        for hit in index.search(&terms.join(" "), selected.len()) {
            if !hit.score.is_finite() {
                return Err("nonfinite search score".into());
            }
            scores.insert(hit.key, hit.score);
        }
    }
    let mut ranked = Vec::new();
    for (i, s) in selected.iter().enumerate() {
        let matched = match &query.mode {
            QueryMode::Browse => true,
            QueryMode::Terms(_) => scores.contains_key(&i),
            QueryMode::Address(v) => s.address.as_ref() == Some(v),
            QueryMode::Phrase(_) | QueryMode::Prefix(_) => {
                has_match(&s.retained.text, &query.mode, &terms)
                    || metadata_match(s, &query.mode, &terms)
            }
        };
        if matched {
            ranked.push((i, scores.get(&i).copied().unwrap_or(1.0)));
        }
    }
    ranked.sort_by(|(ai, a), (bi, b)| {
        b.total_cmp(a)
            .then_with(|| {
                selected[*bi]
                    .retained
                    .text
                    .cmp(&selected[*ai].retained.text)
            })
            .then_with(|| {
                selected[*ai]
                    .retained
                    .source
                    .cmp(&selected[*bi].retained.source)
            })
            .then_with(|| {
                selected[*ai]
                    .retained
                    .version
                    .cmp(&selected[*bi].retained.version)
            })
    });
    let total = ranked.len();
    let hits = ranked
        .into_iter()
        .skip(settings.offset)
        .take(settings.limit)
        .map(|(i, score)| {
            let s = selected[i];
            let body_matches = matches_for(&s.retained.text, &query.mode, &terms);
            let passage = body_matches.first().map(|range| {
                let start = range.start.saturating_sub(settings.snippet_context);
                let len = s.retained.text.chars().count();
                let end = range.end.saturating_add(settings.snippet_context).min(len);
                InferencePassage {
                    source: s.retained.source.clone(),
                    version: s.retained.version.clone(),
                    text_hash: eidetic::Hash::of(s.retained.text.as_bytes()),
                    selector: PassageSelector { start, end },
                    chunker: format!("eidetic.search-context/v1/{}", settings.snippet_context),
                    text: s
                        .retained
                        .text
                        .chars()
                        .skip(start)
                        .take(end - start)
                        .collect(),
                }
            });
            SearchHit {
                source: s.retained.source.clone(),
                version: s.retained.version.clone(),
                producer: s.producer.clone(),
                title: s.retained.title.clone(),
                address: s.address.clone(),
                date: s.date,
                tags: s.tags.clone(),
                score,
                matches: body_matches,
                passage,
            }
        })
        .collect();
    Ok(SearchResult {
        generation: snapshot.generation.clone(),
        scope: snapshot.scope.clone(),
        inputs,
        query: query.clone(),
        settings: settings.clone(),
        resolved_terms: terms,
        operation_version: "eidetic.research-search/v1".into(),
        tokenizer: format!("{TOKENIZER_NAME}/{SEGMENTATION_PROFILE}"),
        coverage,
        total,
        hits,
    })
}
