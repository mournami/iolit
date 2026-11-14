//! Signed strategy bundles: archive and manifest, signature, the client-side check
//! and release notes. The production key only ever comes from the environment.
use anyhow::{Context, Result, bail};
use base64::{Engine, engine::general_purpose::STANDARD};
use ed25519_dalek::{Signer, SigningKey};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
use zapret_core::{
    Strategy,
    bundle::{self, FileHash, MANIFEST_SCHEMA, Manifest, SignedManifest},
};

pub const KEY_ENV: &str = "ZAPRET_BUNDLE_SIGNING_KEY";
pub const UNSIGNED_MANIFEST: &str = "manifest.unsigned.json";

/// Bundle contents: the prepared Windows runtime and licenses. The empty
/// `*-user.txt` placeholders stay in: strategies reference them with `=@`, which
/// must resolve. Real user domains live in service data and are never bundled.
fn bundle_files(root: &Path) -> Result<BTreeMap<String, PathBuf>> {
    let runtime = root.join("resources/runtime");
    let mut files = BTreeMap::new();
    bundle::walk(&runtime.join("windows"), "windows", &mut files)?;
    bundle::walk(&runtime.join("licenses"), "licenses", &mut files)?;
    if !files.contains_key("windows/strategies.json") {
        bail!("No prepared runtime: run `zbuild prepare-windows` first");
    }
    Ok(files)
}

fn archive(files: &BTreeMap<String, PathBuf>) -> Result<Vec<u8>> {
    let mut entries = Vec::new();
    for (path, source) in files {
        entries.push((path.as_str(), fs::read(source)?));
    }
    bundle::pack_archive(entries.iter().map(|(p, d)| (*p, d.as_slice())))
}

/// Writes `bundle-<version>.tar.gz` and the unsigned manifest into `out`.
pub fn bundle(root: &Path, version: &str, flowseal: &str, out: &Path) -> Result<Manifest> {
    if !bundle::valid_version(version) {
        bail!("Invalid bundle version {version:?}; expected e.g. 2026.11.11.1200");
    }
    let files = bundle_files(root)?;
    let bytes = archive(&files)?;
    let mut hashes = Vec::new();
    for (path, source) in &files {
        hashes.push(FileHash {
            path: path.clone(),
            sha256: bundle::sha256_hex(&fs::read(source)?),
        });
    }
    let manifest = Manifest {
        schema: MANIFEST_SCHEMA,
        version: version.to_owned(),
        flowseal: flowseal.to_owned(),
        archive: FileHash {
            path: format!("bundle-{version}.tar.gz"),
            sha256: bundle::sha256_hex(&bytes),
        },
        files: hashes,
    };
    manifest.validate()?;
    fs::create_dir_all(out)?;
    fs::write(out.join(&manifest.archive.path), &bytes)?;
    fs::write(
        out.join(UNSIGNED_MANIFEST),
        serde_json::to_string_pretty(&manifest)? + "\n",
    )?;
    println!(
        "Bundle {version}: {} files, flowseal {flowseal}",
        manifest.files.len()
    );
    Ok(manifest)
}

/// The signing key is base64 of a 32-byte ed25519 seed, e.g. `openssl rand -base64 32`.
pub fn signing_key(value: Option<String>) -> Result<SigningKey> {
    let value = value.filter(|v| !v.trim().is_empty()).with_context(|| {
        format!("Bundle signing key is required (нужен ключ подписи): set {KEY_ENV}")
    })?;
    let seed: [u8; 32] = STANDARD
        .decode(value.trim())
        .ok()
        .and_then(|bytes| bytes.try_into().ok())
        .with_context(|| format!("{KEY_ENV} must be base64 of exactly 32 bytes"))?;
    Ok(SigningKey::from_bytes(&seed))
}

pub fn signing_key_from_env() -> Result<SigningKey> {
    signing_key(std::env::var(KEY_ENV).ok())
}

pub fn public_key(key: &SigningKey) -> String {
    STANDARD.encode(key.verifying_key().as_bytes())
}

