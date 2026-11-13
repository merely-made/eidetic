// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The cose-wg Examples (declared-lens D6's vector suite), vendored at
//! `Code/crates/cose-examples`, revision `53c9d634333bb4f529d78f5980fffa2667ee2c12`.
//!
//! `eddsa-sig-01` is held here as a known answer so every run checks it. The
//! suite walk reads the vendored checkout (`COSE_EXAMPLES_DIR`, or the
//! `Code/crates` sibling of this repository) and is ignored by default:
//!
//! ```text
//! COSE_EXAMPLES_DIR=<cose-examples> cargo test -p eidetic --lib v2_cose_vectors -- --ignored --nocapture
//! ```
//!
//! The examples predate RFC 9864 and name the polymorphic EdDSA (-8) with an
//! Ed25519 key. Their signatures are checked through the same Sig_structure
//! and Ed25519 step a pack signature uses; the pack profile then refuses them
//! for not being detached `-19` signatures, which is also asserted.

use std::path::{Path, PathBuf};

use base64::Engine as _;
use base64::engine::general_purpose::{URL_SAFE, URL_SAFE_NO_PAD};
use coset::{
    ContentType, CoseSign1, CoseSign1Builder, HeaderBuilder, TaggedCborSerializable, iana,
};
use identity::{Ed25519Keypair, Ed25519PublicKey};

use super::envelope::check_signature;
use super::*;
use crate::schema::{ModerationState, SignatureRef, TrustEnvelope, TrustLevel};

const X: &str = "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a";
const D: &str = "9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60";
const PLAINTEXT: &[u8] = b"This is the content.";
const TO_BE_SIGNED: &str =
    "846A5369676E61747572653145A2012703004054546869732069732074686520636F6E74656E742E";
const CBOR: &str = "D28445A201270300A10442313154546869732069732074686520636F6E74656E742E5840\
7142FD2FF96D56DB85BEE905A76BA1D0B7321A95C8C4D3607C5781932B7AFB8711497DFA751BF40B58B3BCC32300\
B1487F3DB34085EEF013BF08F4A44D6FEF0D";

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}

fn arr32(v: &[u8]) -> [u8; 32] {
    v.try_into().unwrap()
}

#[test]
fn eddsa_sig_01_known_answer() {
    let message = CoseSign1::from_tagged_slice(&unhex(CBOR)).unwrap();
    // The Sig_structure coset builds is the example's ToBeSign.
    assert_eq!(message.tbs_data(b""), unhex(TO_BE_SIGNED));
    let public = Ed25519PublicKey::from_bytes(&arr32(&unhex(X))).unwrap();
    assert_eq!(check_signature(&message, b"", &public), Ok(()));
    // Re-signing with the example's key reproduces its bytes exactly (Ed25519 is deterministic).
    let keypair = Ed25519Keypair::from_seed(arr32(&unhex(D)));
    assert_eq!(keypair.public_key(), public);
    let rebuilt = CoseSign1Builder::new()
        .protected(
            HeaderBuilder::new()
                .algorithm(iana::Algorithm::EdDSA)
                .content_format(iana::CoapContentFormat::TextPlainUtf8)
                .build(),
        )
        .unprotected(HeaderBuilder::new().key_id(b"11".to_vec()).build())
        .payload(PLAINTEXT.to_vec())
        .create_signature(b"", |tbs| keypair.sign(tbs).to_bytes().to_vec())
        .build();
    assert_eq!(rebuilt.to_tagged_vec().unwrap(), unhex(CBOR));
}

