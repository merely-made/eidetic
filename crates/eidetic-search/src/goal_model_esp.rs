// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! ESP adapter for the goal model path: any ESP [`EmbeddingProvider`] that
//! declares a `compatibility_space()` (a loaded BERT provider does; test and
//! unloaded providers do not and are refused). Role prefixes come from that
//! space and are applied here, because ESP's raw embed methods apply none.

use crate::goal_model::{EmbedRole, GoalEmbedder};
use eidetic::models::embedding::{EmbeddingCorpusV1, EmbeddingEnvelopeV1, EmbeddingSpaceV1};
use esp::embed::provider::EmbeddingProvider;

/// A [`GoalEmbedder`] over an ESP provider.
pub struct EspGoalEmbedder<P: EmbeddingProvider> {
    provider: P,
    space: EmbeddingSpaceV1,
}

impl<P: EmbeddingProvider> EspGoalEmbedder<P> {
    /// Refuses a provider with no compatibility space, an invalid one, or one
    /// whose declared dimensions differ from the provider's.
    ///
    /// # Errors
    /// The refusal reason.
    pub fn new(provider: P) -> Result<Self, String> {
        let space = provider
            .compatibility_space()
            .ok_or("provider declares no compatibility space")?;
        space.validate()?;
        if space.dimensions != provider.dimensions() {
            return Err("compatibility space and provider dimensions differ".into());
        }
        Ok(Self { provider, space })
    }
}

impl<P: EmbeddingProvider> GoalEmbedder for EspGoalEmbedder<P> {
    fn envelope(&self) -> EmbeddingEnvelopeV1 {
        EmbeddingEnvelopeV1 {
            version: 1,
            space: self.space.clone(),
            // Replaced per run by the assessor.
            corpus: EmbeddingCorpusV1 {
                chunker: "unset".into(),
                sources: Vec::new(),
            },
        }
    }

    fn embed(&mut self, texts: &[&str], role: EmbedRole) -> Result<Vec<Vec<f32>>, String> {
        let prefix = match role {
            EmbedRole::Query => &self.space.query_prefix,
            EmbedRole::Document => &self.space.document_prefix,
        };
        if prefix.is_empty() {
            return self.provider.embed(texts).map_err(|e| e.to_string());
        }
        let prefixed: Vec<String> = texts.iter().map(|t| format!("{prefix}{t}")).collect();
        let refs: Vec<&str> = prefixed.iter().map(String::as_str).collect();
        self.provider.embed(&refs).map_err(|e| e.to_string())
    }
}

/// `match_similarity_milli` calibrated for all-MiniLM-L6-v2 (ESP `bert-f32`)
/// under [`ModelAssessRule::Or`](crate::ModelAssessRule::Or): the middle of the
/// thresholds that maximise F1 on the frozen corpus
/// `tests/fixtures/goal_calibration/corpus.txt`, measured by
/// `tests/goal_model_minilm.rs`. It sits 84 below the highest unrelated
/// source and 79 below the lowest paraphrase, so it passes paraphrases and
/// drops unrelated text; near-miss text that shares goal words is not
/// separated by any threshold (S1 already passes it).
pub const MINILM_L6_V2_MATCH_SIMILARITY_MILLI: i16 = 220;

/// Blake3 of the pinned all-MiniLM-L6-v2 `model.safetensors`, as ESP records
/// it in the space's producer artifacts.
const MINILM_L6_V2_WEIGHTS_BLAKE3: [u8; 32] = [
    128, 135, 233, 191, 151, 194, 101, 248, 67, 94, 210, 104, 115, 62, 207, 55, 145, 130, 90, 210,
    72, 80, 253, 93, 132, 216, 158, 50, 238, 58, 88, 154,
];

/// The calibrated `match_similarity_milli` for `space`, or `None` for a space
/// nobody has calibrated (use the generic default only as a placeholder there:
/// cosine scales differ per model). Keyed on the checkpoint's weights, not just
/// the producer name, because every ESP BERT checkpoint shares the name.
pub fn recommended_match_similarity_milli(space: &EmbeddingSpaceV1) -> Option<i16> {
    let weights = space.producer.artifacts.get("model.safetensors")?;
    (space.producer.name == "esp.bert-f32" && weights.as_bytes() == &MINILM_L6_V2_WEIGHTS_BLAKE3)
        .then_some(MINILM_L6_V2_MATCH_SIMILARITY_MILLI)
}
