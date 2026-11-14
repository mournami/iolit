//! What the checks test and which strategy profiles each one depends on.
//! Ships in the bundle as `windows/targets.json` (signed with the rest), so a
//! new service or a moved check host arrives with a strategy update. The
//! copy in `resources/targets.json` is built in as the fallback for bundles
//! without the file.
use crate::{composite::Slot, strategy};
use anyhow::{Context, Result, bail};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, path::Path};

pub const FILE: &str = "targets.json";
const BUILT_IN: &str = include_str!("../../../resources/targets.json");
const MAX_TARGETS: usize = 16;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TargetList {
    /// Format version; only 1 exists.
    pub schema: u32,
    pub targets: Vec<Target>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Target {
    /// Stable id, also used by the interface (`youtube`, `discord`, ...).
    pub id: String,
    /// Shown to the user.
    pub name: String,
    pub check: Check,
    /// Strategy profiles this service depends on; empty: needs no bypass
    /// (the plain internet check).
    #[serde(default)]
    pub slots: Vec<Slot>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Check {
    /// TLS handshake and the first 64 KB of an HTTPS page.
    Https { url: String },
    /// A STUN binding request over UDP.
    Stun { host: String, port: u16 },
}

impl TargetList {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let list: Self = serde_json::from_slice(bytes).context("targets.json повреждён")?;
        list.validate()?;
        Ok(list)
    }

    /// Fixed public hosts only: checks never take user input, and nothing
    /// here can point the service at a local address or another scheme.
    pub fn validate(&self) -> Result<()> {
        if self.schema != 1 {
            bail!("Неизвестная версия targets.json: {}", self.schema);
        }
        if self.targets.is_empty() || self.targets.len() > MAX_TARGETS {
            bail!("В targets.json должно быть от 1 до {MAX_TARGETS} проверок");
        }
        let mut ids = HashSet::new();
        for t in &self.targets {
            if !strategy::valid_id(&t.id) || !ids.insert(&t.id) {
                bail!("Некорректный или повторный id проверки: {}", t.id);
            }
            if t.name.trim().is_empty() || t.name.chars().count() > 40 {
                bail!("Некорректное имя проверки {}", t.id);
            }
            match &t.check {
                Check::Https { url } => {
                    let parsed = url::Url::parse(url).context("Некорректный адрес проверки")?;
                    if parsed.scheme() != "https"
                        || !parsed.username().is_empty()
                        || parsed.password().is_some()
                        || parsed.port().is_some()
                    {
                        bail!("Проверка {} должна идти на https без порта и логина", t.id);
                    }
                    public_host(parsed.host_str().unwrap_or_default(), &t.id)?;
                }
                Check::Stun { host, port } => {
                    public_host(host, &t.id)?;
                    if *port == 0 {
                        bail!("Нулевой порт в проверке {}", t.id);
                    }
                }
            }
        }
        Ok(())
    }

    /// The bundle's list, or the built-in one if the bundle has none.
    pub fn load(root: &Path) -> Result<Self> {
        match std::fs::read(root.join(FILE)) {
            Ok(bytes) => Self::parse(&bytes),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::built_in()),
            Err(e) => Err(e.into()),
        }
    }

    pub fn built_in() -> Self {
        Self::parse(BUILT_IN.as_bytes()).expect("resources/targets.json is valid")
    }

    pub fn get(&self, id: &str) -> Option<&Target> {
        self.targets.iter().find(|t| t.id == id)
    }

    pub fn slots(&self, id: &str) -> &[Slot] {
        self.get(id).map_or(&[], |t| &t.slots)
    }
}

/// A domain name (not an IP address or `localhost`).
fn public_host(host: &str, id: &str) -> Result<()> {
    let domain = strategy::normalize_domain(host)
        .with_context(|| format!("Некорректный адрес в проверке {id}"))?;
    if domain == "localhost" || domain.ends_with(".localhost") || domain.ends_with(".local") {
        bail!("Локальный адрес в проверке {id}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_list_has_the_usual_checks() {
        let list = TargetList::built_in();
        let ids: Vec<_> = list.targets.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, ["internet", "youtube", "discord", "cloudflare", "udp"]);
        assert_eq!(list.slots("youtube"), [Slot::Google]);
        assert!(list.slots("internet").is_empty());
        assert!(list.slots("missing").is_empty());
    }

    #[test]
    fn checks_stay_on_public_https_and_stun() {
        let good = TargetList::built_in();
        let with = |check: Check| {
            let mut list = good.clone();
            list.targets[0].check = check;
            list.validate()
        };
        let https = |url: &str| Check::Https { url: url.into() };
        for bad in [
            "http://example.com/",
            "https://127.0.0.1/",
            "https://localhost/",
            "https://user@example.com/",
            "https://example.com:8443/",
            "file:///etc/passwd",
        ] {
            assert!(with(https(bad)).is_err(), "{bad}");
        }
        assert!(with(https("https://example.com/path?q=1")).is_ok());
        assert!(
            with(Check::Stun {
                host: "10.0.0.1".into(),
                port: 3478
            })
            .is_err()
        );
        let mut twice = good.clone();
        twice.targets[1].id = "internet".into();
        assert!(twice.validate().is_err());
        assert!(TargetList::parse(br#"{"schema":2,"targets":[]}"#).is_err());
        assert!(
            TargetList::parse(br#"{"schema":1,"targets":[],"extra":1}"#).is_err(),
            "unknown fields are refused"
        );
    }

    #[test]
    fn bundle_without_the_file_uses_the_built_in_list() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            TargetList::load(dir.path()).unwrap(),
            TargetList::built_in()
        );
        std::fs::write(dir.path().join(FILE), b"{").unwrap();
        assert!(TargetList::load(dir.path()).is_err());
    }
}
