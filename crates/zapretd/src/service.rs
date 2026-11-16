//! Transport-independent daemon state and fixed command dispatch.
use crate::{
    engine::Engine,
    logging::log_tail,
    operation::{Mode, OperationManager},
    selector::{Backend, NetworkProbes},
};
use anyhow::{Context, Result, bail};
use std::{path::PathBuf, sync::Arc};
use tokio::sync::Mutex;
use zapret_core::{VERSION, api::*, strategy, targets::TargetList};

/// The writable user list; strategies get it in place of `list-general-user.txt`.
const USER_DOMAINS: &str = "user-domains.txt";
/// Strategy assembled by selection from several bundle strategies.
const COMPOSITE: &str = "composite.json";

pub struct Controller<E: Engine> {
    pub(crate) root: PathBuf,
    pub(crate) data: PathBuf,
    catalog: Vec<Strategy>,
    active: String,
    engine: E,
    /// Checks of the active bundle, see `zapret_core::targets`.
    pub(crate) targets: TargetList,
    /// Why the bundle's checks were not loaded, shown until the bundle changes.
    targets_warning: Option<String>,
    pub(crate) warning: Option<String>,
    pub bundle: BundleStatus,
}
impl<E: Engine> Controller<E> {
    pub fn new(root: PathBuf, data: PathBuf, engine: E) -> Result<Self> {
        std::fs::create_dir_all(&data)?;
        let catalog = with_composite(strategy::load_catalog(&root)?, &data);
        let saved = std::fs::read_to_string(data.join("strategy")).unwrap_or_default();
        let active = catalog
            .iter()
            .find(|s| s.id == saved.trim())
            .or_else(|| catalog.first())
            .context("Нет стратегий")?
            .id
            .clone();
        let (targets, targets_warning) = load_targets(&root);
        Ok(Self {
            root,
            data,
            catalog,
            active,
            engine,
            targets,
            targets_warning,
            warning: None,
            bundle: BundleStatus::default(),
        })
    }
    pub fn root(&self) -> &std::path::Path {
        &self.root
    }
    /// Use the strategy bundle at `root`. The catalog is loaded before anything is
    /// stopped; if the engine was running and fails to start from the new bundle,
    /// the previous bundle, strategy and engine state are restored.
    pub(crate) async fn switch_root(&mut self, root: PathBuf) -> Result<()> {
        let catalog = with_composite(strategy::load_catalog(&root)?, &self.data);
        let active = catalog
            .iter()
            .find(|s| s.id == self.active)
            .or_else(|| catalog.first())
            .context("Нет стратегий")?
            .id
            .clone();
        let running = self.engine.is_ready()?.running;
        self.stop()?;
        let old_root = std::mem::replace(&mut self.root, root);
        let old_catalog = std::mem::replace(&mut self.catalog, catalog);
        let old_active = std::mem::replace(&mut self.active, active);
        let (targets, targets_warning) = load_targets(&self.root);
        let old_targets = std::mem::replace(&mut self.targets, targets);
        let old_targets_warning = std::mem::replace(&mut self.targets_warning, targets_warning);
        if running && let Err(e) = self.start().await {
            let _ = self.stop();
            self.root = old_root;
            self.catalog = old_catalog;
            self.active = old_active;
            self.targets = old_targets;
            self.targets_warning = old_targets_warning;
            let _ = self.start().await;
            return Err(e.context("Новый набор стратегий не запустился"));
        }
        if self.active != old_active {
            std::fs::write(self.data.join("strategy"), &self.active)?;
        }
        Ok(())
    }
    pub(crate) fn status(&mut self) -> Result<ServiceStatus> {
        let state = self.engine.is_ready()?;
        let mut warnings = state.warnings;
        warnings.extend(self.warning.clone());
        warnings.extend(self.targets_warning.clone());
        Ok(ServiceStatus {
            installed: true,
            running: state.running,
            platform: E::PLATFORM.into(),
            engine: E::NAME.into(),
            strategy: self.active.clone(),
            uptime_secs: state.uptime_secs,
            capabilities: E::CAPABILITIES,
            warnings,
            packets: 0,
            modified_packets: 0,
            errors: 0,
            secure_dns: crate::secure_dns::state(&self.data),
        })
    }
    pub(crate) fn stop(&mut self) -> Result<()> {
        self.engine.stop()
    }
    pub(crate) async fn start(&mut self) -> Result<()> {
        let strategy = self
            .catalog
            .iter()
            .find(|s| s.id == self.active)
            .context("Стратегия не найдена")?;
        self.engine.start(strategy, &self.root, &self.data).await?;
        self.warning = None;
        Ok(())
    }
    fn disable(&mut self) -> Result<()> {
        let persisted = std::fs::write(self.data.join("enabled"), b"0");
        self.stop()?;
        persisted.context("Движок остановлен, но сохранить настройку не удалось")
    }
    async fn enable(&mut self) -> Result<()> {
        self.start().await?;
        std::fs::write(self.data.join("enabled"), b"1")?;
        Ok(())
    }
    async fn set_strategy(&mut self, id: &str) -> Result<()> {
        if !strategy::valid_id(id) || !self.catalog.iter().any(|s| s.id == id) {
            bail!("Неизвестная стратегия");
        }
        let old = self.active.clone();
        let running = self.status()?.running;
        self.stop()?;
        self.active = id.into();
        if running && let Err(e) = self.start().await {
            self.active = old;
            let _ = self.start().await;
            return Err(e);
        }
        std::fs::write(self.data.join("strategy"), id)?;
        Ok(())
    }
    /// Stored user domains, one per line, in the form winws matches.
    fn stored_domains(&self) -> Result<Vec<String>> {
        match std::fs::read_to_string(self.data.join(USER_DOMAINS)) {
            Ok(text) => Ok(text
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(str::to_owned)
                .collect()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(vec![]),
            Err(e) => Err(e.into()),
        }
    }
    fn domains(&self) -> Result<Vec<String>> {
        Ok(self
            .stored_domains()?
            .iter()
            .map(|d| strategy::display_domain(d))
            .collect())
    }
    /// winws re-reads a hostlist when its size or time changes, so the running
    /// engine is not restarted: a restart dropped bypass for a second or two.
    /// The file is replaced in one rename so winws never reads half of it.
    fn write_domains(&self, domains: &[String]) -> Result<()> {
        let file = self.data.join(USER_DOMAINS);
        let fresh = self.data.join(format!("{USER_DOMAINS}.new"));
        let mut text = domains.join("\n");
        if !text.is_empty() {
            text.push('\n');
        }
        std::fs::write(&fresh, text)?;
        std::fs::rename(&fresh, &file)?;
        Ok(())
    }
    fn add_domain(&mut self, domain: &str) -> Result<()> {
        let domain = strategy::normalize_domain(domain)?;
        let mut domains = self.stored_domains()?;
        if !domains.contains(&domain) {
            domains.push(domain);
            self.write_domains(&domains)?;
        }
        Ok(())
    }
    fn remove_domain(&mut self, domain: &str) -> Result<()> {
        let domain = strategy::normalize_domain(domain)?;
        let mut domains = self.stored_domains()?;
        let count = domains.len();
        domains.retain(|d| *d != domain);
        if domains.len() == count {
            bail!(
                "Домена {} нет в вашем списке",
                strategy::display_domain(&domain)
            );
        }
        self.write_domains(&domains)
    }
    /// Replace (or with `None` remove) the composite strategy in the catalog
    /// and on disk. The active strategy is not changed here.
    fn set_composite(&mut self, composite: Option<Strategy>) -> Result<()> {
        let file = self.data.join(COMPOSITE);
        self.catalog.retain(|s| s.id != zapret_core::composite::ID);
        match composite {
            Some(composite) => {
                strategy::validate(&composite)?;
                let fresh = self.data.join(format!("{COMPOSITE}.new"));
                std::fs::write(&fresh, serde_json::to_vec_pretty(&composite)?)?;
                std::fs::rename(&fresh, &file)?;
                self.catalog.push(composite);
            }
            None => match std::fs::remove_file(&file) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.into()),
                _ => {}
            },
        }
        Ok(())
    }
    /// The controller's part of Doctor's facts; `crate::doctor` adds the rest.
    fn doctor_facts(&mut self) -> Result<zapret_core::doctor::Facts> {
        let s = self.status()?;
        Ok(zapret_core::doctor::Facts {
            enabled: std::fs::read(self.data.join("enabled")).is_ok_and(|v| v == b"1"),
            engine_running: s.running,
            engine_warnings: s.warnings,
            ..Default::default()
        })
    }
}
impl<E: Engine> Drop for Controller<E> {
    fn drop(&mut self) {
        let _ = self.engine.cleanup();
    }
}
impl<E: Engine> Backend for Controller<E> {
    fn status(&mut self) -> Result<ServiceStatus> {
        Controller::status(self)
    }
    fn strategies(&self) -> Result<Vec<Strategy>> {
        Ok(self.catalog.clone())
    }
    fn disable(&mut self) -> Result<()> {
        Controller::disable(self)
    }
    async fn enable(&mut self) -> Result<()> {
        Controller::enable(self).await
    }
    async fn set_strategy(&mut self, id: &str) -> Result<()> {
        Controller::set_strategy(self, id).await
    }
    fn set_composite(&mut self, composite: Option<Strategy>) -> Result<()> {
        Controller::set_composite(self, composite)
    }
}

