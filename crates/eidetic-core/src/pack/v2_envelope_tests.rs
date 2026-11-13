// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Declared-lens §3.4 negatives for the COSE_Sign1 envelope, taken from RFC
//! 9052's own rules (§3 headers, §3.1 `crit` and `alg`, §4.4 verification)
//! and the cose-wg `sign1-tests` failure kinds, plus D3's acceptance policy.

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use coset::{
    CborSerializable, CoseSign1, CoseSign1Builder, Header, HeaderBuilder, TaggedCborSerializable,
    iana,
};
use identity::{Ed25519Keypair, Ed25519PublicKey};
use insigne::TypedKey;

use super::v2_codec_tests::fixture;
use super::*;
use crate::schema::{ModerationState, SignatureRef, TrustEnvelope, TrustLevel};

fn key(seed: u8) -> Ed25519Keypair {
    Ed25519Keypair::from_seed([seed; 32])
}

fn did(k: &Ed25519Keypair) -> Vec<u8> {
    TypedKey::ed25519(k.public_key().to_bytes())
        .to_text()
        .into_bytes()
}

fn envelope(sigs: Vec<SignatureRef>) -> TrustEnvelope {
    TrustEnvelope {
        level: TrustLevel::SelfAsserted,
        signatures: sigs,
        moderation_state: ModerationState::Unreviewed,
    }
}

fn canonical() -> Vec<u8> {
    fixture().to_canonical().unwrap()
}

fn protected_ok(k: &Ed25519Keypair) -> HeaderBuilder {
    HeaderBuilder::new()
        .algorithm(PACK_ALG)
        .content_type(PACK2_FORMAT.to_string())
        .key_id(did(k))
}

/// A detached Sign1 over `payload` with any headers, signed by `signer`.
fn build(
    protected: Header,
    unprotected: Header,
    payload: &[u8],
    signer: &Ed25519Keypair,
) -> CoseSign1 {
    CoseSign1Builder::new()
        .protected(protected)
        .unprotected(unprotected)
        .create_detached_signature(payload, b"", |tbs| signer.sign(tbs).to_bytes().to_vec())
        .build()
}

fn bind(bytes: Vec<u8>) -> SignatureRef {
    SignatureRef(format!(
        "{COSE_SIGN1_PREFIX}{}",
        URL_SAFE_NO_PAD.encode(bytes)
    ))
}

fn verdict_of(sign1: CoseSign1, payload: &[u8]) -> Verdict {
    verify_envelope(
        payload,
        &envelope(vec![bind(sign1.to_tagged_vec().unwrap())]),
    )
}

fn broken(sign1: CoseSign1, payload: &[u8]) -> Broken {
    match verdict_of(sign1, payload) {
        Verdict::Broken(step) => step,
        other => panic!("expected Broken, got {other:?}"),
    }
}

#[test]
fn rfc8032_test_1_control() {
    // RFC 8032 §7.1 TEST 1 through personae: the instrument signs as the RFC does.
    let seed = "9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60";
    let mut s = [0u8; 32];
    for (i, b) in s.iter_mut().enumerate() {
        *b = u8::from_str_radix(&seed[2 * i..2 * i + 2], 16).unwrap();
    }
    let sig = Ed25519Keypair::from_seed(s).sign(b"").to_bytes();
    assert_eq!(
        sig[..8],
        [0xe5, 0x56, 0x43, 0x00, 0xc3, 0x60, 0xac, 0x72],
        "RFC 8032 TEST 1 signature prefix"
    );
}

#[test]
fn a_signed_pack_is_trusted_and_unsigned_is_unsigned() {
    let bytes = canonical();
    let author = key(7);
    let sig = sign_canonical(&bytes, &author);
    assert!(sig.0.starts_with(COSE_SIGN1_PREFIX));
    let verdict = verify_envelope(&bytes, &envelope(vec![sig.clone()]));
    assert_eq!(verdict, Verdict::Trusted(vec![author.public_key()]));
    assert_eq!(verdict.rung(), PackVerdict::Trusted);
    // The emitted structure is the profile: tagged 18, detached, three protected labels.
    let raw = URL_SAFE_NO_PAD
        .decode(&sig.0[COSE_SIGN1_PREFIX.len()..])
        .unwrap();
    assert_eq!(raw[0], 0xd2, "COSE_Sign1_Tagged");
    let sign1 = CoseSign1::from_tagged_slice(&raw).unwrap();
    assert!(sign1.payload.is_none() && sign1.unprotected.is_empty());
    assert_eq!(sign1.protected.header.key_id, did(&author));
    assert_eq!(
        verify_envelope(&bytes, &TrustEnvelope::self_asserted()),
        Verdict::Unsigned
    );
    let foreign = envelope(vec![SignatureRef("personae:ed25519:ab:cd".into())]);
    assert_eq!(
        verify_envelope(&bytes, &foreign),
        Verdict::Unsigned,
        "not ours to judge"
    );
}

