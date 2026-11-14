//! Application updates (owner's decision, 14.11.2026). The service looks for
//! a newer signed installer in the rolling GitHub release `app`, downloads and
//! verifies it on its own, and runs it only when the user presses «Обновить»
//! (`DaemonCommand::UpdateApp`). The installer replaces the files, restarts the
//! service, puts the previous version back if the new service does not start,
//! and opens the window again.
//!
//! The installer waits under `<install>/updates`, which only administrators can
//! write, and is checked against the signed manifest again right before it runs.
use crate::{operation::OperationManager, updater::Fetch};
use anyhow::{Context, Result, bail};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use zapret_core::{
    app_update::{self, AppManifest, MANIFEST_NAME},
    bundle::SignedManifest,
};

pub const DEFAULT_SOURCE: &str = "https://github.com/mournami/zapret13231/releases/download/app/";
pub const DIR: &str = "updates";
const INSTALLER: &str = "Zapret-Desktop-Setup.exe";
const MAX_MANIFEST: usize = 1 << 20;
const MAX_INSTALLER: usize = 64 << 20;
pub const FIRST_CHECK: Duration = Duration::from_secs(180);
pub const INTERVAL: Duration = Duration::from_secs(6 * 3600);
/// A successful installer stops this service well before that.
pub const INSTALL_TIMEOUT: Duration = Duration::from_secs(180);

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Starts the installer so that it outlives this service.
pub type Launcher = fn(&Path) -> Result<()>;

pub struct AppUpdater<F: Fetch> {
    operations: Arc<OperationManager>,
    dir: PathBuf,
    fetch: F,
    key: [u8; 32],
    sources: Vec<String>,
    current: String,
    launch: Launcher,
    install_timeout: Duration,
}

/// The waiting installer, if its stored manifest verifies, names a version
/// newer than `current` and matches the file.
pub fn verified_installer(
    dir: &Path,
    key: &[u8; 32],
    current: &str,
) -> Result<(PathBuf, AppManifest)> {
    let envelope: SignedManifest = serde_json::from_slice(
        &fs::read(dir.join(MANIFEST_NAME)).context("Нет скачанного обновления")?,
    )?;
    let manifest = app_update::verify(&envelope, key)?;
    if !manifest.newer_than(current) {
        bail!("Скачанное обновление не новее установленной версии");
    }
    let path = dir.join(INSTALLER);
    manifest.check_installer(&fs::read(&path)?)?;
    Ok((path, manifest))
}

impl<F: Fetch> AppUpdater<F> {
    pub fn new(
        operations: Arc<OperationManager>,
        dir: PathBuf,
        fetch: F,
        key: [u8; 32],
        sources: Vec<String>,
        current: &str,
    ) -> Self {
        Self {
            operations,
            dir,
            fetch,
            key,
            sources,
            current: current.into(),
            launch,
            install_timeout: INSTALL_TIMEOUT,
        }
    }

    /// For tests: another launcher and a short wait for the service to stop.
    pub fn with_launcher(mut self, launch: Launcher, install_timeout: Duration) -> Self {
        self.launch = launch;
        self.install_timeout = install_timeout;
        self
    }

    fn clear(&self) {
        let _ = fs::remove_file(self.dir.join(MANIFEST_NAME));
        let _ = fs::remove_file(self.dir.join(INSTALLER));
    }

    /// Offers an installer downloaded before the restart, or removes files of
    /// a version that is installed now or no longer verifies.
    pub async fn startup(&self) {
        let available = match verified_installer(&self.dir, &self.key, &self.current) {
            Ok((_, manifest)) => Some(manifest.version),
            Err(_) => {
                self.clear();
                None
            }
        };
        self.operations.app.lock().await.available = available;
    }

    pub async fn run(self: Arc<Self>) {
        self.startup().await;
        let mut next = tokio::time::Instant::now() + FIRST_CHECK;
        loop {
            tokio::select! {
                _ = tokio::time::sleep_until(next) => {
                    let _ = self.check().await;
                    next = tokio::time::Instant::now() + INTERVAL;
                }
                _ = self.operations.install_app.notified() => {
                    let _ = self.install().await;
                }
            }
        }
    }

