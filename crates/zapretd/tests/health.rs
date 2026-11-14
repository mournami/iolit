//! The background check with scripted results; no network.
use anyhow::Result;
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex as SyncMutex},
    time::Duration,
};
use tokio::sync::Mutex;
use zapret_core::{ProbeResult, ServiceStatus, Strategy};
use zapretd::{
    health::{Monitor, tick},
    operation::OperationManager,
    selector::{Backend, Probes},
};

struct Fake {
    running: bool,
    strategy: String,
}
impl Backend for Fake {
    fn status(&mut self) -> Result<ServiceStatus> {
        let mut status = ServiceStatus::unavailable(String::new());
        status.running = self.running;
        status.strategy = self.strategy.clone();
        Ok(status)
    }
    fn strategies(&self) -> Result<Vec<Strategy>> {
        Ok(vec![])
    }
    fn disable(&mut self) -> Result<()> {
        Ok(())
    }
    async fn enable(&mut self) -> Result<()> {
        Ok(())
    }
    async fn set_strategy(&mut self, _: &str) -> Result<()> {
        Ok(())
    }
    fn set_composite(&mut self, _: Option<Strategy>) -> Result<()> {
        Ok(())
    }
}

/// Each round: the ids that pass.
struct Rounds(SyncMutex<VecDeque<Vec<&'static str>>>);
impl Probes for Rounds {
    async fn run(&self) -> Vec<ProbeResult> {
        let ok = self
            .0
            .lock()
            .unwrap()
            .pop_front()
            .expect("unexpected round");
        ["internet", "youtube", "discord"]
            .iter()
            .map(|id| ProbeResult {
                id: id.to_string(),
                name: id.to_string(),
                protocol: "TCP".into(),
                ok: ok.contains(id),
                latency_ms: 1,
                detail: String::new(),
                warning: None,
            })
            .collect()
    }
}

fn rounds(r: &[&[&'static str]]) -> Rounds {
    Rounds(SyncMutex::new(r.iter().map(|x| x.to_vec()).collect()))
}

#[tokio::test]
async fn a_service_that_stops_working_is_reported_after_a_second_look() {
    let data = tempfile::tempdir().unwrap();
    let state = OperationManager::new(data.path().into());
    let backend = Arc::new(Mutex::new(Fake {
        running: true,
        strategy: "a".into(),
    }));
    let mut monitor = Monitor::default();
    let probes = rounds(&[
        &["internet", "youtube", "discord"],
        // A blip: YouTube fails once, then works on the second look.
        &["internet", "discord"],
        &["internet", "youtube", "discord"],
        // Really broken: fails twice.
        &["internet", "discord"],
        &["internet", "discord"],
    ]);
    let recheck = Duration::from_millis(1);
    tick(&mut monitor, &state, &backend, &probes, recheck)
        .await
        .unwrap();
    assert!(state.health.lock().await.broken.is_empty());
    assert!(state.health.lock().await.checked_at > 0);
    tick(&mut monitor, &state, &backend, &probes, recheck)
        .await
        .unwrap();
    assert!(state.health.lock().await.broken.is_empty(), "blip ignored");
    tick(&mut monitor, &state, &backend, &probes, recheck)
        .await
        .unwrap();
    assert_eq!(state.health.lock().await.broken, ["youtube"]);
}

#[tokio::test]
async fn nothing_runs_while_the_bypass_is_off_or_an_operation_runs() {
    let data = tempfile::tempdir().unwrap();
    let state = OperationManager::new(data.path().into());
    let backend = Arc::new(Mutex::new(Fake {
        running: false,
        strategy: "a".into(),
    }));
    let mut monitor = Monitor::default();
    // No rounds: any probe would panic.
    let probes = rounds(&[]);
    tick(&mut monitor, &state, &backend, &probes, Duration::ZERO)
        .await
        .unwrap();
    assert_eq!(state.health.lock().await.checked_at, 0);
    backend.lock().await.running = true;
    let _busy = state.gate.lock().await;
    tick(&mut monitor, &state, &backend, &probes, Duration::ZERO)
        .await
        .unwrap();
    assert_eq!(state.health.lock().await.checked_at, 0);
}