/// Signs the exact manifest bytes and checks the result the way a client would.
pub fn sign(payload: &[u8], key: &SigningKey) -> Result<SignedManifest> {
    let manifest: Manifest = serde_json::from_slice(payload).context("Manifest is not valid")?;
    manifest.validate()?;
    let envelope = SignedManifest {
        payload: STANDARD.encode(payload),
        signature: STANDARD.encode(key.sign(payload).to_bytes()),
    };
    bundle::verify_signature(&envelope, key.verifying_key().as_bytes())?;
    Ok(envelope)
}

pub fn sign_file(manifest: &Path, out: &Path, key: &SigningKey) -> Result<()> {
    let envelope = sign(&fs::read(manifest)?, key)?;
    fs::write(out, serde_json::to_string_pretty(&envelope)? + "\n")?;
    println!("Signed with public key {}", public_key(key));
    Ok(())
}

/// Signed application update for the rolling `app` release: copies the
/// installer as `Zapret-Desktop-Setup-<version>.exe` (version of this build)
/// and writes `app-manifest.json` into `out`. With `previous` (the published
/// manifest) the version must be newer, or clients would never offer it.
pub fn app_release(
    installer: &Path,
    out: &Path,
    previous: Option<&Path>,
    key: &SigningKey,
) -> Result<()> {
    use zapret_core::app_update::{self, AppManifest, MANIFEST_NAME};
    let version = zapret_core::VERSION;
    if let Some(previous) = previous {
        let envelope: SignedManifest = serde_json::from_slice(&fs::read(previous)?)?;
        let published = app_update::verify(&envelope, key.verifying_key().as_bytes())
            .context("Published app manifest does not verify with this key")?;
        if !bundle::compare_versions(version, &published.version).is_gt() {
            bail!(
                "Version {version} is not newer than the published {}; raise it in Cargo.toml",
                published.version
            );
        }
    }
    let bytes = fs::read(installer).with_context(|| installer.display().to_string())?;
    let manifest = AppManifest::new(version, &bytes)?;
    let payload = serde_json::to_vec_pretty(&manifest)?;
    let envelope = SignedManifest {
        payload: STANDARD.encode(&payload),
        signature: STANDARD.encode(key.sign(&payload).to_bytes()),
    };
    // The same check the service runs.
    app_update::verify(&envelope, key.verifying_key().as_bytes())?.check_installer(&bytes)?;
    fs::create_dir_all(out)?;
    fs::write(out.join(&manifest.installer.path), &bytes)?;
    fs::write(
        out.join(MANIFEST_NAME),
        serde_json::to_string_pretty(&envelope)? + "\n",
    )?;
    println!(
        "{} {version}, signed with {}",
        manifest.installer.path,
        public_key(key)
    );
    Ok(())
}

/// Client-side check, the same code the service runs before switching bundles.
pub fn verify(envelope: &[u8], public_key: &str, archive: &[u8], dest: &Path) -> Result<Manifest> {
    let key: [u8; 32] = STANDARD
        .decode(public_key.trim())
        .ok()
        .and_then(|bytes| bytes.try_into().ok())
        .context("Public key must be base64 of exactly 32 bytes")?;
    bundle::unpack_verified(envelope, &key, archive, dest)
}

pub fn verify_files_cli(manifest: &Path, archive: &Path, public_key: &str) -> Result<()> {
    let dest = std::env::temp_dir().join(format!("zbuild-verify-{}", std::process::id()));
    let result = verify(&fs::read(manifest)?, public_key, &fs::read(archive)?, &dest);
    let _ = fs::remove_dir_all(&dest);
    let manifest = result?;
    let name = archive.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if name != manifest.archive.path {
        bail!("Archive name {name:?} differs from the manifest");
    }
    println!("Verified bundle {}", manifest.version);
    Ok(())
}

