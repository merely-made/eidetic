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
