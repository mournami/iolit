//! Signed strategy bundle updates.
//!
//! Bundles live next to the service executable (`<install>/bundles`), which only
//! administrators can write. `%ProgramData%` is not used for them: standard users
//! may add files to its subfolders, and winws loads DLLs from its own folder.
//! User domains stay in service data and are never touched by an update.
use crate::{engine::Engine, operation::OperationManager, selector::Probes, service::Controller};
use anyhow::{Context, Result, bail};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    future::Future,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::{Mutex, Notify};
use zapret_core::{
    BundleStatus, ProbeResult,
    bundle::{self, Manifest, SignedManifest},
};

pub const DEFAULT_SOURCE: &str =
    "https://github.com/mournami/zapret13231/releases/latest/download/";
pub const MANIFEST_NAME: &str = "manifest.json";
const STATE_FILE: &str = "state.json";
const MAX_MANIFEST: usize = 1 << 20;
const MAX_ARCHIVE: usize = 64 << 20;
pub const FIRST_CHECK: Duration = Duration::from_secs(120);
pub const INTERVAL: Duration = Duration::from_secs(6 * 3600);

/// Public key baked in at build time. Without it this build never updates bundles.
pub fn public_key() -> Option<[u8; 32]> {
    decode_key(option_env!("ZAPRET_BUNDLE_PUBLIC_KEY")?)
}

pub fn decode_key(text: &str) -> Option<[u8; 32]> {
    STANDARD.decode(text.trim()).ok()?.try_into().ok()
}

/// `bundle-sources.json` next to the service (admin-only): a JSON array of https
/// base URLs ending in `/`, tried in order. Without it, GitHub Releases.
pub fn load_sources(install: &Path) -> Result<Vec<String>> {
    let file = install.join("bundle-sources.json");
    if !file.try_exists()? {
        return Ok(vec![DEFAULT_SOURCE.into()]);
    }
    validate_sources(serde_json::from_slice(&fs::read(file)?)?)
}

pub fn validate_sources(sources: Vec<String>) -> Result<Vec<String>> {
    if sources.is_empty()
        || sources.iter().any(|s| {
            !s.starts_with("https://") || !s.ends_with('/') || s.contains(char::is_whitespace)
        })
    {
        bail!("Источники обновлений: нужны адреса https://…/");
    }
    Ok(sources)
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Persisted in `<install>/bundles/state.json`. `current` is always the bundle the
/// controller uses; `None` means the installer's bundle.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct State {
    pub current: Option<String>,
    pub previous: Option<String>,
    pub flowseal: Option<String>,
    /// Versions that failed to start or made probes worse; never retried.
    pub rejected: Vec<String>,
    pub last_check: Option<u64>,
    pub last_update: Option<u64>,
    pub error: Option<String>,
}

impl State {
    pub fn status(&self, updates_enabled: bool) -> BundleStatus {
        BundleStatus {
            version: self.current.clone(),
            flowseal: self.flowseal.clone(),
            updates_enabled,
            last_check: self.last_check,
            last_update: self.last_update,
            error: self.error.clone(),
        }
    }
}

pub struct Store {
    dir: PathBuf,
    installed: PathBuf,
}