#[test]
fn two_signers_are_both_named_and_one_bad_breaks_all() {
    let bytes = canonical();
    let (a, b) = (key(1), key(2));
    let both = envelope(vec![sign_canonical(&bytes, &a), sign_canonical(&bytes, &b)]);
    assert_eq!(
        verify_envelope(&bytes, &both),
        Verdict::Trusted(vec![a.public_key(), b.public_key()])
    );
    let mut other = fixture();
    other.version = "9.9.9".into();
    let stale = sign_canonical(&other.to_canonical().unwrap(), &b);
    let mixed = envelope(vec![sign_canonical(&bytes, &a), stale]);
    assert_eq!(
        verify_envelope(&bytes, &mixed),
        Verdict::Broken(Broken::Signature)
    );
}

type Tamper = fn(&mut PackHeader);

#[test]
fn a_tampered_or_swapped_payload_is_broken() {
    let bytes = canonical();
    let env = envelope(vec![sign_canonical(&bytes, &key(7))]);
    let tamperings: [(&str, Tamper); 12] = [
        ("format", |h| h.format = "mere.pack/2 ".into()),
        ("id", |h| h.id = "https://mere.computer/packs/other".into()),
        ("version", |h| h.version = "1.2.4".into()),
        ("kind", |h| {
            h.kind.push(PackKind::Lens);
            h.body.insert(PackKind::Lens, BodyValue::Bool(true));
        }),
        ("licence", |h| h.licence = "MIT".into()),
        ("credit", |h| h.credit.reverse()),
        ("requires", |h| {
            h.requires.pop();
        }),
        ("provides", |h| h.provides.push("urn:x:widened".into())),
        ("contributes", |h| {
            h.contributes[0].kind = ContributionKind::Servitor
        }),
        ("scopes", |h| h.scopes.push("wallet/".into())),
        ("parts", |h| {
            h.parts[0].digest = Multihash(crate::schema::Hash::of(b"evil"))
        }),
        ("body", |h| {
            h.body
                .insert(PackKind::Participant, BodyValue::Bytes(vec![9]));
        }),
    ];
    for (field, tamper) in tamperings {
        let mut h = fixture();
        tamper(&mut h);
        let Ok(changed) = h.to_canonical() else {
            // A tampering the codec itself refuses never reaches the signature.
            assert_eq!(field, "format");
            continue;
        };
        assert_ne!(changed, bytes, "{field} changed the bytes");
        assert_eq!(
            verify_envelope(&changed, &env),
            Verdict::Broken(Broken::Signature),
            "{field}"
        );
    }
    // Detached payload mismatch: other bytes entirely, and a one-bit flip.
    assert_eq!(
        verify_envelope(b"other", &env),
        Verdict::Broken(Broken::Signature)
    );
    let mut flipped = bytes.clone();
    *flipped.last_mut().unwrap() ^= 1;
    assert_eq!(
        verify_envelope(&flipped, &env),
        Verdict::Broken(Broken::Signature)
    );
}

#[test]
fn wrong_key_and_wrong_kid_are_broken() {
    let bytes = canonical();
    let (author, intruder) = (key(7), key(99));
    // kid names the author, the intruder signs.
    let forged = build(
        protected_ok(&author).build(),
        Header::default(),
        &bytes,
        &intruder,
    );
    assert_eq!(broken(forged, &bytes), Broken::Signature);
    // kid is not a did:key, as in the cose-wg examples ("11").
    let plain_kid = protected_ok(&author).key_id(b"11".to_vec()).build();
    assert_eq!(
        broken(build(plain_kid, Header::default(), &bytes, &author), &bytes),
        Broken::Kid
    );
    // kid is a did:key of another family (the did:key spec's secp256k1 example).
    let secp = b"did:key:zQ3shQo6TF2moaqMTrUZEM1jeuYRQXeHEx4evX9751y2qPqRA".to_vec();
    let other_family = protected_ok(&author).key_id(secp).build();
    assert_eq!(
        broken(
            build(other_family, Header::default(), &bytes, &author),
            &bytes
        ),
        Broken::Kid
    );
    // kid absent from the protected header, present unprotected.
    let mut no_kid = protected_ok(&author).build();
    no_kid.key_id.clear();
    let unprot = HeaderBuilder::new().key_id(did(&author)).build();
    assert_eq!(
        broken(build(no_kid, unprot, &bytes, &author), &bytes),
        Broken::Kid
    );
}