    /// Looks for a newer version and downloads it. A network error keeps a
    /// download that is already waiting.
    pub async fn check(&self) -> Result<Option<String>> {
        let result = self.check_inner().await;
        let mut app = self.operations.app.lock().await;
        app.last_check = Some(now());
        match &result {
            Ok(available) => {
                app.available = available.clone();
                app.error = None;
            }
            Err(e) => app.error = Some(format!("{e:#}")),
        }
        result
    }

    async fn fetch_manifest(&self) -> Result<(Vec<u8>, AppManifest, &str)> {
        let mut errors = Vec::new();
        for source in &self.sources {
            let attempt = async {
                let bytes = self
                    .fetch
                    .get(&format!("{source}{MANIFEST_NAME}"), MAX_MANIFEST)
                    .await?;
                let envelope: SignedManifest = serde_json::from_slice(&bytes)?;
                Ok::<_, anyhow::Error>((bytes.clone(), app_update::verify(&envelope, &self.key)?))
            };
            match attempt.await {
                Ok((bytes, manifest)) => return Ok((bytes, manifest, source)),
                Err(e) => errors.push(format!("{source}: {e:#}")),
            }
        }
        bail!(
            "Не удалось проверить обновление приложения: {}",
            errors.join("; ")
        )
    }

    async fn check_inner(&self) -> Result<Option<String>> {
        let (envelope, manifest, source) = self.fetch_manifest().await?;
        if !manifest.newer_than(&self.current) {
            self.clear();
            return Ok(None);
        }
        if verified_installer(&self.dir, &self.key, &self.current)
            .is_ok_and(|(_, waiting)| waiting == manifest)
        {
            return Ok(Some(manifest.version));
        }
        let installer = self
            .fetch
            .get(
                &format!("{source}{}", manifest.installer.path),
                MAX_INSTALLER,
            )
            .await
            .context("Не удалось скачать обновление приложения")?;
        manifest.check_installer(&installer)?;
        fs::create_dir_all(&self.dir)?;
        // The manifest goes last: a half-written pair never verifies.
        self.clear();
        let tmp = self.dir.join(format!("{INSTALLER}.tmp"));
        fs::write(&tmp, &installer)?;
        fs::rename(&tmp, self.dir.join(INSTALLER))?;
        fs::write(self.dir.join(MANIFEST_NAME), &envelope)?;
        Ok(Some(manifest.version))
    }

    /// Runs the waiting installer. Holds the operation gate so no selection
    /// starts while the service is being replaced.
    pub async fn install(&self) -> Result<()> {
        let result = self.install_inner().await;
        let mut app = self.operations.app.lock().await;
        app.installing = false;
        if let Err(e) = &result {
            app.error = Some(format!("{e:#}"));
        }
        result
    }

    async fn install_inner(&self) -> Result<()> {
        let Ok(_gate) = self.operations.gate.clone().try_lock_owned() else {
            bail!("Дождитесь завершения проверки или отмените её");
        };
        let (installer, _) = match verified_installer(&self.dir, &self.key, &self.current) {
            Ok(found) => found,
            Err(e) => {
                self.clear();
                self.operations.app.lock().await.available = None;
                return Err(e.context("Скачанное обновление не прошло проверку, скачаем заново"));
            }
        };
        {
            let mut app = self.operations.app.lock().await;
            app.installing = true;
            app.error = None;
        }
        (self.launch)(&installer).context("Не удалось запустить установщик")?;
        // A working installer stops this service; still running means it gave
        // up before changing anything (its log is in the installer's output).
        tokio::time::sleep(self.install_timeout).await;
        bail!("Обновление не установилось. Попробуйте ещё раз или скачайте установщик с GitHub.")
    }
}

/// Starts the installer detached from the service, so stopping the service
/// does not end it.
#[cfg(windows)]
fn launch(installer: &Path) -> Result<()> {
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
    let command = |flags: u32| {
        Command::new(installer)
            .args(["--quiet", "--update"])
            .current_dir(installer.parent().unwrap_or(Path::new(".")))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(flags)
            .spawn()
    };
    let flags = DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP;
    // Breaking away fails inside a job that forbids it; then start it plainly.
    command(flags | CREATE_BREAKAWAY_FROM_JOB)
        .or_else(|_| command(flags))
        .map(drop)
        .map_err(Into::into)
}

#[cfg(not(windows))]
fn launch(_: &Path) -> Result<()> {
    bail!("Обновление приложения пока есть только для Windows")
}
