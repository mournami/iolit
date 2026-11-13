//! Transport-independent daemon state and fixed command dispatch.
use crate::{
    engine::Engine,
    logging::log_tail,
    operation::OperationManager,
    selector::{Backend, NetworkProbes},
};
use anyhow::{Context, Result, bail};
use std::{path::PathBuf, sync::Arc};
use tokio::sync::Mutex;
use zapret_core::{VERSION, api::*, strategy};

/// The writable user list; strategies get it in place of `list-general-user.txt`.
const USER_DOMAINS: &str = "user-domains.txt";

pub struct Controller<E: Engine> {
    pub(crate) root: PathBuf,
    pub(crate) data: PathBuf,
    catalog: Vec<Strategy>,
    active: String,
    engine: E,
    pub(crate) warning: Option<String>,
    pub bundle: BundleStatus,
}
impl<E: Engine> Controller<E> {
    pub fn new(root: PathBuf, data: PathBuf, engine: E) -> Result<Self> {
        let catalog = strategy::load_catalog(&root)?;
        std::fs::create_dir_all(&data)?;
        let saved = std::fs::read_to_string(data.join("strategy")).unwrap_or_default();
        let active = catalog
            .iter()
            .find(|s| s.id == saved.trim())
            .or_else(|| catalog.first())
            .context("Нет стратегий")?
            .id
            .clone();
        Ok(Self {
            root,
            data,
            catalog,
            active,
            engine,
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
        let catalog = strategy::load_catalog(&root)?;
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
        if running && let Err(e) = self.start().await {
            let _ = self.stop();
            self.root = old_root;
            self.catalog = old_catalog;
            self.active = old_active;
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
    fn doctor(&mut self) -> Result<DoctorResult> {
        let s = self.status()?;
        Ok(DoctorResult {
            checks: vec![
                DoctorCheck {
                    name: "Системная служба".into(),
                    ok: true,
                    detail: Some("Служба доступна".into()),
                },
                DoctorCheck {
                    name: "Движок".into(),
                    ok: s.running,
                    detail: Some(
                        if s.running {
                            "Движок запущен"
                        } else {
                            "Движок выключен"
                        }
                        .into(),
                    ),
                },
                DoctorCheck {
                    name: "WinDivert".into(),
                    ok: self.root.join("bin/WinDivert64.sys").is_file(),
                    detail: Some(
                        "Проверка наличия драйвера. Работу подтверждает запуск движка.".into(),
                    ),
                },
            ],
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
}

pub struct Daemon<E: Engine> {
    pub controller: Arc<Mutex<Controller<E>>>,
    pub operations: Arc<OperationManager>,
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
                }))
            }
            DaemonCommand::Diagnose | DaemonCommand::SelectStrategy => {
                self.operations
                    .launch(
                        self.controller.clone(),
                        Arc::new(NetworkProbes),
                        matches!(command, DaemonCommand::SelectStrategy),
                    )
                    .await?;
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
                let controller = &mut *self.controller.lock().await;
                match other {
                    DaemonCommand::Enable => controller.enable().await?,
                    DaemonCommand::Disable => controller.disable()?,
                    DaemonCommand::SetStrategy { strategy_id } => {
                        controller.set_strategy(&strategy_id).await?
                    }
                    DaemonCommand::AddDomain { domain } => controller.add_domain(&domain)?,
                    DaemonCommand::RemoveDomain { domain } => controller.remove_domain(&domain)?,
                    DaemonCommand::Doctor => {
                        return Ok(DaemonResponse::Doctor(controller.doctor()?));
                    }
                    _ => unreachable!(),
                }
                Ok(DaemonResponse::Empty(()))
            }
        }
    }
}