impl Store {
    /// `dir` holds downloaded bundles; `installed` is the installer's `windows` folder.
    pub fn new(dir: PathBuf, installed: PathBuf) -> Self {
        Self { dir, installed }
    }
    pub fn state(&self) -> State {
        fs::read(self.dir.join(STATE_FILE))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }
    pub fn save(&self, state: &State) -> Result<()> {
        fs::create_dir_all(&self.dir)?;
        let tmp = self.dir.join(format!("{STATE_FILE}.tmp"));
        fs::write(&tmp, serde_json::to_vec_pretty(state)?)?;
        fs::rename(tmp, self.dir.join(STATE_FILE))?;
        Ok(())
    }
    fn bundle_dir(&self, version: &str) -> PathBuf {
        self.dir.join(version)
    }
    fn envelope_path(&self, version: &str) -> PathBuf {
        self.dir.join(format!("{version}.manifest.json"))
    }
    pub fn root_of(&self, version: &str) -> PathBuf {
        self.bundle_dir(version).join("windows")
    }
    /// Re-check an unpacked bundle against its stored signed manifest.
    fn verify_unpacked(&self, version: &str, key: &[u8; 32]) -> Result<Manifest> {
        let envelope: SignedManifest =
            serde_json::from_slice(&fs::read(self.envelope_path(version))?)?;
        let manifest = bundle::verify_signature(&envelope, key)?;
        if manifest.version != version {
            bail!("Версия манифеста не совпадает с папкой набора");
        }
        bundle::verify_tree(&manifest, &self.bundle_dir(version))?;
        Ok(manifest)
    }
    /// Root to start the service with: the current downloaded bundle if it still
    /// verifies, otherwise the installer's bundle (and `current` is cleared).
    pub fn startup(&self, key: Option<&[u8; 32]>) -> (PathBuf, BundleStatus, Option<String>) {
        let mut state = self.state();
        let Some(version) = state.current.clone() else {
            return (self.installed.clone(), state.status(key.is_some()), None);
        };
        let verified = match key {
            Some(key) => self.verify_unpacked(&version, key),
            None => Err(anyhow::anyhow!("в этой сборке нет ключа проверки")),
        };
        match verified {
            Ok(_) => (self.root_of(&version), state.status(key.is_some()), None),
            Err(e) => {
                let warning = format!(
                    "Скачанный набор стратегий {version} не прошёл проверку, используется встроенный: {e:#}"
                );
                state.current = None;
                state.flowseal = None;
                state.error = Some(warning.clone());
                let _ = self.save(&state);
                (
                    self.installed.clone(),
                    state.status(key.is_some()),
                    Some(warning),
                )
            }
        }
    }
    /// Remove every bundle except `keep`. Failures are left for the next update.
    fn prune(&self, keep: &[&str]) {
        let Ok(entries) = fs::read_dir(&self.dir) else {
            return;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let version = name.strip_suffix(".manifest.json").unwrap_or(&name);
            if name == STATE_FILE || keep.contains(&version) {
                continue;
            }
            let path = entry.path();
            let _ = if path.is_dir() {
                fs::remove_dir_all(path)
            } else {
                fs::remove_file(path)
            };
        }
    }
}

pub trait Fetch: Send + Sync {
    fn get(&self, url: &str, limit: usize) -> impl Future<Output = Result<Vec<u8>>> + Send;
}

pub struct HttpFetch(reqwest::Client);
impl HttpFetch {
    pub fn new() -> Result<Self> {
        Ok(Self(
            reqwest::Client::builder()
                .user_agent(concat!("ZapretDesktop/", env!("CARGO_PKG_VERSION")))
                .https_only(true)
                .connect_timeout(Duration::from_secs(15))
                .timeout(Duration::from_secs(120))
                .build()?,
        ))
    }
}
impl Fetch for HttpFetch {
    async fn get(&self, url: &str, limit: usize) -> Result<Vec<u8>> {
        let mut response = self.0.get(url).send().await?.error_for_status()?;
        if response.content_length().is_some_and(|n| n > limit as u64) {
            bail!("Файл слишком большой");
        }
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await? {
            body.extend_from_slice(&chunk);
            if body.len() > limit {
                bail!("Файл слишком большой");
            }
        }
        Ok(body)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    UpToDate,
    /// A selection or command holds the gate; the downloaded bundle waits.
    Busy,
    Updated(String),
    RolledBack(String),
}

pub struct Updater<E: Engine, F: Fetch, P: Probes> {
    controller: Arc<Mutex<Controller<E>>>,
    operations: Arc<OperationManager>,
    store: Store,
    fetch: F,
    probes: Arc<P>,
    key: [u8; 32],
    sources: Vec<String>,
    /// Wakes the loop early, e.g. from Doctor.
    pub trigger: Notify,
}

fn passed(results: &[ProbeResult]) -> usize {
    results.iter().filter(|r| r.ok).count()
}

impl<E: Engine, F: Fetch, P: Probes> Updater<E, F, P> {
    pub fn new(
        controller: Arc<Mutex<Controller<E>>>,
        operations: Arc<OperationManager>,
        store: Store,
        fetch: F,
        probes: Arc<P>,
        key: [u8; 32],
        sources: Vec<String>,
    ) -> Self {
        Self {
            controller,
            operations,
            store,
            fetch,
            probes,
            key,
            sources,
            trigger: Notify::new(),
        }
    }

    /// First check shortly after start, then every [`INTERVAL`] or on trigger.
    pub async fn run(self: Arc<Self>) {
        tokio::select! {
            _ = tokio::time::sleep(FIRST_CHECK) => {}
            _ = self.trigger.notified() => {}
        }
        loop {
            let _ = self.check().await;
            tokio::select! {
                _ = tokio::time::sleep(INTERVAL) => {}
                _ = self.trigger.notified() => {}
            }
        }
    }

