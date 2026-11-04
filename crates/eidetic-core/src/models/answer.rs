// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Reproducible claim-to-passage evidence. These are structural and verbatim
//! checks, never an entailment judgment or an owner assertion.
use super::recall::{InferencePassage, PassageSelector};
use super::requested::{RequestedFact, RequestedFactSet};
use crate::{ManifestId, SchemaRef, TypedPayload};
use serde::{Deserialize, Serialize};

pub const SUPPORT_METHOD: &str = "eidetic.claim-support/v1";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimStatus {
    Supported,
    Insufficient,
    Conflicting,
}

/// Model-authored interpretation. An insufficient claim does not prove absence
/// from the corpus; a conflicting claim does not establish a true contradiction.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnswerDocument {
    pub claims: Vec<AnswerClaim>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnswerClaim {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fact: Option<RequestedFact>,
    /// Model-authored relevance and agreement decisions, never an entailment proof.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review: Option<CandidateReview>,
    pub status: ClaimStatus,
    pub text: String,
    pub evidence: Vec<ClaimEvidence>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClaimEvidence {
    pub handle: u32,
    pub quote: String,
}

/// Every supplied candidate is considered in its retained order. This makes
/// the model's relevance decisions inspectable, including rejected passages.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CandidateReview {
    pub candidates: Vec<CandidateDecision>,
    pub agreement: CandidateAgreement,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CandidateDecision {
    pub handle: u32,
    /// Repeat the exact requested property beside its candidate, without a
    /// second interpretation or a new requested-fact identity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requested: Option<String>,
    /// Exact candidate text placed before the decision in quoted-review tasks.
    /// Absent on historical handle-only reviews.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quote: Option<String>,
    pub answers: bool,
}
impl Drop for CandidateDecision {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.requested.zeroize();
        self.quote.zeroize();
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CandidateAgreement {
    None,
    Single,
    Same,
    Different,
}
impl CandidateReview {
    /// Checks completeness and the decision-to-claim relationship only. It
    /// cannot establish that an `answers` or agreement judgment is correct.
    pub fn validate(
        &self,
        status: ClaimStatus,
        links: &[ClaimEvidence],
        passages: &[(u32, &InferencePassage)],
    ) -> Result<(), String> {
        use CandidateAgreement::*;
        if passages.len() > 8
            || self.candidates.len() != passages.len()
            || self
                .candidates
                .iter()
                .zip(passages)
                .any(|(d, (h, _))| d.handle != *h)
            || self
                .candidates
                .iter()
                .zip(passages)
                .any(|(d, (_, p))| d.quote.as_ref().is_some_and(|q| q != &p.text))
            || passages
                .iter()
                .enumerate()
                .any(|(i, (h, _))| passages[..i].iter().any(|(other, _)| h == other))
        {
            return Err(
                "Candidate review must cover every unique supplied handle in order.".into(),
            );
        }
        let relevant: Vec<_> = self
            .candidates
            .iter()
            .filter(|d| d.answers)
            .map(|d| d.handle)
            .collect();
        let shape = match self.agreement {
            None => relevant.is_empty() && status == ClaimStatus::Insufficient && links.is_empty(),
            Single => relevant.len() == 1 && status == ClaimStatus::Supported && links.len() == 1,
            Same => relevant.len() >= 2 && status == ClaimStatus::Supported && links.len() == 1,
            Different => {
                relevant.len() >= 2
                    && status == ClaimStatus::Conflicting
                    && links.len() == 2
                    && links[0].handle != links[1].handle
            }
        };
        if !shape
            || links.iter().any(|link| {
                !relevant.contains(&link.handle)
                    || !passages
                        .iter()
                        .any(|(handle, p)| *handle == link.handle && p.text == link.quote)
            })
        {
            return Err(
                "Claim status and exact quotations must follow the candidate review.".into(),
            );
        }
        if self.agreement == Different {
            let source = |h| {
                &passages
                    .iter()
                    .find(|(handle, _)| *handle == h)
                    .unwrap()
                    .1
                    .source
            };
            if source(links[0].handle) == source(links[1].handle) {
                return Err("Conflicting review requires two distinct sources.".into());
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PassageSupport {
    pub handle: u32,
    pub quote: String,
    pub passage: Option<ManifestId>,
    /// Absolute Unicode scalar range in the source's canonical text stream.
    pub selector: Option<PassageSelector>,
    pub error: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssessedClaim {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fact: Option<RequestedFact>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review: Option<CandidateReview>,
    pub status: ClaimStatus,
    pub text: String,
    pub evidence: Vec<PassageSupport>,
    pub issues: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SemanticReview {
    Unreviewed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnswerSupport {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requested_facts: Option<ManifestId>,
    pub method: String,
    pub raw_output: ManifestId,
    pub semantic_review: SemanticReview,
    pub claims: Vec<AssessedClaim>,
    pub error: Option<String>,
}
impl AnswerSupport {
    /// Strict whole-document parse; no extraction of a plausible JSON fragment
    /// from otherwise unassessed output. Code fences must enclose the whole body.
    pub fn assess(raw: &str, passages: &[(u32, &InferencePassage)]) -> Self {
        Self::assess_requested(raw, passages, None)
    }
    /// Validate exact count and ordered fact identities before admitting claims.
    /// The caller validates the fact set against the retained question first.
    pub fn assess_requested(
        raw: &str,
        passages: &[(u32, &InferencePassage)],
        facts: Option<&RequestedFactSet>,
    ) -> Self {
        let mut result = Self {
            requested_facts: facts.map(|s| s.id()),
            method: SUPPORT_METHOD.into(),
            raw_output: ManifestId::of_blob(raw.as_bytes()),
            semantic_review: SemanticReview::Unreviewed,
            claims: vec![],
            error: None,
        };
        if facts.is_some_and(|s| s.validate_identity().is_err()) {
            result.error = Some("Invalid requested-fact set.".into());
            return result;
        }
        let body = raw.trim();
        let body = body
            .strip_prefix("```json")
            .or_else(|| body.strip_prefix("```"))
            .and_then(|body| body.trim().strip_suffix("```"))
            .unwrap_or(body)
            .trim();
        let document: AnswerDocument = match serde_json::from_str(body) {
            Ok(document) => document,
            Err(_) => {
                result.error = Some("The model did not return a complete claim-support document. Raw output is retained.".into());
                return result;
            }
        };
        if document.claims.is_empty() || document.claims.len() > 16 {
            result.error = Some("An answer must contain between one and sixteen claims.".into());
            return result;
        }
        if facts.is_some_and(|s| {
            s.facts.len() != document.claims.len()
                || document
                    .claims
                    .iter()
                    .zip(&s.facts)
                    .any(|(c, f)| c.fact.as_ref() != Some(f))
        }) || (facts.is_none() && document.claims.iter().any(|c| c.fact.is_some()))
        {
            result.error=Some("Answer claim count or ordered requested-fact identities do not match the retained request.".into());
            return result;
        }
        for claim in &document.claims {
            if let Some(review) = &claim.review {
                if review.candidates.iter().any(|d| {
                    d.requested
                        .as_ref()
                        .is_some_and(|text| claim.fact.as_ref().map(|f| &f.text) != Some(text))
                }) {
                    result.error = Some(
                        "Candidate review requested property differs from its claim fact.".into(),
                    );
                    result.claims.clear();
                    return result;
                }
                if let Err(error) = review.validate(claim.status, &claim.evidence, passages) {
                    result.error = Some(error);
                    result.claims.clear();
                    return result;
                }
            }
            let mut assessed = AssessedClaim {
                fact: claim.fact.clone(),
                review: claim.review.clone(),
                status: claim.status,
                text: claim.text.clone(),
                evidence: vec![],
                issues: vec![],
            };
            if claim.text.trim().is_empty()
                || claim.text.chars().count() > 4096
                || claim.evidence.len() > 8
            {
                result.error = Some("Claim text or evidence exceeds answer bounds.".into());
                result.claims.clear();
                return result;
            }
            let mut sources = std::collections::BTreeSet::new();
            for link in &claim.evidence {
                let mut support = PassageSupport {
                    handle: link.handle,
                    quote: link.quote.clone(),
                    passage: None,
                    selector: None,
                    error: None,
                };
                if let Some((_, passage)) =
                    passages.iter().find(|(handle, _)| *handle == link.handle)
                {
                    support.passage = Some(passage.id());
                    if passage.selector.end.checked_sub(passage.selector.start)
                        != Some(passage.text.chars().count())
                    {
                        support.error =
                            Some("Supplied passage source range is inconsistent.".into());
                    } else if link.quote.trim().chars().count() < 16 {
                        support.error =
                            Some("Evidence quotation is shorter than sixteen characters.".into());
                    } else if let Some(offset) = passage.text.find(&link.quote) {
                        let start = passage.selector.start + passage.text[..offset].chars().count();
                        support.selector = Some(PassageSelector {
                            start,
                            end: start + link.quote.chars().count(),
                        });
                        sources.insert(passage.source.clone());
                    } else {
                        support.error = Some(
                            "Quotation does not occur verbatim in the supplied passage.".into(),
                        );
                    }
                } else {
                    support.error =
                        Some("Evidence handle does not name a supplied passage.".into());
                }
                if let Some(error) = &support.error {
                    assessed.issues.push(error.clone());
                }
                assessed.evidence.push(support);
            }
            match claim.status {
                ClaimStatus::Supported if sources.is_empty() => assessed.issues.push("Supported claim has no checked quotation.".into()),
                ClaimStatus::Conflicting if sources.len() < 2 => assessed.issues.push("Conflicting claim needs checked quotations from at least two distinct sources.".into()),
                ClaimStatus::Insufficient if !claim.evidence.is_empty() => assessed.issues.push("An insufficient-evidence statement must not assert supporting passages.".into()),
                _ => {}
            }
            if result
                .claims
                .iter()
                .any(|previous| previous.text.trim() == assessed.text.trim())
            {
                assessed
                    .issues
                    .push("The model repeated an earlier claim.".into());
            }
            result.claims.push(assessed);
        }
        result
    }
    pub fn id(&self) -> ManifestId {
        let bytes =
            zeroize::Zeroizing::new(serde_json::to_vec(self).expect("support serialization"));
        ManifestId::of_blob(&bytes)
    }
}
impl TypedPayload for AnswerSupport {
    fn schema_ref() -> SchemaRef {
        SchemaRef::from_id(ManifestId::of_blob(br#"{"format":"mere-native","schema_id":"eidetic.AnswerSupport/v1","body":{"version":1,"description":"Model-declared claims and reproducible exact quotation links. Semantic support remains unreviewed."}}"#))
    }
}
impl Drop for AnswerClaim {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.text.zeroize();
    }
}
impl Drop for ClaimEvidence {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.quote.zeroize();
    }
}
impl Drop for AssessedClaim {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.text.zeroize();
    }
}
impl Drop for PassageSupport {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.quote.zeroize();
    }
}

#[cfg(test)]
mod tests;
