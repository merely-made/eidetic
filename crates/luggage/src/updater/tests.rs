// Copyright 2019-2023 Tauri Programme within The Commons Conservancy
// Copyright 2023-2023 CrabNebula Ltd.
// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;
use crate::release::{ReleaseManifestPlatform, RemoteReleaseData, UpdateFormat};

fn updater_for(feeds: Vec<Feed>, current: &str) -> Updater {
    UpdaterBuilder::new(
        current.parse().unwrap(),
        Config {
            feeds,
            pubkey: String::new(),
            // Feed selection is under test here; the manifest-signature
            // policy has its own tests below.
            require_signed_manifest: false,
            ..Default::default()
        },
    )
    .target("test-target".to_string())
    .executable_path(std::env::temp_dir().join("luggage-test-exe"))
    .build()
    .unwrap()
}

fn write_manifest(dir: &Path, version: &str) {
    let manifest = serde_json::json!({
        "version": version,
        "platforms": {
            "test-target": {
                "url": "file:///tmp/app-artifact",
                "signature": "c2ln",
                "blake3": "00",
                "format": "nsis",
            }
        }
    });
    std::fs::write(
        dir.join(MANIFEST_NAME),
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();
}

#[test]
fn no_feeds_is_a_build_error() {
    let result = UpdaterBuilder::new(
        "0.1.0".parse().unwrap(),
        Config {
            feeds: vec![],
            ..Default::default()
        },
    )
    .build();
    assert!(matches!(result, Err(Error::EmptyFeeds)));
}

#[test]
fn directory_feed_finds_a_newer_release() {
    let dir = tempfile::tempdir().unwrap();
    write_manifest(dir.path(), "0.2.0");
    let updater = updater_for(vec![Feed::Directory(dir.path().to_path_buf())], "0.1.0");
    let update = updater.check().unwrap().expect("update should be offered");
    assert_eq!(update.version, "0.2.0");
    assert_eq!(update.blake3.as_deref(), Some("00"));
    assert_eq!(update.format, UpdateFormat::Nsis);
}

#[test]
fn directory_feed_with_the_same_version_offers_nothing() {
    let dir = tempfile::tempdir().unwrap();
    write_manifest(dir.path(), "0.1.0");
    let updater = updater_for(vec![Feed::Directory(dir.path().to_path_buf())], "0.1.0");
    assert!(updater.check().unwrap().is_none());
}

#[test]
fn a_missing_manifest_is_a_named_error() {
    let dir = tempfile::tempdir().unwrap();
    let updater = updater_for(vec![Feed::Directory(dir.path().to_path_buf())], "0.1.0");
    let err = updater.check().unwrap_err();
    assert!(matches!(err, Error::ManifestNotFound { .. }), "got: {err}");
}

#[test]
fn a_later_feed_serves_when_an_earlier_one_fails() {
    let empty = tempfile::tempdir().unwrap();
    let good = tempfile::tempdir().unwrap();
    write_manifest(good.path(), "0.3.0");
    let updater = updater_for(
        vec![
            Feed::Directory(empty.path().to_path_buf()),
            Feed::Directory(good.path().to_path_buf()),
        ],
        "0.1.0",
    );
    let update = updater.check().unwrap().expect("second feed should serve");
    assert_eq!(update.version, "0.3.0");
}

#[test]
fn custom_version_comparator_wins() {
    let dir = tempfile::tempdir().unwrap();
    write_manifest(dir.path(), "9.9.9");
    let updater = UpdaterBuilder::new(
        "0.1.0".parse().unwrap(),
        Config {
            feeds: vec![Feed::Directory(dir.path().to_path_buf())],
            require_signed_manifest: false,
            ..Default::default()
        },
    )
    .target("test-target".to_string())
    .executable_path(std::env::temp_dir().join("luggage-test-exe"))
    .version_comparator(|_current, _release| false)
    .build()
    .unwrap();
    assert!(updater.check().unwrap().is_none(), "comparator said no");
}

// ─── Manifest signing (T3): the downgrade gap ─────────────────────────

/// Write a manifest and, optionally, a detached signature over exactly
/// the bytes written. Returns the base64 public key.
fn write_signed_manifest(dir: &Path, version: &str, sign_it: bool) -> String {
    use base64::Engine as _;
    let manifest = serde_json::json!({
        "version": version,
        "platforms": {
            "test-target": {
                "url": "file:///tmp/app-artifact",
                "signature": "c2ln",
                "format": "nsis",
            }
        }
    });
    let bytes = serde_json::to_vec_pretty(&manifest).unwrap();
    std::fs::write(dir.join(MANIFEST_NAME), &bytes).unwrap();

    let keypair = minisign::KeyPair::generate_unencrypted_keypair().unwrap();
    if sign_it {
        let signature =
            minisign::sign(None, &keypair.sk, std::io::Cursor::new(&bytes), None, None).unwrap();
        // As `cargo packager signer sign` leaves it: raw minisign text.
        std::fs::write(
            dir.join(format!("{MANIFEST_NAME}.sig")),
            signature.to_string(),
        )
        .unwrap();
    }
    base64::engine::general_purpose::STANDARD.encode(keypair.pk.to_box().unwrap().to_string())
}

fn strict_updater(dir: &Path, pubkey: String) -> Updater {
    UpdaterBuilder::new(
        "0.1.0".parse().unwrap(),
        Config {
            feeds: vec![Feed::Directory(dir.to_path_buf())],
            pubkey,
            // The default, spelled out because it is the point here.
            require_signed_manifest: true,
            ..Default::default()
        },
    )
    .target("test-target".to_string())
    .executable_path(std::env::temp_dir().join("luggage-test-exe"))
    .build()
    .unwrap()
}

#[test]
fn a_signed_manifest_is_accepted() {
    let dir = tempfile::tempdir().unwrap();
    let pubkey = write_signed_manifest(dir.path(), "0.2.0", true);
    let update = strict_updater(dir.path(), pubkey)
        .check()
        .unwrap()
        .expect("a signed manifest should be believed");
    assert_eq!(update.version, "0.2.0");
}

#[test]
fn an_unsigned_manifest_is_refused_by_default() {
    let dir = tempfile::tempdir().unwrap();
    let pubkey = write_signed_manifest(dir.path(), "0.2.0", false);
    let err = strict_updater(dir.path(), pubkey).check().unwrap_err();
    assert!(matches!(err, Error::ManifestUnsigned), "got: {err}");
}

#[test]
fn editing_a_signed_manifest_invalidates_it() {
    // THE downgrade attack: the feed keeps a real signed artifact and
    // rewrites the version around it. With the manifest signed, the edit
    // no longer verifies, so the lie is refused.
    let dir = tempfile::tempdir().unwrap();
    let pubkey = write_signed_manifest(dir.path(), "0.2.0", true);
    let manifest_path = dir.path().join(MANIFEST_NAME);
    let tampered = std::fs::read_to_string(&manifest_path)
        .unwrap()
        .replace("0.2.0", "9.9.9");
    std::fs::write(&manifest_path, tampered).unwrap();

    let err = strict_updater(dir.path(), pubkey).check().unwrap_err();
    assert!(
        matches!(err, Error::ManifestSignatureInvalid),
        "a rewritten version must not verify, got: {err}"
    );
}

#[test]
fn a_signature_from_another_key_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    // Signed by the key this manifest came with, checked against another.
    write_signed_manifest(dir.path(), "0.2.0", true);
    let other = write_signed_manifest(&tempfile::tempdir().unwrap().keep(), "0.2.0", true);
    let err = strict_updater(dir.path(), other).check().unwrap_err();
    assert!(matches!(err, Error::ManifestSignatureInvalid), "got: {err}");
}

#[test]
fn opting_out_accepts_an_unsigned_manifest() {
    // The escape hatch exists, but it has to be asked for in code.
    let dir = tempfile::tempdir().unwrap();
    write_signed_manifest(dir.path(), "0.2.0", false);
    let updater = updater_for(vec![Feed::Directory(dir.path().to_path_buf())], "0.1.0");
    assert!(updater.check().unwrap().is_some());
}

#[test]
fn release_accessors_respect_the_requested_target() {
    // Static data keyed by target must not leak across targets.
    let release = RemoteRelease {
        version: "1.0.0".parse().unwrap(),
        notes: None,
        pub_date: None,
        data: RemoteReleaseData::Static {
            platforms: [(
                "linux-x86_64".to_string(),
                ReleaseManifestPlatform {
                    url: "https://e.com/a".parse().unwrap(),
                    signature: "s".into(),
                    blake3: None,
                    format: UpdateFormat::AppImage,
                },
            )]
            .into(),
        },
    };
    assert!(release.download_url("linux-x86_64").is_ok());
    assert!(release.download_url("windows-x86_64").is_err());
}
