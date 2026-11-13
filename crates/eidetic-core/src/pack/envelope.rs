// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The `mere.pack/2` signing envelope (declared-lens D6) and its verdict.
//!
//! A signature is a detached COSE_Sign1 (RFC 9052 §4.2) over the pack's
//! canonical bytes, `alg` -19 (Ed25519, RFC 9864), `content type`
//! `mere.pack/2` and `kid` the signer's `did:key`, all three protected. It
//! rides in the codicil's [`TrustEnvelope::signatures`] as
//! `cose-sign1:<base64url of the tagged COSE_Sign1>`. `coset` builds and reads
//! the structure and the Sig_structure; personae's Ed25519 (the family's
//! `ed25519-dalek` 3) signs and verifies. No cryptography is ours.
//!
//! [`verify_envelope`] gives the verdict ladder `pack.rs` already has:
//! `Trusted` when every claimed `cose-sign1:` signature verifies, `Unsigned`
//! when none is claimed, `Broken` when one is claimed and fails.
//! [`TrustPolicy`] then decides acceptance (D3): the release key is the one
//! built-in signer, and any other is accepted per pack id by the host's
//! install review, behind [`SignerReview`].

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use coset::{
    Algorithm, ContentType, CoseSign1, CoseSign1Builder, HeaderBuilder, TaggedCborSerializable,
    iana,
};
use identity::{Ed25519Keypair, Ed25519PublicKey, Ed25519Signature};
use insigne::TypedKey;

use super::PackVerdict;
use super::header::PACK2_FORMAT;
use crate::schema::{SignatureRef, TrustEnvelope};

/// The binding prefix a COSE_Sign1 rides under in [`TrustEnvelope::signatures`].
pub const COSE_SIGN1_PREFIX: &str = "cose-sign1:";

/// The one algorithm a pack signature may name: Ed25519, -19 (RFC 9864).
pub const PACK_ALG: iana::Algorithm = iana::Algorithm::Ed25519;

/// Which verification step refused a signature (RFC 9052 §3 and §4.4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Broken {
    /// The binding is not base64url.
    Encoding,
    /// Not a well-formed COSE_Sign1_Tagged (wrong tag, bad CBOR, a label twice
    /// in one header map).
    Structure,
    /// Well-formed, but re-encoding gives other bytes.
    NotDeterministic,
    /// The payload is attached; a pack signature is detached.
    AttachedPayload,
    /// A `crit` header is present; this profile understands none.
    Critical,
    /// One label is in both the protected and the unprotected header.
    DuplicateLabel,
    /// `alg` is absent from the protected header.
    AlgNotProtected,
    /// `alg` is not -19.
    WrongAlg,
    /// The protected `content type` is not `mere.pack/2`.
    ContentType,
    /// The protected `kid` is absent or not an Ed25519 `did:key`.
    Kid,
    /// The signature is not 64 bytes.
    SignatureLength,
    /// The signature does not verify over the Sig_structure.
    Signature,
}

/// The verdict on a pack's signatures.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// Every claimed signature verifies; the signers, in envelope order.
    Trusted(Vec<Ed25519PublicKey>),
    /// No `cose-sign1:` signature is claimed.
    Unsigned,
    /// A claimed signature fails, at this step.
    Broken(Broken),
}

impl Verdict {
    /// The rung on `pack.rs`'s ladder.
    pub fn rung(&self) -> PackVerdict {
        match self {
            Self::Trusted(_) => PackVerdict::Trusted,
            Self::Unsigned => PackVerdict::Unsigned,
            Self::Broken(_) => PackVerdict::Broken,
        }
    }
}

/// Sign `canonical` (a pack's canonical bytes) with `keypair`, which comes
/// from custody; this module never mints a key.
pub fn sign_canonical(canonical: &[u8], keypair: &Ed25519Keypair) -> SignatureRef {
    let kid = TypedKey::ed25519(keypair.public_key().to_bytes()).to_text();
    let protected = HeaderBuilder::new()
        .algorithm(PACK_ALG)
        .content_type(PACK2_FORMAT.to_string())
        .key_id(kid.into_bytes())
        .build();
    let sign1 = CoseSign1Builder::new()
        .protected(protected)
        .create_detached_signature(canonical, b"", |tbs| keypair.sign(tbs).to_bytes().to_vec())
        .build();
    // Encoding a structure coset just built cannot fail: every field is set
    // from the types above.
    let bytes = sign1.to_tagged_vec().unwrap_or_default();
    SignatureRef(format!(
        "{COSE_SIGN1_PREFIX}{}",
        URL_SAFE_NO_PAD.encode(bytes)
    ))
}

/// The verdict on the `cose-sign1:` signatures in `envelope` over `canonical`.
/// Signatures under other prefixes are not this module's to judge.
pub fn verify_envelope(canonical: &[u8], envelope: &TrustEnvelope) -> Verdict {
    let mut signers = Vec::new();
    for sig in &envelope.signatures {
        let Some(encoded) = sig.0.strip_prefix(COSE_SIGN1_PREFIX) else {
            continue;
        };
        match verify_binding(encoded, canonical) {
            Ok(signer) => signers.push(signer),
            Err(step) => return Verdict::Broken(step),
        }
    }
    if signers.is_empty() {
        Verdict::Unsigned
    } else {
        Verdict::Trusted(signers)
    }
}

fn verify_binding(encoded: &str, canonical: &[u8]) -> Result<Ed25519PublicKey, Broken> {
    let bytes = URL_SAFE_NO_PAD
        .decode(encoded)
        .map_err(|_| Broken::Encoding)?;
    let sign1 = CoseSign1::from_tagged_slice(&bytes).map_err(|_| Broken::Structure)?;
    if sign1.clone().to_tagged_vec().ok().as_deref() != Some(bytes.as_slice()) {
        return Err(Broken::NotDeterministic);
    }
    if sign1.payload.is_some() {
        return Err(Broken::AttachedPayload);
    }
    let signer = check_headers(&sign1)?;
    check_signature(&sign1, canonical, &signer)?;
    Ok(signer)
}

