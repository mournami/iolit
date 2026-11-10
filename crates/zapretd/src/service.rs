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

pub struct Controller<E: Engine> {
    pub(crate) root: PathBuf,
    pub(crate) data: PathBuf,
    catalog: Vec<Strategy>,
    active: String,
    engine: E,
    pub(crate) warning: Option<String>,
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
        })
    }
    fn status(&mut self) -> Result<ServiceStatus> {
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
    async fn add_domain(&mut self, domain: &str) -> Result<()> {
        let domain = strategy::normalize_domain(domain)?;
        let file = self.data.join("user-domains.txt");
        let mut text = std::fs::read_to_string(&file).unwrap_or_default();
        if !text.lines().any(|l| l == domain) {
            text.push_str(&format!("{domain}\n"));
            std::fs::write(file, text)?;
        }
        if self.status()?.running {
            self.stop()?;
            self.start().await?;
        }
        Ok(())
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
                    DaemonCommand::AddDomain { domain } => controller.add_domain(&domain).await?,
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