/// Markdown summary of what a new prepared runtime changes against a base one.
pub fn changes(base: &Path, new: &Path) -> Result<String> {
    fn strategies(dir: &Path) -> Result<BTreeMap<String, Vec<String>>> {
        let catalog: Vec<Strategy> =
            serde_json::from_slice(&fs::read(dir.join("strategies.json"))?)?;
        Ok(catalog.into_iter().map(|s| (s.id, s.arguments)).collect())
    }
    fn hashes(dir: &Path) -> Result<BTreeMap<String, String>> {
        let mut files = BTreeMap::new();
        bundle::walk(dir, "", &mut files)?;
        files
            .into_iter()
            .filter(|(path, _)| path != "strategies.json")
            .map(|(path, file)| Ok((path, bundle::sha256_hex(&fs::read(file)?))))
            .collect()
    }
    fn diff<V: PartialEq>(
        old: &BTreeMap<String, V>,
        new: &BTreeMap<String, V>,
    ) -> [Vec<String>; 3] {
        let added = new.keys().filter(|k| !old.contains_key(*k)).cloned();
        let removed = old.keys().filter(|k| !new.contains_key(*k)).cloned();
        let changed = new
            .iter()
            .filter(|(k, v)| old.get(*k).is_some_and(|o| o != *v))
            .map(|(k, _)| k.clone());
        [added.collect(), removed.collect(), changed.collect()]
    }
    let mut text = String::new();
    for (title, [added, removed, changed]) in [
        ("Стратегии", diff(&strategies(base)?, &strategies(new)?)),
        ("Файлы", diff(&hashes(base)?, &hashes(new)?)),
    ] {
        text.push_str(&format!("### {title}\n\n"));
        if added.is_empty() && removed.is_empty() && changed.is_empty() {
            text.push_str("Без изменений.\n\n");
            continue;
        }
        for (label, items) in [
            ("Добавлены", added),
            ("Удалены", removed),
            ("Изменены", changed),
        ] {
            if !items.is_empty() {
                text.push_str(&format!("- {label}: `{}`\n", items.join("`, `")));
            }
        }
        text.push('\n');
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test-only seed. Its public key appears nowhere outside these tests.
    const TEST_ONLY_SEED: [u8; 32] = [0x7e; 32];

    fn test_key() -> SigningKey {
        signing_key(Some(STANDARD.encode(TEST_ONLY_SEED))).unwrap()
    }

    fn write(path: &Path, data: &[u8]) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, data).unwrap();
    }

    fn project() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let runtime = dir.path().join("resources/runtime");
        write(
            &runtime.join("windows/strategies.json"),
            br#"[{"id":"general","name":"general","description":"","supported":true,"arguments":["--wf-tcp=443"]}]"#,
        );
        write(&runtime.join("windows/bin/winws.exe"), b"engine");
        write(
            &runtime.join("windows/lists/list-general.txt"),
            b"youtube.com\n",
        );
        write(&runtime.join("windows/lists/list-general-user.txt"), b"");
        write(&runtime.join("licenses/Flowseal-MIT.txt"), b"MIT");
        dir
    }

    fn built() -> (tempfile::TempDir, Manifest, Vec<u8>, Vec<u8>) {
        let project = project();
        let out = project.path().join("out");
        let manifest = bundle(project.path(), "2026.11.11.1200", "1.10.4", &out).unwrap();
        let archive = fs::read(out.join(&manifest.archive.path)).unwrap();
        let envelope = serde_json::to_vec(
            &sign(&fs::read(out.join(UNSIGNED_MANIFEST)).unwrap(), &test_key()).unwrap(),
        )
        .unwrap();
        (project, manifest, archive, envelope)
    }

    #[test]
    fn bundle_is_deterministic_and_keeps_list_placeholders() {
        let project = project();
        let a = bundle(project.path(), "1", "1.10.4", &project.path().join("a")).unwrap();
        let b = bundle(project.path(), "1", "1.10.4", &project.path().join("b")).unwrap();
        assert_eq!(a, b);
        assert_eq!(
            fs::read(project.path().join("a/bundle-1.tar.gz")).unwrap(),
            fs::read(project.path().join("b/bundle-1.tar.gz")).unwrap()
        );
        let paths: Vec<_> = a.files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(
            paths,
            [
                "licenses/Flowseal-MIT.txt",
                "windows/bin/winws.exe",
                "windows/lists/list-general-user.txt",
                "windows/lists/list-general.txt",
                "windows/strategies.json",
            ]
        );
        assert!(bundle(project.path(), "v1", "1.10.4", &project.path().join("c")).is_err());
    }

    #[test]
    fn client_accepts_valid_bundle() {
        let (project, manifest, archive, envelope) = built();
        let dest = project.path().join("client");
        let verified = verify(&envelope, &public_key(&test_key()), &archive, &dest).unwrap();
        assert_eq!(verified, manifest);
        assert_eq!(
            fs::read(dest.join("windows/bin/winws.exe")).unwrap(),
            b"engine"
        );
    }

    #[test]
    fn client_rejects_tampered_manifest() {
        let (project, _, archive, envelope) = built();
        let mut env: SignedManifest = serde_json::from_slice(&envelope).unwrap();
        let payload = String::from_utf8(STANDARD.decode(&env.payload).unwrap()).unwrap();
        env.payload = STANDARD.encode(payload.replace("2026.11.11.1200", "2099.1.1.0"));
        let env = serde_json::to_vec(&env).unwrap();
        let dest = project.path().join("client");
        assert!(verify(&env, &public_key(&test_key()), &archive, &dest).is_err());
    }

    #[test]
    fn client_rejects_replaced_files() {
        let (project, manifest, _, envelope) = built();
        let key = public_key(&test_key());
        let dest = project.path().join("client");
        // A different archive than the signed one.
        let windows = project.path().join("resources/runtime/windows");
        fs::write(windows.join("bin/winws.exe"), b"malware").unwrap();
        let swapped = archive(&bundle_files(project.path()).unwrap()).unwrap();
        assert!(verify(&envelope, &key, &swapped, &dest).is_err());
        // A file replaced after extraction.
        fs::create_dir_all(&dest).unwrap();
        bundle::extract_tar_gz(&swapped, &dest).unwrap();
        assert!(bundle::verify_files(&manifest, &dest).is_err());
    }

    #[test]
    fn client_rejects_foreign_key() {
        let (project, _, archive, envelope) = built();
        let other = SigningKey::from_bytes(&[0x11; 32]);
        let dest = project.path().join("client");
        assert!(verify(&envelope, &public_key(&other), &archive, &dest).is_err());
    }

    #[test]
    fn missing_or_malformed_key_is_an_error() {
        let err = signing_key(None).unwrap_err().to_string();
        assert!(err.contains("нужен ключ подписи"), "{err}");
        assert!(signing_key(Some(" ".into())).is_err());
        assert!(signing_key(Some(STANDARD.encode([1; 16]))).is_err());
    }

    #[test]
    fn change_notes_list_strategies_and_files() {
        let base = project();
        let new = project();
        let windows = new.path().join("resources/runtime/windows");
        write(
            &windows.join("strategies.json"),
            br#"[{"id":"general","name":"general","description":"","supported":true,"arguments":["--wf-tcp=80"]},
                {"id":"general-alt","name":"alt","description":"","supported":true,"arguments":["--wf-udp=443"]}]"#,
        );
        write(&windows.join("lists/list-general.txt"), b"discord.com\n");
        let notes = changes(&base.path().join("resources/runtime/windows"), &windows).unwrap();
        assert!(notes.contains("- Добавлены: `general-alt`"), "{notes}");
        assert!(notes.contains("- Изменены: `general`"), "{notes}");
        assert!(
            notes.contains("- Изменены: `lists/list-general.txt`"),
            "{notes}"
        );
        let same = changes(
            &base.path().join("resources/runtime/windows"),
            &base.path().join("resources/runtime/windows"),
        )
        .unwrap();
        assert_eq!(same.matches("Без изменений.").count(), 2);
    }

    #[test]
    fn app_release_signs_a_newer_installer_only() {
        let dir = tempfile::tempdir().unwrap();
        let setup = dir.path().join("setup.exe");
        fs::write(&setup, b"installer").unwrap();
        let out = dir.path().join("out");
        app_release(&setup, &out, None, &test_key()).unwrap();
        let name = zapret_core::app_update::installer_name(zapret_core::VERSION);
        assert_eq!(fs::read(out.join(&name)).unwrap(), b"installer");
        let manifest = out.join("app-manifest.json");
        let envelope: SignedManifest =
            serde_json::from_slice(&fs::read(&manifest).unwrap()).unwrap();
        let verified =
            zapret_core::app_update::verify(&envelope, test_key().verifying_key().as_bytes())
                .unwrap();
        assert_eq!(verified.version, zapret_core::VERSION);
        // Publishing the same version again would never reach clients.
        let err = app_release(
            &setup,
            &dir.path().join("again"),
            Some(&manifest),
            &test_key(),
        )
        .unwrap_err();
        assert!(format!("{err:#}").contains("not newer"), "{err:#}");
    }
}