/// RFC 9052 §3 and §3.1 for this profile; returns the `kid`'s key.
fn check_headers(sign1: &CoseSign1) -> Result<Ed25519PublicKey, Broken> {
    let protected = &sign1.protected.header;
    let unprotected = &sign1.unprotected;
    if !protected.crit.is_empty() || !unprotected.crit.is_empty() {
        return Err(Broken::Critical);
    }
    let both = (protected.alg.is_some() && unprotected.alg.is_some())
        || (protected.content_type.is_some() && unprotected.content_type.is_some())
        || (!protected.key_id.is_empty() && !unprotected.key_id.is_empty())
        || (!protected.iv.is_empty() && !unprotected.iv.is_empty())
        || (!protected.partial_iv.is_empty() && !unprotected.partial_iv.is_empty())
        || protected
            .rest
            .iter()
            .any(|(label, _)| unprotected.rest.iter().any(|(other, _)| other == label));
    if both {
        return Err(Broken::DuplicateLabel);
    }
    match &protected.alg {
        None => return Err(Broken::AlgNotProtected),
        Some(alg) if *alg != Algorithm::Assigned(PACK_ALG) => return Err(Broken::WrongAlg),
        Some(_) => {}
    }
    if protected.content_type != Some(ContentType::Text(PACK2_FORMAT.to_string())) {
        return Err(Broken::ContentType);
    }
    ed25519_kid(&protected.key_id).ok_or(Broken::Kid)
}

fn ed25519_kid(kid: &[u8]) -> Option<Ed25519PublicKey> {
    let text = std::str::from_utf8(kid).ok()?;
    match text.parse::<TypedKey>().ok()? {
        TypedKey::Ed25519(bytes) => Ed25519PublicKey::from_bytes(&bytes).ok(),
        _ => None,
    }
}

/// RFC 9052 §4.4's verification: the Sig_structure (built by coset over the
/// protected bytes as received, empty external AAD, and the payload, attached
/// or detached) checked against `key` by personae's Ed25519.
pub(crate) fn check_signature(
    sign1: &CoseSign1,
    detached: &[u8],
    key: &Ed25519PublicKey,
) -> Result<(), Broken> {
    let signature: [u8; 64] = sign1
        .signature
        .as_slice()
        .try_into()
        .map_err(|_| Broken::SignatureLength)?;
    let tbs = match sign1.payload {
        Some(_) => sign1.tbs_data(b""),
        None => sign1.tbs_detached_data(detached, b""),
    };
    if key.verify(&tbs, &Ed25519Signature::from_bytes(&signature)) {
        Ok(())
    } else {
        Err(Broken::Signature)
    }
}

/// Where a pack came from, as far as trust goes (D20).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    /// Authored or kept locally; may stay unsigned.
    Local,
    /// Shared, published or arriving from a peer: needs an accepted signer.
    CrossesTrust,
}

/// The host's install review, the hook D3 names: whether `signer` has been
/// accepted for the pack `pack_id`. The review itself is the host's.
pub trait SignerReview {
    fn accepts(&self, pack_id: &str, signer: &Ed25519PublicKey) -> bool;
}

/// No review: only the release key is accepted.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoReview;

impl SignerReview for NoReview {
    fn accepts(&self, _pack_id: &str, _signer: &Ed25519PublicKey) -> bool {
        false
    }
}

/// Why a pack is admitted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Admission {
    /// Signed by the release key.
    Release,
    /// Signed by a signer the install review accepted for this pack id.
    Reviewed(Ed25519PublicKey),
    /// Local: unsigned, or signed by no accepted signer.
    Local,
}

/// Why a pack is refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// A claimed signature fails, at any origin.
    Broken(Broken),
    /// Unsigned, and the pack crosses trust.
    Unsigned,
    /// Signed, but by no accepted signer, and the pack crosses trust.
    UnacceptedSigner(Vec<Ed25519PublicKey>),
}

/// Signer acceptance (D3): the release key, held in custody, whose public key
/// the engine builds in; then the host's review per pack id.
#[derive(Clone, Debug)]
pub struct TrustPolicy<R> {
    pub release_key: Ed25519PublicKey,
    pub review: R,
}

impl<R: SignerReview> TrustPolicy<R> {
    /// Decide whether the pack `pack_id` with `verdict` is admitted from `origin`.
    ///
    /// # Errors
    ///
    /// A [`Refusal`] naming the rule that refuses.
    pub fn admit(
        &self,
        pack_id: &str,
        verdict: &Verdict,
        origin: Origin,
    ) -> Result<Admission, Refusal> {
        let signers = match verdict {
            Verdict::Broken(step) => return Err(Refusal::Broken(*step)),
            Verdict::Unsigned => {
                return match origin {
                    Origin::Local => Ok(Admission::Local),
                    Origin::CrossesTrust => Err(Refusal::Unsigned),
                };
            }
            Verdict::Trusted(signers) => signers,
        };
        if signers.contains(&self.release_key) {
            return Ok(Admission::Release);
        }
        if let Some(signer) = signers.iter().find(|s| self.review.accepts(pack_id, s)) {
            return Ok(Admission::Reviewed(*signer));
        }
        match origin {
            Origin::Local => Ok(Admission::Local),
            Origin::CrossesTrust => Err(Refusal::UnacceptedSigner(signers.clone())),
        }
    }
}