#[test]
fn wrong_alg_and_header_rules_are_broken() {
    let bytes = canonical();
    let k = key(7);
    let none = Header::default;
    let with_alg = |alg: iana::Algorithm| protected_ok(&k).algorithm(alg).build();
    // RFC 9864 deprecates the polymorphic EdDSA (-8): a valid signature under it is refused.
    let eddsa = build(with_alg(iana::Algorithm::EdDSA), none(), &bytes, &k);
    assert_eq!(broken(eddsa, &bytes), Broken::WrongAlg);
    let es256 = build(with_alg(iana::Algorithm::ES256), none(), &bytes, &k);
    assert_eq!(broken(es256, &bytes), Broken::WrongAlg);
    // alg missing, and alg only in the unprotected header.
    let mut no_alg = protected_ok(&k).build();
    no_alg.alg = None;
    assert_eq!(
        broken(build(no_alg.clone(), none(), &bytes, &k), &bytes),
        Broken::AlgNotProtected
    );
    let unprot_alg = HeaderBuilder::new().algorithm(PACK_ALG).build();
    assert_eq!(
        broken(build(no_alg, unprot_alg, &bytes, &k), &bytes),
        Broken::AlgNotProtected
    );
    // The same label protected and unprotected (§3).
    let dup = HeaderBuilder::new()
        .content_type(PACK2_FORMAT.to_string())
        .build();
    assert_eq!(
        broken(build(protected_ok(&k).build(), dup, &bytes, &k), &bytes),
        Broken::DuplicateLabel
    );
    // crit names a label this profile does not understand (§3.1).
    let crit = protected_ok(&k)
        .add_critical(iana::HeaderParameter::CounterSignature)
        .build();
    assert_eq!(
        broken(build(crit, none(), &bytes, &k), &bytes),
        Broken::Critical
    );
    // content type absent, or another one.
    let mut no_ctyp = protected_ok(&k).build();
    no_ctyp.content_type = None;
    assert_eq!(
        broken(build(no_ctyp, none(), &bytes, &k), &bytes),
        Broken::ContentType
    );
    let other = protected_ok(&k)
        .content_type("application/cbor".to_string())
        .build();
    assert_eq!(
        broken(build(other, none(), &bytes, &k), &bytes),
        Broken::ContentType
    );
}

#[test]
fn the_cose_wg_sign1_failure_kinds_on_ed25519() {
    let bytes = canonical();
    let k = key(7);
    let good = || build(protected_ok(&k).build(), Header::default(), &bytes, &k);
    // sign-fail-01, wrong CBOR tag: COSE_Sign's 98 instead of 18; and no tag at all.
    let mut retagged = good().to_tagged_vec().unwrap();
    assert_eq!(retagged[0], 0xd2);
    retagged.splice(0..1, [0xd8, 0x62]);
    let env = envelope(vec![bind(retagged)]);
    assert_eq!(
        verify_envelope(&bytes, &env),
        Verdict::Broken(Broken::Structure)
    );
    let untagged = envelope(vec![bind(good().to_vec().unwrap())]);
    assert_eq!(
        verify_envelope(&bytes, &untagged),
        Verdict::Broken(Broken::Structure)
    );
    // sign-fail-02, changed signature; and a truncated one.
    let mut flipped = good();
    flipped.signature[0] ^= 0x80;
    assert_eq!(broken(flipped, &bytes), Broken::Signature);
    let mut short = good();
    short.signature.pop();
    assert_eq!(broken(short, &bytes), Broken::SignatureLength);
    // sign-fail-03/04, algorithm changed after signing.
    let mut realg = good();
    realg.protected = coset::ProtectedHeader {
        original_data: None,
        header: protected_ok(&k).algorithm(iana::Algorithm::EdDSA).build(),
    };
    assert_eq!(broken(realg, &bytes), Broken::WrongAlg);
    // sign-fail-06, a protected attribute added after signing.
    let mut added = good();
    added.protected = coset::ProtectedHeader {
        original_data: None,
        header: protected_ok(&k)
            .value(-70_000, coset::cbor::Value::Bool(true))
            .build(),
    };
    assert_eq!(broken(added, &bytes), Broken::Signature);
    // sign-fail-07, a protected attribute removed after signing.
    let mut removed = good();
    let mut header = protected_ok(&k).build();
    header.content_type = None;
    removed.protected = coset::ProtectedHeader {
        original_data: None,
        header,
    };
    assert_eq!(broken(removed, &bytes), Broken::ContentType);
    // An attached payload is a different message.
    let mut attached = good();
    attached.payload = Some(bytes.clone());
    assert_eq!(broken(attached, &bytes), Broken::AttachedPayload);
}