#[test]
fn eddsa_sig_01_negatives() {
    let message = CoseSign1::from_tagged_slice(&unhex(CBOR)).unwrap();
    let public = Ed25519PublicKey::from_bytes(&arr32(&unhex(X))).unwrap();
    let mut tampered = message.clone();
    tampered.payload = Some(b"This is the content!".to_vec());
    assert_eq!(
        check_signature(&tampered, b"", &public),
        Err(Broken::Signature)
    );
    let wrong = Ed25519Keypair::from_seed([5; 32]).public_key();
    assert_eq!(
        check_signature(&message, b"", &wrong),
        Err(Broken::Signature)
    );
    let mut flipped = message.clone();
    flipped.signature[63] ^= 1;
    assert_eq!(
        check_signature(&flipped, b"", &public),
        Err(Broken::Signature)
    );
    // Detached: the same signature over the payload supplied out of band.
    let mut detached = message.clone();
    detached.payload = None;
    assert_eq!(check_signature(&detached, PLAINTEXT, &public), Ok(()));
    assert_eq!(
        check_signature(&detached, b"other", &public),
        Err(Broken::Signature)
    );
    // The pack profile refuses the example: attached, -8, ctyp 0, kid "11".
    let env = TrustEnvelope {
        level: TrustLevel::SelfAsserted,
        signatures: vec![SignatureRef(format!(
            "{COSE_SIGN1_PREFIX}{}",
            URL_SAFE_NO_PAD.encode(unhex(CBOR))
        ))],
        moderation_state: ModerationState::Unreviewed,
    };
    assert_eq!(
        verify_envelope(PLAINTEXT, &env),
        Verdict::Broken(Broken::AttachedPayload)
    );
    let rest = detached.to_tagged_vec().unwrap();
    let env = TrustEnvelope {
        signatures: vec![SignatureRef(format!(
            "{COSE_SIGN1_PREFIX}{}",
            URL_SAFE_NO_PAD.encode(rest)
        ))],
        ..env
    };
    assert_eq!(
        verify_envelope(PLAINTEXT, &env),
        Verdict::Broken(Broken::WrongAlg)
    );
    assert_eq!(
        message.protected.header.content_type,
        Some(ContentType::Assigned(
            iana::CoapContentFormat::TextPlainUtf8
        ))
    );
}

fn suite_dir() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("COSE_EXAMPLES_DIR") {
        return Some(PathBuf::from(dir)).filter(|d| d.join("eddsa-examples").is_dir());
    }
    // `repos/eidetic/crates/eidetic-core` -> `Code/crates/cose-examples`.
    let guess = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../crates/cose-examples");
    guess.join("eddsa-examples").is_dir().then_some(guess)
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            if !path.file_name().is_some_and(|n| n == ".git") {
                walk(&path, out);
            }
        } else if path.extension().is_some_and(|e| e == "json") {
            out.push(path);
        }
    }
}

fn key_bytes(key: &serde_json::Value, name: &str) -> Option<Vec<u8>> {
    if let Some(h) = key.get(format!("{name}_hex")).and_then(|v| v.as_str()) {
        return Some(unhex(h));
    }
    let b64 = key.get(name)?.as_str()?;
    URL_SAFE_NO_PAD
        .decode(b64)
        .or_else(|_| URL_SAFE.decode(b64))
        .ok()
}

/// Every Sign1 example signed with an EdDSA key: Ed25519 ones must verify
/// (and a `fail` one must not); Ed448 is counted as unsupported.
#[test]
#[ignore = "reads the vendored cose-wg Examples; see the module doc"]
fn cose_wg_sign1_eddsa_suite() {
    let Some(dir) = suite_dir() else {
        eprintln!("cose-wg Examples not found; set COSE_EXAMPLES_DIR");
        return;
    };
    let mut files = Vec::new();
    walk(&dir, &mut files);
    files.sort();
    let (mut verified, mut refused_fail, mut ed448) = (0, 0, 0);
    for path in &files {
        let json: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let Some(sign0) = json.pointer("/input/sign0") else {
            continue;
        };
        let crv = sign0
            .pointer("/key/crv")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let cbor = json
            .pointer("/output/cbor")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if !crv.starts_with("Ed") || !cbor.starts_with("D2") {
            continue;
        }
        let name = path.strip_prefix(&dir).unwrap().display().to_string();
        if crv == "Ed448" {
            ed448 += 1;
            eprintln!("unsupported (Ed448): {name}");
            continue;
        }
        let x = key_bytes(sign0.get("key").unwrap(), "x").unwrap();
        let public = Ed25519PublicKey::from_bytes(&arr32(&x)).unwrap();
        let message = CoseSign1::from_tagged_slice(&unhex(cbor)).expect(&name);
        if let Some(tbs) = json
            .pointer("/intermediates/ToBeSign_hex")
            .and_then(|v| v.as_str())
        {
            assert_eq!(message.tbs_data(b""), unhex(tbs), "{name}: Sig_structure");
        }
        let fail = json.get("fail").and_then(|v| v.as_bool()).unwrap_or(false);
        let result = check_signature(&message, b"", &public);
        if fail {
            assert!(result.is_err(), "{name}: a fail case verified");
            refused_fail += 1;
        } else {
            assert_eq!(result, Ok(()), "{name}");
            verified += 1;
        }
        eprintln!("{}: {name}", if fail { "refused" } else { "verified" });
    }
    eprintln!(
        "Ed25519 Sign1 verified {verified}, fail cases refused {refused_fail}, Ed448 unsupported {ed448}"
    );
    assert!(verified >= 1, "positive control: eddsa-sig-01 at least");
}