/// The bundle's checks; a broken file falls back to the built-in list with a
/// warning instead of leaving the service without checks.
fn load_targets(root: &std::path::Path) -> (TargetList, Option<String>) {
    match TargetList::load(root) {
        Ok(targets) => (targets, None),
        Err(e) => (
            TargetList::built_in(),
            Some(format!(
                "Список проверок из набора не загружен ({e:#}); используется встроенный"
            )),
        ),
    }
}

/// The bundle catalog plus the saved composite, if it is valid.
fn with_composite(mut catalog: Vec<Strategy>, data: &std::path::Path) -> Vec<Strategy> {
    let saved = std::fs::read(data.join(COMPOSITE))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Strategy>(&bytes).ok())
        .filter(|s| s.id == zapret_core::composite::ID && strategy::validate(s).is_ok());
    catalog.retain(|s| s.id != zapret_core::composite::ID);
    catalog.extend(saved);
    catalog
}

pub struct Daemon<E: Engine> {
    pub controller: Arc<Mutex<Controller<E>>>,
    pub operations: Arc<OperationManager>,
    pub telegram: Arc<crate::telegram::Manager>,
}
impl<E: Engine + 'static> Daemon<E> {
    pub async fn dispatch(&self, command: DaemonCommand) -> Result<DaemonResponse> {
        match command {
            DaemonCommand::Snapshot => {
                let controller = &mut *self.controller.lock().await;
                Ok(DaemonResponse::Snapshot(Snapshot {
                    version: VERSION.into(),
                    service: controller.status()?,
                    strategies: controller.catalog.clone(),
                    operation: self.operations.operation.lock().await.clone(),
                    bundle: controller.bundle.clone(),
                    health: self.operations.health.lock().await.clone(),
                    app: self.operations.app.lock().await.clone(),
                    telegram: self.telegram.status().await,
                }))
            }
            // Its own switch: no engine restart, so no need to wait for a check.
            DaemonCommand::SetTelegramProxy { enabled } => {
                self.telegram.set(enabled).await?;
                Ok(DaemonResponse::Empty(()))
            }
            DaemonCommand::Diagnose
            | DaemonCommand::SelectStrategy
            | DaemonCommand::RepairStrategy => {
                let mode = match command {
                    DaemonCommand::Diagnose => Mode::Diagnose,
                    DaemonCommand::SelectStrategy => Mode::Select,
                    _ => {
                        let broken = self.operations.health.lock().await.broken.clone();
                        if broken.is_empty() {
                            bail!("Фоновая проверка не нашла сломавшихся сервисов");
                        }
                        Mode::Repair(broken.into_iter().collect())
                    }
                };
                self.operations
                    .launch(
                        self.controller.clone(),
                        Arc::new(NetworkProbes {
                            targets: self.controller.lock().await.targets.clone(),
                        }),
                        mode,
                    )
                    .await?;
                Ok(DaemonResponse::Empty(()))
            }
            DaemonCommand::UpdateApp => {
                let app = self.operations.app.lock().await.clone();
                if app.available.is_none() {
                    bail!("Новой версии приложения пока нет");
                }
                if !app.installing {
                    if self.operations.gate.try_lock().is_err() {
                        bail!("Дождитесь завершения проверки или отмените её");
                    }
                    // The updater verifies the installer again and starts it.
                    self.operations.install_app.notify_one();
                }
                Ok(DaemonResponse::Empty(()))
            }
            DaemonCommand::CancelOperation => {
                self.operations.cancel();
                Ok(DaemonResponse::Empty(()))
            }
            DaemonCommand::Domains => Ok(DaemonResponse::Domains(
                self.controller.lock().await.domains()?,
            )),
            DaemonCommand::Logs => Ok(DaemonResponse::Logs(log_tail(
                &self.controller.lock().await.data.join("engine.log"),
            ))),
            DaemonCommand::Report => {
                let service = self.controller.lock().await.status()?;
                let operation = self.operations.operation.lock().await.clone();
                Ok(DaemonResponse::Report(Report {
                    app_version: VERSION.into(),
                    platform: service.platform,
                    engine: service.engine,
                    running: service.running,
                    strategy: service.strategy,
                    capabilities: service.capabilities,
                    packets: service.packets,
                    errors: service.errors,
                    checks: operation
                        .results
                        .iter()
                        .map(|r| ReportCheck {
                            service: r.id.clone(),
                            protocol: r.protocol.clone(),
                            ok: r.ok,
                            latency_ms: r.latency_ms,
                        })
                        .collect(),
                }))
            }
            other => {
                let _gate = self.operations.gate.try_lock().map_err(|_| {
                    anyhow::anyhow!("Дождитесь завершения проверки или отмените её")
                })?;
                // Applying and checking secure DNS takes seconds; status requests
                // keep working meanwhile, so it runs off the controller lock.
                if let DaemonCommand::SetSecureDns { enabled } = other {
                    let data = self.controller.lock().await.data.clone();
                    tokio::task::spawn_blocking(move || crate::secure_dns::set(&data, enabled))
                        .await??;
                    return Ok(DaemonResponse::Empty(()));
                }
                // Doctor queries DNS and the service manager: off the lock too.
                if let DaemonCommand::Doctor = other {
                    let (root, data, facts) = {
                        let controller = &mut *self.controller.lock().await;
                        let facts = controller.doctor_facts()?;
                        (controller.root.clone(), controller.data.clone(), facts)
                    };
                    let facts = tokio::task::spawn_blocking(move || {
                        crate::doctor::facts(&root, &data, facts)
                    })
                    .await?;
                    return Ok(DaemonResponse::Doctor(DoctorResult {
                        checks: zapret_core::doctor::checks(&facts),
                    }));
                }
                let controller = &mut *self.controller.lock().await;
                match other {
                    DaemonCommand::Enable => controller.enable().await?,
                    DaemonCommand::Disable => controller.disable()?,
                    DaemonCommand::SetStrategy { strategy_id } => {
                        controller.set_strategy(&strategy_id).await?
                    }
                    DaemonCommand::AddDomain { domain } => controller.add_domain(&domain)?,
                    DaemonCommand::RemoveDomain { domain } => controller.remove_domain(&domain)?,
                    _ => unreachable!(),
                }
                Ok(DaemonResponse::Empty(()))
            }
        }
    }
}
