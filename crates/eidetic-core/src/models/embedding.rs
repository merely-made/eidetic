// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Compatibility evidence for a rebuildable embedding projection. Numeric
//! shape alone is not an embedding space. Hosts supply exact source revisions;
//! providers describe the algorithm and bytes they actually loaded.
use super::recall::RecallSourceId;
use crate::{Hash, ManifestId, SchemaRef, TypedPayload};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EmbeddingMetric {
    Cosine,
    Euclidean,
    DotProduct,
}

/// Versioned algorithm/transform plus all settings and artifact digests that
/// affect its output. A deterministic algorithm may have no external artifacts.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmbeddingComponentV1 {
    pub name: String,
    pub revision: String,
    pub parameters: BTreeMap<String, String>,
    pub artifacts: BTreeMap<String, Hash>,
}
impl EmbeddingComponentV1 {
    pub fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty()
            || self.revision.trim().is_empty()
            || self
                .parameters
                .keys()
                .chain(self.artifacts.keys())
                .any(|k| k.trim().is_empty())
        {
            return Err("embedding component requires an identity and revision".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmbeddingSpaceV1 {
    pub producer: EmbeddingComponentV1,
    pub tokenizer: EmbeddingComponentV1,
    pub preprocessing: EmbeddingComponentV1,
    pub pooling: String,
    pub query_prefix: String,
    pub document_prefix: String,
    pub normalization: String,
    pub dimensions: usize,
    pub metric: EmbeddingMetric,
}
impl EmbeddingSpaceV1 {
    pub fn validate(&self) -> Result<(), String> {
        self.producer.validate()?;
        self.tokenizer.validate()?;
        self.preprocessing.validate()?;
        if self.dimensions == 0
            || self.pooling.trim().is_empty()
            || self.normalization.trim().is_empty()
        {
            return Err("embedding space requires dimensions, pooling and normalization".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmbeddingSourceRevisionV1 {
    pub source: RecallSourceId,
    pub version: String,
    pub text_hash: Hash,
}

/// Sources are sorted by owner identity, unique, and bind the complete retained
/// text before chunking. Empty corpora are legitimate empty projections.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmbeddingCorpusV1 {
    pub chunker: String,
    pub sources: Vec<EmbeddingSourceRevisionV1>,
}
impl EmbeddingCorpusV1 {
    pub fn validate(&self) -> Result<(), String> {
        if self.chunker.trim().is_empty()
            || self
                .sources
                .iter()
                .any(|s| s.source.source.trim().is_empty() || s.version.trim().is_empty())
            || self
                .sources
                .windows(2)
                .any(|pair| pair[0].source >= pair[1].source)
        {
            return Err(
                "embedding corpus requires a chunker and unique sorted exact source revisions"
                    .into(),
            );
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmbeddingEnvelopeV1 {
    pub version: u32,
    pub space: EmbeddingSpaceV1,
    pub corpus: EmbeddingCorpusV1,
}
impl EmbeddingEnvelopeV1 {
    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1 {
            return Err("unsupported embedding envelope version".into());
        }
        self.space.validate()?;
        self.corpus.validate()
    }
    pub fn ensure_compatible(&self, expected: &Self) -> Result<(), String> {
        self.validate()?;
        expected.validate()?;
        if self.space != expected.space {
            return Err("foreign embedding space; rebuild the derived index".into());
        }
        if self.corpus != expected.corpus {
            return Err("embedding corpus or chunker changed; rebuild the derived index".into());
        }
        Ok(())
    }
    pub fn id(&self) -> ManifestId {
        ManifestId::of_blob(&serde_json::to_vec(self).expect("embedding envelope serialization"))
    }
}
impl TypedPayload for EmbeddingEnvelopeV1 {
    fn schema_ref() -> SchemaRef {
        SchemaRef::from_id(ManifestId::of_blob(br#"{"format":"mere-native","schema_id":"eidetic.EmbeddingEnvelope/v1","body":{"version":1,"description":"Exact embedding algorithm/artifacts, tokenizer, preprocessing, pooling, prefixes, normalization, shape, metric, chunker and source revisions."}}"#))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> EmbeddingEnvelopeV1 {
        let component = EmbeddingComponentV1 {
            name: "algorithm".into(),
            revision: "v1".into(),
            parameters: BTreeMap::new(),
            artifacts: BTreeMap::new(),
        };
        EmbeddingEnvelopeV1 {
            version: 1,
            space: EmbeddingSpaceV1 {
                producer: component.clone(),
                tokenizer: component.clone(),
                preprocessing: component,
                pooling: "mean".into(),
                query_prefix: "".into(),
                document_prefix: "".into(),
                normalization: "l2".into(),
                dimensions: 384,
                metric: EmbeddingMetric::Cosine,
            },
            corpus: EmbeddingCorpusV1 {
                chunker: "unicode/v1/512".into(),
                sources: vec![EmbeddingSourceRevisionV1 {
                    source: RecallSourceId {
                        kind: super::super::recall::RecallSourceKind::PageText,
                        owner: None,
                        source: "https://retained.test/a".into(),
                        node: None,
                    },
                    version: "v1".into(),
                    text_hash: Hash::of(b"retained text"),
                }],
            },
        }
    }
    #[test]
    fn every_output_or_corpus_setting_is_part_of_compatibility() {
        let original = fixture();
        original.ensure_compatible(&original).unwrap();
        let changes: Vec<Box<dyn Fn(&mut EmbeddingEnvelopeV1)>> = vec![
            Box::new(|e| {
                e.space
                    .producer
                    .artifacts
                    .insert("weights".into(), Hash::of(b"foreign weights"));
            }),
            Box::new(|e| e.space.producer.revision.push('2')),
            Box::new(|e| e.space.tokenizer.revision.push('2')),
            Box::new(|e| {
                e.space
                    .preprocessing
                    .parameters
                    .insert("truncate".into(), "128".into());
            }),
            Box::new(|e| e.space.pooling = "cls".into()),
            Box::new(|e| e.space.query_prefix = "query: ".into()),
            Box::new(|e| e.space.document_prefix = "passage: ".into()),
            Box::new(|e| e.space.normalization = "none".into()),
            Box::new(|e| e.space.metric = EmbeddingMetric::DotProduct),
            Box::new(|e| e.corpus.chunker.push('2')),
            Box::new(|e| e.corpus.sources[0].version.push('2')),
            Box::new(|e| e.corpus.sources[0].text_hash = Hash::of(b"changed")),
            Box::new(|e| e.corpus.sources[0].source.owner = Some("another owner".into())),
        ];
        for change in changes {
            let mut foreign = original.clone();
            change(&mut foreign);
            assert_eq!(foreign.space.dimensions, original.space.dimensions);
            assert!(foreign.ensure_compatible(&original).is_err());
            assert_ne!(foreign.id(), original.id());
        }
        let decoded: EmbeddingEnvelopeV1 =
            serde_json::from_slice(&serde_json::to_vec(&original).unwrap()).unwrap();
        assert_eq!(decoded, original);
        assert_eq!(decoded.id(), original.id());
    }
    #[test]
    fn unknown_versions_and_ambiguous_corpus_identities_are_refused() {
        let original = fixture();
        let mut invalid = original.clone();
        invalid.version = 2;
        assert!(invalid.validate().is_err());
        invalid = original.clone();
        invalid.space.dimensions = 0;
        assert!(invalid.validate().is_err());
        invalid = original.clone();
        invalid
            .corpus
            .sources
            .push(invalid.corpus.sources[0].clone());
        assert!(invalid.validate().is_err());
        invalid = original.clone();
        invalid.corpus.sources[0].version.clear();
        assert!(invalid.validate().is_err());
        let mut value = serde_json::to_value(original).unwrap();
        value["silent_extension"] = true.into();
        assert!(serde_json::from_value::<EmbeddingEnvelopeV1>(value).is_err());
    }
}
