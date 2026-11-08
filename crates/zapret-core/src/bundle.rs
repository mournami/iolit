//! Signed manifests are verified before any downloaded file can be used.
use anyhow::{Context, Result, bail};
use base64::{Engine, engine::general_purpose::STANDARD};
use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    path::{Component, Path},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileHash {
    pub path: String,
    pub sha256: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub schema: u32,
    pub version: String,
    pub files: Vec<FileHash>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedManifest {
    pub payload: String,
    pub signature: String,
}

pub fn verify_signature(envelope: &SignedManifest, key: &[u8; 32]) -> Result<Manifest> {
    let payload = STANDARD.decode(&envelope.payload)?;
    let signature = Signature::from_slice(&STANDARD.decode(&envelope.signature)?)?;
    VerifyingKey::from_bytes(key)?
        .verify_strict(&payload, &signature)
        .context("Подпись набора недействительна")?;
    let manifest: Manifest = serde_json::from_slice(&payload)?;
    if manifest.schema != 1 || manifest.files.is_empty() {
        bail!("Неподдерживаемый манифест");
    }
    Ok(manifest)
}

pub fn verify_files(manifest: &Manifest, root: &Path) -> Result<()> {
    let root = root.canonicalize()?;
    let mut seen = HashSet::new();
    for f in &manifest.files {
        if !seen.insert(&f.path)
            || Path::new(&f.path)
                .components()
                .any(|c| !matches!(c, Component::Normal(_)))
        {
            bail!("Недопустимый путь в манифесте");
        }
        let file = root.join(&f.path).canonicalize()?;
        if !file.starts_with(&root) || !file.is_file() {
            bail!("Файл за пределами набора");
        }
        let expected = hex::decode(&f.sha256)?;
        if expected.len() != 32 || Sha256::digest(std::fs::read(&file)?).as_slice() != expected {
            bail!("Контрольная сумма {} не совпала", f.path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    #[test]
    fn tampered_manifest_is_rejected() {
        let key = SigningKey::from_bytes(&[7; 32]);
        let payload = br#"{"schema":1,"version":"1","files":[{"path":"engine","sha256":"00"}]}"#;
        let mut env = SignedManifest {
            payload: STANDARD.encode(payload),
            signature: STANDARD.encode(key.sign(payload).to_bytes()),
        };
        assert!(verify_signature(&env, key.verifying_key().as_bytes()).is_ok());
        env.payload = STANDARD.encode(b"tampered");
        assert!(verify_signature(&env, key.verifying_key().as_bytes()).is_err());
        env.signature = STANDARD.encode([0; 64]);
        assert!(verify_signature(&env, key.verifying_key().as_bytes()).is_err());
    }
}