    /// One update attempt; the result is recorded in the state and the snapshot.
    pub async fn check(&self) -> Result<Outcome> {
        let result = self.check_inner().await;
        let mut state = self.store.state();
        state.last_check = Some(now());
        state.error = match &result {
            Ok(Outcome::RolledBack(v)) => Some(format!(
                "Набор {v} ухудшил проверки и был отменён, используется предыдущий"
            )),
            Ok(_) => None,
            Err(e) => Some(format!("{e:#}")),
        };
        let saved = self.store.save(&state);
        self.controller.lock().await.bundle = state.status(true);
        saved?;
        result
    }

    async fn fetch_manifest(&self) -> Result<(Vec<u8>, Manifest, &str)> {
        let mut errors = Vec::new();
        for source in &self.sources {
            let attempt = async {
                let bytes = self
                    .fetch
                    .get(&format!("{source}{MANIFEST_NAME}"), MAX_MANIFEST)
                    .await?;
                let envelope: SignedManifest = serde_json::from_slice(&bytes)?;
                let manifest = bundle::verify_signature(&envelope, &self.key)?;
                Ok::<_, anyhow::Error>((bytes, manifest))
            };
            match attempt.await {
                Ok((bytes, manifest)) => return Ok((bytes, manifest, source)),
                Err(e) => errors.push(format!("{source}: {e:#}")),
            }
        }
        bail!("Не удалось получить манифест набора: {}", errors.join("; "))
    }

    fn reject(&self, version: &str) -> Result<()> {
        let mut state = self.store.state();
        if !state.rejected.iter().any(|v| v == version) {
            state.rejected.push(version.into());
        }
        self.store.save(&state)
    }

    async fn check_inner(&self) -> Result<Outcome> {
        let (envelope, manifest, source) = self.fetch_manifest().await?;
        let version = manifest.version.clone();
        let state = self.store.state();
        let newer = state
            .current
            .as_deref()
            .is_none_or(|current| bundle::compare_versions(&version, current).is_gt());
        if !newer || state.rejected.contains(&version) {
            return Ok(Outcome::UpToDate);
        }

        // Download and verify without blocking commands; reuse a verified copy.
        if self.store.verify_unpacked(&version, &self.key).is_err() {
            let archive = self
                .fetch
                .get(&format!("{source}{}", manifest.archive.path), MAX_ARCHIVE)
                .await
                .context("Не удалось скачать набор")?;
            fs::create_dir_all(&self.store.dir)?;
            let staging = self.store.dir.join(format!(".staging-{version}"));
            bundle::unpack_verified(&envelope, &self.key, &archive, &staging)?;
            let dest = self.store.bundle_dir(&version);
            if dest.exists() {
                fs::remove_dir_all(&dest)?;
            }
            fs::rename(&staging, &dest)?;
            fs::write(self.store.envelope_path(&version), &envelope)?;
        }

        let Ok(_gate) = self.operations.gate.clone().try_lock_owned() else {
            return Ok(Outcome::Busy);
        };
        let (old_root, running) = {
            let controller = &mut *self.controller.lock().await;
            (
                controller.root().to_path_buf(),
                controller.status()?.running,
            )
        };
        // Probes run without the controller lock, as in selection.
        let before = if running {
            Some(self.probes.run().await)
        } else {
            None
        };
        if let Err(e) = self
            .controller
            .lock()
            .await
            .switch_root(self.store.root_of(&version))
            .await
        {
            self.reject(&version)?;
            return Err(e);
        }
        if let Some(before) = before {
            let after = self.probes.run().await;
            if passed(&after) < passed(&before) {
                self.controller
                    .lock()
                    .await
                    .switch_root(old_root)
                    .await
                    .context("Не удалось вернуть предыдущий набор стратегий")?;
                self.reject(&version)?;
                return Ok(Outcome::RolledBack(version));
            }
        }

        let mut state = self.store.state();
        state.previous = state.current.take();
        state.current = Some(version.clone());
        state.flowseal = Some(manifest.flowseal);
        state.last_update = Some(now());
        self.store.save(&state)?;
        let keep: Vec<&str> = [state.current.as_deref(), state.previous.as_deref()]
            .into_iter()
            .flatten()
            .collect();
        self.store.prune(&keep);
        Ok(Outcome::Updated(version))
    }
}
