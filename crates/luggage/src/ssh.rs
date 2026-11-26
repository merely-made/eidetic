// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Publish and install signed releases through a fixed OpenSSH command.

use std::{
    error::Error,
    fs::{self, File, OpenOptions},
    io::{self, BufRead, BufReader, Read, Seek, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use semver::Version;
use serde::Deserialize;
use serde_json::{json, Value};
use url::Url;

use crate::{
    signing, Config, Feed, RemoteRelease, RemoteReleaseData, UpdateFormat, UpdaterBuilder,
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
const PUBLISH: &[u8] = b"publish-v1\n";
const PUBLISHED: &str = "published.luggage";
const MAX_MANIFEST: usize = 256 * 1024;
const MAX_SIGNATURE: usize = 16 * 1024;
const MAX_ARTIFACT: usize = 128 * 1024 * 1024;

struct Bundle {
    manifest: Vec<u8>,
    signature: Vec<u8>,
    artifact: Vec<u8>,
}

impl Bundle {
    fn read(input: &mut impl Read) -> Result<Self> {
        fn part(input: &mut impl Read, max: usize) -> Result<Vec<u8>> {
            let mut length = [0; 4];
            input.read_exact(&mut length)?;
            let length = u32::from_be_bytes(length) as usize;
            if !(1..=max).contains(&length) {
                return Err("release frame part exceeds its size limit or is empty".into());
            }
            let mut bytes = vec![0; length];
            input.read_exact(&mut bytes)?;
            Ok(bytes)
        }
        let bundle = Self {
            manifest: part(input, MAX_MANIFEST)?,
            signature: part(input, MAX_SIGNATURE)?,
            artifact: part(input, MAX_ARTIFACT)?,
        };
        if input.read(&mut [0])? != 0 {
            return Err("unexpected data after the release frame".into());
        }
        Ok(bundle)
    }

    fn write(&self, mut output: impl Write) -> Result<()> {
        output.write_all(PUBLISH)?;
        for bytes in [&self.manifest, &self.signature, &self.artifact] {
            output.write_all(&u32::try_from(bytes.len())?.to_be_bytes())?;
            output.write_all(bytes)?;
        }
        output.flush()?;
        Ok(())
    }

    fn digest(&self) -> blake3::Hash {
        blake3::hash(&self.manifest)
    }

    fn verify(&self, policy: &Policy) -> Result<Version> {
        let key = policy.publisher_key.trim();
        let signature = signing::normalize_signature_file(str::from_utf8(&self.signature)?);
        signing::verify_bytes(&self.manifest, &signature, key)
            .map_err(|_| crate::Error::ManifestSignatureInvalid)?;
        let release: RemoteRelease = serde_json::from_slice(&self.manifest)?;
        if !matches!(release.data, RemoteReleaseData::Static { .. })
            || release.format(&policy.target)? != UpdateFormat::External
        {
            return Err(
                "published release must name its target and use the external format".into(),
            );
        }
        let locator = release
            .download_url(&policy.target)?
            .to_file_path()
            .map_err(|()| "published release must use a file in the configured feed")?;
        let parent = locator.parent().and_then(|path| path.canonicalize().ok());
        if parent.as_deref() != Some(policy.feed.as_path()) {
            return Err("published release must use a file directly in the configured feed".into());
        }
        let hash = release
            .blake3(&policy.target)?
            .ok_or("published release needs an artifact BLAKE3")?;
        signing::verify_artifact(
            &self.artifact,
            release.signature(&policy.target)?,
            Some(&hash),
            key,
        )?;
        Ok(release.version)
    }
}

fn read_header(input: &mut impl BufRead) -> Result<Vec<u8>> {
    let mut header = Vec::new();
    input.take(66).read_until(b'\n', &mut header)?;
    if header.pop() != Some(b'\n') {
        return Err("expected a manifest BLAKE3 or publish-v1 followed by a newline".into());
    }
    if header.last() == Some(&b'\r') {
        header.pop();
    }
    Ok(header)
}

fn read_published(path: &Path) -> Result<Option<Bundle>> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let mut input = BufReader::new(file);
    if read_header(&mut input)? != b"publish-v1" {
        return Err("protected feed has an invalid publication header".into());
    }
    Ok(Some(Bundle::read(&mut input)?))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Policy {
    feed: PathBuf,
    publisher_key: String,
    target: String,
    installer: PathBuf,
}

impl Policy {
    fn command(&self) -> Command {
        let mut command = Command::new(&self.installer);
        command
            .env_clear()
            .env("PATH", "/usr/sbin:/usr/bin:/sbin:/bin")
            .current_dir("/")
            .stdin(Stdio::null());
        command
    }

    fn running_version(&self) -> Result<Version> {
        let output = self.command().arg("running-version").output()?;
        if !output.status.success() {
            return Err(format!("running-version failed ({})", output.status).into());
        }
        Ok(str::from_utf8(&output.stdout)?.trim().parse()?)
    }

    fn legacy_release(
        &self,
        installed: Version,
        digest: blake3::Hash,
    ) -> Result<(Version, Vec<u8>)> {
        let mut update = UpdaterBuilder::new(
            installed,
            Config {
                feeds: vec![Feed::Directory(self.feed.clone())],
                pubkey: self.publisher_key.trim().to_owned(),
                ..Config::default()
            },
        )
        .target(self.target.as_str())
        .version_comparator(|installed, release| {
            matches!(release.data, RemoteReleaseData::Static { .. })
                && release.version.cmp_precedence(&installed).is_gt()
        })
        .expected_manifest_blake3(*digest.as_bytes())
        .build()?
        .check()?
        .ok_or("requested release must name its platform and be newer than the running version")?;
        if update.format != UpdateFormat::External || update.blake3.is_none() {
            return Err(
                "requested release needs the external format and an artifact BLAKE3".into(),
            );
        }
        let artifact = update
            .download_url
            .to_file_path()
            .map_err(|()| "requested release must use a file in the configured feed")?
            .canonicalize()?;
        if !artifact.starts_with(&self.feed) || !artifact.is_file() {
            return Err("requested release must use a file in the configured feed".into());
        }
        update.download_url =
            Url::from_file_path(artifact).map_err(|()| "invalid artifact path")?;
        Ok((update.version.parse()?, update.download()?))
    }
}

fn require_upgrade(version: &Version, running: &Version) -> Result<()> {
    if !version.cmp_precedence(running).is_gt() {
        return Err("requested release must be newer than the running version".into());
    }
    Ok(())
}

fn publish(policy: &Policy, path: &Path, running: &Version, bundle: Bundle) -> Result<Value> {
    let version = bundle.verify(policy)?;
    require_upgrade(&version, running)?;
    let digest = bundle.digest();
    if let Some(previous) = read_published(path)? {
        if previous.digest() != digest {
            let previous_version = previous.verify(policy)?;
            if !version.cmp_precedence(&previous_version).is_gt() {
                return Err("publication must be newer than the existing staged release".into());
            }
        }
    }
    let mut file = tempfile::NamedTempFile::new_in(&policy.feed)?;
    bundle.write(&mut file)?;
    file.as_file().sync_all()?;
    file.persist(path)?;
    #[cfg(unix)]
    File::open(&policy.feed)?.sync_all()?;
    Ok(receipt("published", &version, digest))
}

fn install(
    policy: &Policy,
    policy_path: &Path,
    path: &Path,
    running: Version,
    digest: blake3::Hash,
) -> Result<Value> {
    let (version, bytes) = if let Some(published) = read_published(path)? {
        if published.digest() != digest {
            return Err(crate::Error::ManifestDigestMismatch.into());
        }
        let version = published.verify(policy)?;
        require_upgrade(&version, &running)?;
        (version, published.artifact)
    } else {
        policy.legacy_release(running, digest)?
    };

    let mut staged = tempfile::tempfile_in(policy_path.parent().ok_or("invalid policy path")?)?;
    staged.write_all(&bytes)?;
    staged.rewind()?;
    let status = policy
        .command()
        .args(["install", &version.to_string()])
        .stdin(staged)
        .stdout(Stdio::null())
        .status()?;
    if !status.success() {
        return Err(format!("installer failed ({status})").into());
    }
    let running = policy.running_version()?;
    if running != version {
        return Err(format!("installer returned success but version {running} is running").into());
    }
    Ok(receipt("installed", &running, digest))
}

fn receipt(status: &str, version: &Version, digest: blake3::Hash) -> Value {
    json!({
        "status": status,
        "version": version.to_string(),
        "manifest_blake3": digest.to_hex().as_str(),
    })
}

/// Handle one signed publication or manifest-digest install request, using a
/// local administrator-owned policy and feed. The caller authenticates the
/// operator before starting this function, for example through OpenSSH.
///
/// # Errors
///
/// Refuses malformed or untrusted requests, stale releases, and failed installs.
pub fn serve(policy_path: &Path, request: impl Read) -> Result<Value> {
    let mut input = BufReader::new(request);
    let header = read_header(&mut input)?;
    let upload = if header == b"publish-v1" {
        Some(Bundle::read(&mut input)?)
    } else {
        None
    };
    let mut policy: Policy = serde_json::from_slice(&fs::read(policy_path)?)?;
    if !policy.feed.is_absolute() || !policy.installer.is_absolute() || policy.target.is_empty() {
        return Err("policy requires absolute feed and installer paths and a target".into());
    }
    policy.feed = policy.feed.canonicalize()?;
    let running_lock = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(policy_path.with_extension("lock"))?;
    running_lock.lock()?;
    let running = policy.running_version()?;
    let path = policy.feed.join(PUBLISHED);
    if let Some(bundle) = upload {
        publish(&policy, &path, &running, bundle)
    } else {
        install(
            &policy,
            policy_path,
            &path,
            running,
            blake3::Hash::from_hex(&header)?,
        )
    }
}

/// Write the binary upload request for a manifest, its adjacent `.sig`, and
/// the artifact for the target. Send it to an enrolled OpenSSH update account.
///
/// # Errors
///
/// Returns an error for missing, oversized or empty files and on I/O failure.
pub fn write_publication(manifest: &Path, artifact: &Path, output: impl Write) -> Result<()> {
    fn read_part(path: &Path, max: usize) -> Result<Vec<u8>> {
        let mut bytes = Vec::new();
        File::open(path)?
            .take(max as u64 + 1)
            .read_to_end(&mut bytes)?;
        if !(1..=max).contains(&bytes.len()) {
            return Err(format!("{} exceeds its size limit or is empty", path.display()).into());
        }
        Ok(bytes)
    }
    let mut signature = manifest.as_os_str().to_os_string();
    signature.push(".sig");
    Bundle {
        manifest: read_part(manifest, MAX_MANIFEST)?,
        signature: read_part(Path::new(&signature), MAX_SIGNATURE)?,
        artifact: read_part(artifact, MAX_ARTIFACT)?,
    }
    .write(output)
}

#[cfg(test)]
mod tests;
