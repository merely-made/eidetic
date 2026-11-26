// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use std::io::Cursor;

use base64::{engine::general_purpose::STANDARD, Engine as _};
use minisign::KeyPair;

use super::*;

fn signed_bundle(policy: &Policy, key: &KeyPair, version: &str) -> Bundle {
    let sign = |bytes: &[u8]| {
        minisign::sign(None, &key.sk, Cursor::new(bytes), None, None)
            .unwrap()
            .to_string()
    };
    let artifact = format!("app {version}").into_bytes();
    let manifest = serde_json::to_vec(&json!({
        "version": version,
        "platforms": { (&policy.target): {
            "url": Url::from_file_path(policy.feed.join("app.bin")).unwrap(),
            "signature": STANDARD.encode(sign(&artifact)),
            "blake3": blake3::hash(&artifact).to_hex().as_str(),
            "format": "external",
        }}
    }))
    .unwrap();
    Bundle {
        signature: sign(&manifest).into_bytes(),
        manifest,
        artifact,
    }
}

#[test]
fn publication_replaces_only_with_a_verified_newer_release() {
    let dir = tempfile::tempdir().unwrap();
    let key = KeyPair::generate_unencrypted_keypair().unwrap();
    let policy = Policy {
        feed: dir.path().canonicalize().unwrap(),
        publisher_key: STANDARD.encode(key.pk.to_box().unwrap().to_string()),
        target: "app-test".into(),
        installer: dir.path().join("unused"),
    };
    let path = policy.feed.join(PUBLISHED);
    let running = "1.0.0".parse().unwrap();
    let current = signed_bundle(&policy, &key, "1.1.0");
    let original = current.digest();
    assert_eq!(
        publish(&policy, &path, &running, current).unwrap()["status"],
        "published"
    );

    let mut tampered = signed_bundle(&policy, &key, "1.2.0");
    tampered.artifact.push(0);
    assert!(publish(&policy, &path, &running, tampered).is_err());
    assert_eq!(read_published(&path).unwrap().unwrap().digest(), original);

    let replacement = signed_bundle(&policy, &key, "1.2.0");
    let new_digest = replacement.digest();
    publish(&policy, &path, &running, replacement).unwrap();
    let old = signed_bundle(&policy, &key, "1.1.0");
    assert!(publish(&policy, &path, &running, old).is_err());
    assert_eq!(read_published(&path).unwrap().unwrap().digest(), new_digest);
}

#[test]
fn incomplete_or_extra_upload_bytes_are_refused() {
    let input = Bundle {
        manifest: vec![1],
        signature: vec![2],
        artifact: vec![3],
    };
    let mut frame = Vec::new();
    input.write(&mut frame).unwrap();
    let body = &frame[PUBLISH.len()..];
    assert!(Bundle::read(&mut &body[..body.len() - 1]).is_err());
    frame.push(4);
    assert!(Bundle::read(&mut &frame[PUBLISH.len()..]).is_err());
}

#[cfg(unix)]
#[test]
fn receiver_publishes_then_installs_by_digest() {
    use std::os::unix::fs::PermissionsExt as _;

    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let key = KeyPair::generate_unencrypted_keypair().unwrap();
    let policy = Policy {
        feed: root.join("feed"),
        publisher_key: STANDARD.encode(key.pk.to_box().unwrap().to_string()),
        target: "app-test".into(),
        installer: root.join("installer"),
    };
    fs::create_dir(&policy.feed).unwrap();
    let running = root.join("running");
    fs::write(&running, "1.0.0\n").unwrap();
    fs::write(
        &policy.installer,
        r#"#!/bin/sh
set -eu
root=$(dirname "$0")
case "$1" in
    running-version) cat "$root/running" ;;
    install)
        [ "$(cat)" = "app $2" ]
        printf '%s\n' "$2" > "$root/running"
        ;;
    *) exit 2 ;;
esac
"#,
    )
    .unwrap();
    fs::set_permissions(&policy.installer, fs::Permissions::from_mode(0o700)).unwrap();
    let policy_file = root.join("policy.json");
    fs::write(
        &policy_file,
        serde_json::to_vec(&json!({
            "feed": policy.feed,
            "publisher_key": policy.publisher_key,
            "target": policy.target,
            "installer": policy.installer,
        }))
        .unwrap(),
    )
    .unwrap();
    let bundle = signed_bundle(&policy, &key, "1.1.0");
    let request = format!("{}\n", bundle.digest());
    let mut upload = Vec::new();
    bundle.write(&mut upload).unwrap();
    assert_eq!(
        serve(&policy_file, upload.as_slice()).unwrap()["status"],
        "published"
    );
    assert_eq!(fs::read_to_string(&running).unwrap(), "1.0.0\n");
    assert_eq!(
        serve(&policy_file, request.as_bytes()).unwrap()["status"],
        "installed"
    );
    assert_eq!(fs::read_to_string(&running).unwrap(), "1.1.0\n");
    assert!(serve(&policy_file, request.as_bytes()).is_err());
}
