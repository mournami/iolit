//! Signed manifests are verified before any downloaded file can be used.
//!
//! A bundle is published as two files: the archive `bundle-<version>.tar.gz` and
//! `manifest.json`, a [`SignedManifest`] whose payload is a [`Manifest`].
//! `contracts/bundle-manifest.schema.json` is generated from these types.
use anyhow::{Context, Result, bail};
use base64::{Engine, engine::general_purpose::STANDARD};
use ed25519_dalek::{Signature, VerifyingKey};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    cmp::Ordering,
    collections::{BTreeMap, HashSet},
    fs,
    io::Read,
    path::{Component, Path, PathBuf},
};

pub const MANIFEST_SCHEMA: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FileHash {
    /// Relative path with `/` separators.
    pub path: String,
    /// Lowercase hex SHA-256.
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub schema: u32,
    /// Dot-separated numbers, `YYYY.MM.DD.HHMM` in UTC; compared numerically.
    pub version: String,
    /// Flowseal release tag the strategies and lists were built from. On Windows the
    /// bol-van engine binaries ship inside that release; their hashes are in `files`.
    pub flowseal: String,
    /// The archive itself, checked before extraction.
    pub archive: FileHash,
    /// Every file inside the archive, relative to its root.
    pub files: Vec<FileHash>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SignedManifest {
    /// Base64 of the exact manifest JSON bytes that were signed.
    pub payload: String,
    /// Base64 ed25519 signature over the decoded payload.
    pub signature: String,
}

/// Root type for schema generation only.
#[derive(JsonSchema)]
pub struct BundleContract {
    pub envelope: SignedManifest,
    pub payload: Manifest,
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

pub fn valid_version(version: &str) -> bool {
    let parts: Vec<_> = version.split('.').collect();
    (1..=4).contains(&parts.len())
        && parts
            .iter()
            .all(|p| (1..=8).contains(&p.len()) && p.bytes().all(|b| b.is_ascii_digit()))
}

/// Numeric, part by part; missing parts count as zero. Callers validate first.
pub fn compare_versions(a: &str, b: &str) -> Ordering {
    let parse = |v: &str| -> Vec<u64> { v.split('.').map(|p| p.parse().unwrap_or(0)).collect() };
    let (a, b) = (parse(a), parse(b));
    (0..a.len().max(b.len()))
        .map(|i| a.get(i).unwrap_or(&0).cmp(b.get(i).unwrap_or(&0)))
        .find(|o| o.is_ne())
        .unwrap_or(Ordering::Equal)
}

fn valid_relative_path(path: &str) -> bool {
    !path.is_empty()
        && !path.contains('\\')
        && Path::new(path)
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
}

fn valid_sha256(hex: &str) -> bool {
    hex.len() == 64
        && hex
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

impl Manifest {
    /// Structural checks shared by the builder and the client.
    pub fn validate(&self) -> Result<()> {
        if self.schema != MANIFEST_SCHEMA {
            bail!("Неподдерживаемая версия манифеста");
        }
        if !valid_version(&self.version) || self.flowseal.trim().is_empty() {
            bail!("Некорректная версия набора");
        }
        if self.archive.path.contains('/')
            || !valid_relative_path(&self.archive.path)
            || !valid_sha256(&self.archive.sha256)
        {
            bail!("Некорректное описание архива");
        }
        if self.files.is_empty() {
            bail!("В манифесте нет файлов");
        }
        let mut seen = HashSet::new();
        for f in &self.files {
            if !valid_relative_path(&f.path) || !valid_sha256(&f.sha256) || !seen.insert(&f.path) {
                bail!("Недопустимый путь в манифесте");
            }
        }
        Ok(())
    }

    pub fn verify_archive(&self, bytes: &[u8]) -> Result<()> {
        if sha256_hex(bytes) != self.archive.sha256 {
            bail!("Контрольная сумма архива не совпала");
        }
        Ok(())
    }
}

pub fn verify_signature(envelope: &SignedManifest, key: &[u8; 32]) -> Result<Manifest> {
    let payload = STANDARD.decode(&envelope.payload)?;
    let signature = Signature::from_slice(&STANDARD.decode(&envelope.signature)?)?;
    VerifyingKey::from_bytes(key)?
        .verify_strict(&payload, &signature)
        .context("Подпись набора недействительна")?;
    let manifest: Manifest = serde_json::from_slice(&payload)?;
    manifest.validate()?;
    Ok(manifest)
}

/// Every manifest file exists under `root` with the listed hash.
pub fn verify_files(manifest: &Manifest, root: &Path) -> Result<()> {
    manifest.validate()?;
    let root = root.canonicalize()?;
    for f in &manifest.files {
        let file = root.join(&f.path).canonicalize()?;
        if !file.starts_with(&root) || !file.is_file() {
            bail!("Файл за пределами набора");
        }
        if sha256_hex(&std::fs::read(&file)?) != f.sha256 {
            bail!("Контрольная сумма {} не совпала", f.path);
        }
    }
    Ok(())
}

/// Files under `dir`, keyed by `prefix/relative/path` with `/` separators.
pub fn walk(dir: &Path, prefix: &str, out: &mut BTreeMap<String, PathBuf>) -> Result<()> {
    for entry in fs::read_dir(dir).with_context(|| dir.display().to_string())? {
        let entry = entry?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|n| anyhow::anyhow!("Non-UTF-8 file name: {n:?}"))?;
        let path = if prefix.is_empty() {
            name
        } else {
            format!("{prefix}/{name}")
        };
        let kind = entry.file_type()?;
        if kind.is_dir() {
            walk(&entry.path(), &path, out)?;
        } else if kind.is_file() {
            out.insert(path, entry.path());
        } else {
            bail!("Unsupported file type: {}", entry.path().display());
        }
    }
    Ok(())
}

/// [`verify_files`] plus: nothing under `root` that the manifest does not list.
pub fn verify_tree(manifest: &Manifest, root: &Path) -> Result<()> {
    verify_files(manifest, root)?;
    let mut found = BTreeMap::new();
    walk(root, "", &mut found)?;
    if found.len() != manifest.files.len() {
        bail!("В наборе есть файлы, которых нет в манифесте");
    }
    Ok(())
}

/// Deterministic `.tar.gz`: given order, fixed metadata, no directory entries.
pub fn pack_archive<'a>(entries: impl IntoIterator<Item = (&'a str, &'a [u8])>) -> Result<Vec<u8>> {
    let mut builder = tar::Builder::new(Vec::new());
    for (path, data) in entries {
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Regular);
        header.set_size(data.len() as u64);
        header.set_mode(0o644);
        header.set_mtime(0);
        header.set_uid(0);
        header.set_gid(0);
        builder.append_data(&mut header, path, data)?;
    }
    let tar = builder.into_inner()?;
    let mut gz = flate2::GzBuilder::new()
        .mtime(0)
        .write(Vec::new(), flate2::Compression::best());
    std::io::Write::write_all(&mut gz, &tar)?;
    Ok(gz.finish()?)
}