#[test]
fn encoding_faults_are_broken() {
    let bytes = canonical();
    let bad = envelope(vec![SignatureRef(format!(
        "{COSE_SIGN1_PREFIX}not base64!"
    ))]);
    assert_eq!(
        verify_envelope(&bytes, &bad),
        Verdict::Broken(Broken::Encoding)
    );
    let k = key(7);
    let good = build(protected_ok(&k).build(), Header::default(), &bytes, &k);
    let tagged = good.to_tagged_vec().unwrap();
    // The outer array made indefinite-length: decodes, but is not the bytes we write.
    assert_eq!(tagged[1], 0x84);
    let indefinite = [&[0xd2, 0x9f][..], &tagged[2..], &[0xff]].concat();
    assert_eq!(
        verify_envelope(&bytes, &envelope(vec![bind(indefinite)])),
        Verdict::Broken(Broken::NotDeterministic)
    );
    let trailing = [&tagged[..], &[0]].concat();
    assert_eq!(
        verify_envelope(&bytes, &envelope(vec![bind(trailing)])),
        Verdict::Broken(Broken::Structure)
    );
}

// ---- D3: acceptance ----------------------------------------------------------

struct Reviewed(&'static str, Ed25519PublicKey);

impl SignerReview for Reviewed {
    fn accepts(&self, pack_id: &str, signer: &Ed25519PublicKey) -> bool {
        pack_id == self.0 && *signer == self.1
    }
}

#[test]
fn acceptance_follows_d3_and_d20() {
    let bytes = canonical();
    let id = fixture().id;
    let (release, author, stranger) = (key(1), key(2), key(3));
    let verdict =
        |k: &Ed25519Keypair| verify_envelope(&bytes, &envelope(vec![sign_canonical(&bytes, k)]));
    let policy = TrustPolicy {
        release_key: release.public_key(),
        review: Reviewed("https://mere.computer/packs/core", author.public_key()),
    };
    let across = Origin::CrossesTrust;
    assert_eq!(
        policy.admit(&id, &verdict(&release), across),
        Ok(Admission::Release)
    );
    assert_eq!(
        policy.admit(&id, &verdict(&author), across),
        Ok(Admission::Reviewed(author.public_key()))
    );
    // The review is per pack id: the same author on another pack is not accepted.
    assert_eq!(
        policy.admit("https://x.example/other", &verdict(&author), across),
        Err(Refusal::UnacceptedSigner(vec![author.public_key()]))
    );
    assert_eq!(
        policy.admit(&id, &verdict(&stranger), across),
        Err(Refusal::UnacceptedSigner(vec![stranger.public_key()]))
    );
    assert_eq!(
        policy.admit(&id, &verdict(&stranger), Origin::Local),
        Ok(Admission::Local)
    );
    assert_eq!(
        policy.admit(&id, &Verdict::Unsigned, Origin::Local),
        Ok(Admission::Local)
    );
    assert_eq!(
        policy.admit(&id, &Verdict::Unsigned, across),
        Err(Refusal::Unsigned)
    );
    let broken = Verdict::Broken(Broken::Signature);
    assert_eq!(
        policy.admit(&id, &broken, Origin::Local),
        Err(Refusal::Broken(Broken::Signature))
    );
    let none = TrustPolicy {
        release_key: release.public_key(),
        review: NoReview,
    };
    assert_eq!(
        none.admit(&id, &verdict(&author), across),
        Err(Refusal::UnacceptedSigner(vec![author.public_key()]))
    );
}
