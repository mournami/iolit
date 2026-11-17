//! Signed application updates: the installer of a newer Iolit.
//!
//! Published in the rolling GitHub release `app` as `app-manifest.json` (a
//! [`SignedManifest`] whose payload is an [`AppManifest`]) next to the installer
//! it names. Signed with the bundle key; `product` and the different required
//! fields keep a bundle manifest from passing as an app manifest and back.
use crate::bundle::{self, FileHash, SignedManifest};
use anyhow::{Context, Result, bail};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const MANIFEST_NAME: &str = "app-manifest.json";
pub const SCHEMA: u32 = 1;
pub const PRODUCT: &str = "zapret-desktop-windows";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AppManifest {
    pub schema: u32,
    /// Always [`PRODUCT`].
    pub product: String,
    /// Application version, dot-separated numbers (`0.2.0`).
    pub version: String,
    /// `Iolit-Setup-<version>.exe` and its hash.
    pub installer: FileHash,
}

/// File name of the installer for `version` in the release.
pub fn installer_name(version: &str) -> String {
    format!("Iolit-Setup-{version}.exe")
}

fn valid_version(version: &str) -> bool {
    let parts: Vec<_> = version.split('.').collect();
    (1..=4).contains(&parts.len())
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.len() <= 9 && p.bytes().all(|b| b.is_ascii_digit()))
}

impl AppManifest {
    pub fn new(version: &str, installer: &[u8]) -> Result<Self> {
        let manifest = Self {
            schema: SCHEMA,
            product: PRODUCT.into(),
            version: version.into(),
            installer: FileHash {
                path: installer_name(version),
                sha256: bundle::sha256_hex(installer),
            },
        };
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn validate(&self) -> Result<()> {
        if self.schema != SCHEMA || self.product != PRODUCT {
            bail!("Это не манифест обновления Iolit для Windows");
        }
        if !valid_version(&self.version) {
            bail!("Некорректная версия приложения {:?}", self.version);
        }
        if self.installer.path != installer_name(&self.version) {
            bail!("Неожиданное имя установщика {:?}", self.installer.path);
        }
        if self.installer.sha256.len() != 64
            || !self
                .installer
                .sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            bail!("Некорректная контрольная сумма установщика");
        }
        Ok(())
    }

    /// Newer than the running application.
    pub fn newer_than(&self, current: &str) -> bool {
        bundle::compare_versions(&self.version, current).is_gt()
    }

    pub fn check_installer(&self, bytes: &[u8]) -> Result<()> {
        if bundle::sha256_hex(bytes) != self.installer.sha256 {
            bail!("Контрольная сумма установщика не совпала");
        }
        Ok(())
    }
}

pub fn verify(envelope: &SignedManifest, key: &[u8; 32]) -> Result<AppManifest> {
    let payload = bundle::signed_payload(envelope, key)
        .context("Подпись обновления приложения недействительна")?;
    let manifest: AppManifest =
        serde_json::from_slice(&payload).context("Манифест обновления приложения повреждён")?;
    manifest.validate()?;
    Ok(manifest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::{Engine, engine::general_purpose::STANDARD};
    use ed25519_dalek::{Signer, SigningKey};

    fn sign(payload: &[u8], key: &SigningKey) -> SignedManifest {
        SignedManifest {
            payload: STANDARD.encode(payload),
            signature: STANDARD.encode(key.sign(payload).to_bytes()),
        }
    }

    #[test]
    fn signed_manifest_verifies_and_checks_the_installer() {
        let key = SigningKey::from_bytes(&[7; 32]);
        let public = key.verifying_key().to_bytes();
        let manifest = AppManifest::new("0.2.0", b"setup").unwrap();
        let envelope = sign(&serde_json::to_vec(&manifest).unwrap(), &key);
        let verified = verify(&envelope, &public).unwrap();
        assert_eq!(verified, manifest);
        assert!(verified.newer_than("0.1.0"));
        assert!(!verified.newer_than("0.2.0"));
        assert!(!verified.newer_than("0.10.0"));
        assert!(verified.check_installer(b"setup").is_ok());
        assert!(verified.check_installer(b"evil").is_err());

        let other = SigningKey::from_bytes(&[8; 32]);
        assert!(
            verify(
                &sign(&serde_json::to_vec(&manifest).unwrap(), &other),
                &public
            )
            .is_err()
        );
    }

    #[test]
    fn bundle_manifests_and_odd_names_are_refused() {
        let key = SigningKey::from_bytes(&[7; 32]);
        let public = key.verifying_key().to_bytes();
        let bundle = br#"{"schema":1,"version":"2026.11.14.1200","flowseal":"1.10.5","archive":{"path":"a","sha256":"00"},"files":[]}"#;
        assert!(verify(&sign(bundle, &key), &public).is_err());
        let mut m = AppManifest::new("0.2.0", b"setup").unwrap();
        m.installer.path = "../evil.exe".into();
        assert!(m.validate().is_err());
        for bad in ["", "1..2", "v1", "1.2.3.4.5", "1.-2"] {
            assert!(AppManifest::new(bad, b"x").is_err(), "{bad}");
        }
    }
}
