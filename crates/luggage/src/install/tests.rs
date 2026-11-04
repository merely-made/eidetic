// Copyright 2019-2023 Tauri Programme within The Commons Conservancy
// Copyright 2023-2023 CrabNebula Ltd.
// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;
use crate::config::Feed;
use crate::release::MANIFEST_NAME;
use crate::{Config, UpdaterBuilder};
use base64::Engine as _;
use std::io::Write as _;

/// Sign bytes with a fresh in-process minisign key, returning the
/// base64-of-boxed forms download() expects for (pubkey, signature).
fn sign(data: &[u8]) -> (String, String) {
    let keypair = minisign::KeyPair::generate_unencrypted_keypair().unwrap();
    let signature =
        minisign::sign(None, &keypair.sk, std::io::Cursor::new(data), None, None).unwrap();
    let engine = base64::engine::general_purpose::STANDARD;
    (
        engine.encode(keypair.pk.to_box().unwrap().to_string()),
        engine.encode(signature.to_string()),
    )
}

fn file_url(path: &std::path::Path) -> String {
    Url::from_file_path(path).unwrap().to_string()
}

/// Build an Update via a real directory feed + manifest.
fn update_for(dir: &std::path::Path, artifact: &[u8], blake3: Option<String>) -> Update {
    let artifact_path = dir.join("artifact.exe");
    std::fs::File::create(&artifact_path)
        .unwrap()
        .write_all(artifact)
        .unwrap();
    let (pubkey, signature) = sign(artifact);
    let manifest = serde_json::json!({
        "version": "0.2.0",
        "platforms": {
            "test-target": {
                "url": file_url(&artifact_path),
                "signature": signature,
                "blake3": blake3,
                "format": "nsis",
            }
        }
    });
    std::fs::write(
        dir.join(MANIFEST_NAME),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();

    UpdaterBuilder::new(
        "0.1.0".parse().unwrap(),
        Config {
            feeds: vec![Feed::Directory(dir.to_path_buf())],
            pubkey,
            // These tests are about artifact verification; the manifest
            // signature has its own tests in `updater`.
            require_signed_manifest: false,
            ..Default::default()
        },
    )
    .target("test-target".to_string())
    .executable_path(dir.join("fake-exe"))
    .build()
    .unwrap()
    .check()
    .unwrap()
    .expect("update should be offered")
}

#[test]
fn download_verifies_blake3_and_signature_from_a_directory_feed() {
    let dir = tempfile::tempdir().unwrap();
    let artifact = b"pretend installer bytes";
    let digest = blake3::hash(artifact).to_hex().to_string();
    let update = update_for(dir.path(), artifact, Some(digest));
    let bytes = update.download().expect("verified download");
    assert_eq!(bytes, artifact);
}

#[test]
fn a_wrong_blake3_fails_before_the_signature_check() {
    let dir = tempfile::tempdir().unwrap();
    let update = update_for(dir.path(), b"real bytes", Some("00".repeat(32)));
    let err = update.download().unwrap_err();
    assert!(matches!(err, Error::Blake3Mismatch { .. }), "got: {err}");
}

#[test]
fn a_manifest_without_blake3_still_verifies_the_signature() {
    let dir = tempfile::tempdir().unwrap();
    let artifact = b"unsummed but signed";
    let update = update_for(dir.path(), artifact, None);
    assert_eq!(update.download().unwrap(), artifact);
}

#[test]
fn tampered_bytes_fail_the_signature() {
    let dir = tempfile::tempdir().unwrap();
    let update = update_for(dir.path(), b"original bytes", None);
    // Overwrite the artifact after signing.
    let artifact_path = dir.path().join("artifact.exe");
    std::fs::write(&artifact_path, b"tampered bytes!").unwrap();
    let err = update.download().unwrap_err();
    assert!(matches!(err, Error::Minisign(_)), "got: {err}");
}