/// Extract only regular files and directories that stay inside `dest`.
pub fn extract_tar_gz(bytes: &[u8], dest: &Path) -> Result<()> {
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(bytes));
    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.into_owned();
        if path.as_os_str().is_empty()
            || path
                .components()
                .any(|c| !matches!(c, Component::Normal(_) | Component::CurDir))
        {
            bail!("Unsafe archive path: {}", path.display());
        }
        let out = dest.join(&path);
        match entry.header().entry_type() {
            tar::EntryType::Directory => fs::create_dir_all(&out)?,
            tar::EntryType::Regular => {
                fs::create_dir_all(out.parent().context("archive file parent")?)?;
                let mut data = Vec::new();
                entry.read_to_end(&mut data)?;
                fs::write(&out, data)?;
            }
            // pax/GNU metadata headers carry no files of their own.
            tar::EntryType::XGlobalHeader | tar::EntryType::XHeader => {}
            other => bail!("Unsupported archive entry {other:?}: {}", path.display()),
        }
    }
    Ok(())
}

/// The full client check: signature, archive hash, extraction into an emptied
/// `dest`, every file hash and no unlisted files. `dest` is removed on failure.
pub fn unpack_verified(
    envelope: &[u8],
    key: &[u8; 32],
    archive: &[u8],
    dest: &Path,
) -> Result<Manifest> {
    let envelope: SignedManifest =
        serde_json::from_slice(envelope).context("Манифест повреждён")?;
    let manifest = verify_signature(&envelope, key)?;
    manifest.verify_archive(archive)?;
    if dest.exists() {
        fs::remove_dir_all(dest)?;
    }
    let result = extract_tar_gz(archive, dest).and_then(|()| verify_tree(&manifest, dest));
    if result.is_err() {
        let _ = fs::remove_dir_all(dest);
    }
    result.map(|()| manifest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    fn manifest() -> Manifest {
        Manifest {
            schema: MANIFEST_SCHEMA,
            version: "2026.11.11.1200".into(),
            flowseal: "1.10.4".into(),
            archive: FileHash {
                path: "bundle-2026.11.11.1200.tar.gz".into(),
                sha256: sha256_hex(b"archive"),
            },
            files: vec![FileHash {
                path: "windows/strategies.json".into(),
                sha256: sha256_hex(b"[]"),
            }],
        }
    }

    #[test]
    fn tampered_manifest_is_rejected() {
        let key = SigningKey::from_bytes(&[7; 32]);
        let payload = serde_json::to_vec(&manifest()).unwrap();
        let mut env = SignedManifest {
            payload: STANDARD.encode(&payload),
            signature: STANDARD.encode(key.sign(&payload).to_bytes()),
        };
        assert_eq!(
            verify_signature(&env, key.verifying_key().as_bytes()).unwrap(),
            manifest()
        );
        env.payload = STANDARD.encode(b"tampered");
        assert!(verify_signature(&env, key.verifying_key().as_bytes()).is_err());
        env.signature = STANDARD.encode([0; 64]);
        assert!(verify_signature(&env, key.verifying_key().as_bytes()).is_err());
    }

    #[test]
    fn structural_checks() {
        assert!(manifest().validate().is_ok());
        for version in ["", "1.2.3.4.5", "1.x", "1..2", "v1"] {
            assert!(!valid_version(version), "{version}");
        }
        let mut m = manifest();
        m.files[0].path = "../escape".into();
        assert!(m.validate().is_err());
        let mut m = manifest();
        m.files.push(m.files[0].clone());
        assert!(m.validate().is_err());
        let mut m = manifest();
        m.archive.path = "dir/bundle.tar.gz".into();
        assert!(m.validate().is_err());
        let mut m = manifest();
        m.files[0].sha256 = "00".into();
        assert!(m.validate().is_err());
        assert!(manifest().verify_archive(b"archive").is_ok());
        assert!(manifest().verify_archive(b"other").is_err());
        let extra = r#"{"schema":1,"version":"1","flowseal":"x","archive":{"path":"a","sha256":"00"},"files":[],"extra":1}"#;
        assert!(serde_json::from_str::<Manifest>(extra).is_err());
    }

    #[test]
    fn versions_compare_numerically() {
        assert_eq!(
            compare_versions("2026.11.11.900", "2026.11.11.1200"),
            Ordering::Less
        );
        assert_eq!(
            compare_versions("2026.11.12", "2026.11.11.2359"),
            Ordering::Greater
        );
        assert_eq!(compare_versions("1.0", "1"), Ordering::Equal);
        assert_eq!(compare_versions("0", "2026.1.1.0"), Ordering::Less);
    }
}
