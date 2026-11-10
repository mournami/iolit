//! Daemon-owned operations survive closing the GUI. Recovery belongs to service data.
use crate::selector::{self, Backend, Probes};
use anyhow::Result;
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use tokio::sync::Mutex;
use zapret_core::{Operation, ServiceStatus};

pub struct OperationManager {
    pub operation: Mutex<Operation>,
    pub gate: Arc<Mutex<()>>,
    pub(crate) cancelled: AtomicBool,
    pub(crate) recovery_path: PathBuf,
}

impl OperationManager {
    pub fn new(data: PathBuf) -> Self {
        Self {
            operation: Mutex::new(Operation::default()),
            gate: Arc::new(Mutex::new(())),
            cancelled: AtomicBool::new(false),
            recovery_path: data.join("selection-recovery.json"),
        }
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    pub async fn launch<B: Backend + 'static, P: Probes + 'static>(
        self: &Arc<Self>,
        backend: Arc<Mutex<B>>,
        probes: Arc<P>,
        selection: bool,
    ) -> Result<()> {
        let gate = self
            .gate
            .clone()
            .try_lock_owned()
            .map_err(|_| anyhow::anyhow!("Проверка уже выполняется"))?;
        self.cancelled.store(false, Ordering::Release);
        *self.operation.lock().await = Operation {
            busy: true,
            stage: if selection {
                "Подготавливаем подбор"
            } else {
                "Проверяем соединения"
            }
            .into(),
            ..Default::default()
        };
        let state = self.clone();
        tokio::spawn(async move {
            let _gate = gate;
            let result = if selection {
                selector::auto_select(&state, &backend, probes.as_ref()).await
            } else {
                let results = probes.run().await;
                state.operation.lock().await.results = results;
                Ok(())
            };
            let mut operation = state.operation.lock().await;
            operation.busy = false;
            operation.stage = if state.cancelled.load(Ordering::Acquire) {
                "Проверка отменена"
            } else if result.is_err() {
                "Проверка завершилась с ошибкой"
            } else {
                "Проверка завершена"
            }
            .into();
            operation.error = result.err().map(|e| format!("{e:#}"));
        });
        Ok(())
    }

    /// Called before accepting requests or restoring the persisted enabled preference.
    pub async fn recover<B: Backend>(&self, backend: &Arc<Mutex<B>>) -> Result<bool> {
        if !self.recovery_path.try_exists()? {
            return Ok(false);
        }
        let _gate = self.gate.lock().await;
        *self.operation.lock().await = Operation {
            busy: true,
            stage: "Восстанавливаем настройку после прерванного подбора".into(),
            ..Default::default()
        };
        let result = async {
            let saved: ServiceStatus =
                serde_json::from_slice(&tokio::fs::read(&self.recovery_path).await?)?;
            selector::restore_service(backend, &saved).await?;
            tokio::fs::remove_file(&self.recovery_path).await?;
            Ok::<_, anyhow::Error>(())
        }
        .await;
        let mut op = self.operation.lock().await;
        op.busy = false;
        op.stage = "Восстановление настройки".into();
        match &result {
            Ok(()) => op.notice = Some("Предыдущая настройка восстановлена.".into()),
            Err(e) => {
                op.error = Some(format!(
                    "Не удалось автоматически восстановить настройку: {e:#}. Проверьте системную службу."
                ))
            }
        }
        result.map(|()| true)
    }
}
