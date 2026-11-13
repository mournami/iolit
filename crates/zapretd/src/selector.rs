//! Selection moved from the Tauri client; rounds and rollback retain their order.
use crate::operation::OperationManager;
use anyhow::Result;
use std::{
    future::Future,
    sync::{Arc, atomic::Ordering},
};
use tokio::sync::Mutex;
use zapret_core::{ProbeResult, ServiceStatus, Strategy};

pub trait Backend: Send {
    fn status(&mut self) -> Result<ServiceStatus>;
    fn strategies(&self) -> Result<Vec<Strategy>>;
    fn disable(&mut self) -> Result<()>;
    fn enable(&mut self) -> impl Future<Output = Result<()>> + Send;
    fn set_strategy(&mut self, id: &str) -> impl Future<Output = Result<()>> + Send;
}

pub trait Probes: Send + Sync {
    fn run(&self) -> impl Future<Output = Vec<ProbeResult>> + Send;
}
pub struct NetworkProbes;
impl Probes for NetworkProbes {
    async fn run(&self) -> Vec<ProbeResult> {
        zapret_core::probe::run().await
    }
}

async fn wait_ready<B: Backend>(backend: &Arc<Mutex<B>>) -> anyhow::Result<()> {
    for _ in 0..30 {
        if backend.lock().await.status()?.running {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
    anyhow::bail!("Движок не запустился")
}

pub async fn restore_service<B: Backend>(
    backend: &Arc<Mutex<B>>,
    original: &ServiceStatus,
) -> anyhow::Result<()> {
    backend.lock().await.disable()?;
    if !original.strategy.is_empty() {
        backend
            .lock()
            .await
            .set_strategy(&original.strategy)
            .await?;
    }
    if original.running {
        backend.lock().await.enable().await?;
    }
    Ok(())
}

pub async fn auto_select<B: Backend, P: Probes>(
    state: &OperationManager,
    backend: &Arc<Mutex<B>>,
    probes: &P,
) -> anyhow::Result<()> {
    let original = backend.lock().await.status()?;
    let candidates: Vec<_> = backend
        .lock()
        .await
        .strategies()?
        .into_iter()
        .filter(|s| s.supported)
        .collect();
    if candidates.is_empty() {
        anyhow::bail!("Нет совместимых стратегий");
    }
    let recovery_path = state.recovery_path.clone();
    tokio::fs::write(&recovery_path, serde_json::to_vec(&original)?).await?;
    state.operation.lock().await.total = candidates.len();
    // Candidates whose engine did not start, with the first line of the reason.
    let mut skipped = Vec::new();
    // Always roll back on cancellation, failed probes, or a service/IPC error.
    let outcome = async {
        backend.lock().await.disable()?;
        state.operation.lock().await.stage = "Проверяем доступность без обхода".into();
        let baseline = probes.run().await;
        let all_accessible = baseline.iter().all(|r| r.ok);
        state.operation.lock().await.results = baseline;
        if all_accessible {
            state.operation.lock().await.notice = Some(
                "Все проверенные соединения доступны без обхода. Исходная настройка сохранена."
                    .into(),
            );
            return Ok(None);
        }
        for (index, s) in candidates.iter().enumerate() {
            if state.cancelled.load(Ordering::Acquire) {
                return Ok(None);
            }
            {
                let mut op = state.operation.lock().await;
                op.current = index + 1;
                op.stage = format!("Проверяем {}", s.name);
            }
            backend.lock().await.disable()?;
            backend.lock().await.set_strategy(&s.id).await?;
            // One strategy the engine rejects must not end the whole search.
            let enabled = backend.lock().await.enable().await;
            let started = match enabled {
                Ok(()) => wait_ready(backend).await,
                Err(e) => Err(e),
            };
            if let Err(e) = started {
                let reason = e.to_string();
                let reason = reason.lines().next().unwrap_or_default().trim();
                skipped.push(format!("{} ({reason})", s.name));
                continue;
            }
            // New clients and sockets per round prevent connection reuse between candidates.
            let first = probes.run().await;
            let first_pass = first.iter().all(|r| r.ok);
            state.operation.lock().await.results = first;
            if first_pass && !state.cancelled.load(Ordering::Acquire) {
                let confirmation = probes.run().await;
                let passed = confirmation.iter().all(|r| r.ok);
                state.operation.lock().await.results = confirmation;
                if passed {
                    return Ok::<_, anyhow::Error>(Some(s.id.clone()));
                }
            }
        }
        Ok(None)
    }
    .await;
    match outcome {
        Ok(Some(winner)) if !state.cancelled.load(Ordering::Acquire) => {
            let mut op = state.operation.lock().await;
            op.winner = Some(winner);
            if !skipped.is_empty() {
                op.notice = Some(format!(
                    "Не запустились и пропущены: {}.",
                    skipped.join("; ")
                ));
            }
            drop(op);
            tokio::fs::remove_file(&recovery_path).await?;
            Ok(())
        }
        other => {
            restore_service(backend, &original).await.map_err(|e| {
                anyhow::anyhow!("Не удалось восстановить исходную настройку: {e:#}")
            })?;
            tokio::fs::remove_file(&recovery_path).await?;
            match other {
                Err(e) => Err(e),
                _ if state.cancelled.load(Ordering::Acquire) => Ok(()),
                _ if state.operation.lock().await.notice.is_some() => Ok(()),
                _ if skipped.is_empty() => anyhow::bail!(
                    "Ни одна стратегия не прошла все проверки. Исходная настройка восстановлена."
                ),
                _ => anyhow::bail!(
                    "Ни одна стратегия не прошла все проверки. Не запустились: {}. Исходная настройка восстановлена.",
                    skipped.join("; ")
                ),
            }
        }
    }
}
