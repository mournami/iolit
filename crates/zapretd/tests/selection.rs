//! Controlled probes exercise rollback without changing this machine's network.
use anyhow::{Result, bail};
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex as SyncMutex},
};
use tokio::sync::{Mutex, Notify};
use zapret_core::{ProbeResult, ServiceStatus, Strategy};
use zapretd::{
    operation::OperationManager,
    selector::{self, Backend, Probes},
};

struct FakeBackend {
    running: bool,
    active: String,
    events: Vec<String>,
    fail_start: bool,
    fail_restore: bool,
}
impl FakeBackend {
    fn new(running: bool) -> Self {
        Self {
            running,
            active: "original".into(),
            events: vec![],
            fail_start: false,
            fail_restore: false,
        }
    }
}
impl Backend for FakeBackend {
    fn status(&mut self) -> Result<ServiceStatus> {
        let mut status = ServiceStatus::unavailable(String::new());
        status.installed = true;
        status.running = self.running;
        status.strategy = self.active.clone();
        Ok(status)
    }
    fn strategies(&self) -> Result<Vec<Strategy>> {
        Ok(["candidate-1", "candidate-2"]
            .iter()
            .map(|id| Strategy {
                id: id.to_string(),
                name: id.to_string(),
                description: String::new(),
                supported: true,
                arguments: vec!["--test".into()],
            })
            .collect())
    }
    fn disable(&mut self) -> Result<()> {
        self.events.push("disable".into());
        self.running = false;
        Ok(())
    }
    async fn enable(&mut self) -> Result<()> {
        self.events.push(format!("enable:{}", self.active));
        if self.fail_start {
            self.fail_start = false;
            bail!("engine startup failed");
        }
        self.running = true;
        Ok(())
    }
    async fn set_strategy(&mut self, id: &str) -> Result<()> {
        self.events.push(format!("strategy:{id}"));
        if self.fail_restore && id == "original" {
            bail!("restore failed");
        }
        self.active = id.into();
        Ok(())
    }
}

struct ScriptedProbes {
    rounds: SyncMutex<VecDeque<bool>>,
    seen: SyncMutex<Vec<(bool, String)>>,
    backend: Arc<Mutex<FakeBackend>>,
    state: Arc<OperationManager>,
    cancel_on: usize,
}
impl Probes for ScriptedProbes {
    async fn run(&self) -> Vec<ProbeResult> {
        let backend = self.backend.lock().await;
        let mut seen = self.seen.lock().unwrap();
        seen.push((backend.running, backend.active.clone()));
        if seen.len() == self.cancel_on {
            self.state.cancel();
        }
        let ok = self
            .rounds
            .lock()
            .unwrap()
            .pop_front()
            .expect("unexpected probe round");
        vec![ProbeResult {
            id: "test".into(),
            name: "test".into(),
            protocol: "TCP".into(),
            ok,
            latency_ms: 1,
            detail: String::new(),
        }]
    }
}
fn setup(
    running: bool,
    rounds: &[bool],
    cancel_on: usize,
) -> (
    tempfile::TempDir,
    Arc<Mutex<FakeBackend>>,
    Arc<OperationManager>,
    ScriptedProbes,
) {
    let data = tempfile::tempdir().unwrap();
    let backend = Arc::new(Mutex::new(FakeBackend::new(running)));
    let state = Arc::new(OperationManager::new(data.path().into()));
    let probes = ScriptedProbes {
        rounds: SyncMutex::new(rounds.iter().copied().collect()),
        seen: SyncMutex::new(vec![]),
        backend: backend.clone(),
        state: state.clone(),
        cancel_on,
    };
    (data, backend, state, probes)
}

#[tokio::test]
async fn baseline_restores_the_original_enabled_state() {
    let (data, backend, state, probes) = setup(true, &[true], 0);
    selector::auto_select(&state, &backend, &probes)
        .await
        .unwrap();
    assert_eq!(*probes.seen.lock().unwrap(), [(false, "original".into())]);
    assert!(backend.lock().await.running);
    assert_eq!(backend.lock().await.active, "original");
    assert!(state.operation.lock().await.notice.is_some());
    assert!(!data.path().join("selection-recovery.json").exists());
}
#[tokio::test]
async fn winner_requires_two_successful_rounds() {
    let (_data, backend, state, probes) = setup(false, &[false, true, false, true, true], 0);
    selector::auto_select(&state, &backend, &probes)
        .await
        .unwrap();
    assert_eq!(
        *probes.seen.lock().unwrap(),
        [
            (false, "original".into()),
            (true, "candidate-1".into()),
            (true, "candidate-1".into()),
            (true, "candidate-2".into()),
            (true, "candidate-2".into()),
        ]
    );
    assert_eq!(
        state.operation.lock().await.winner.as_deref(),
        Some("candidate-2")
    );
    assert!(backend.lock().await.running);
}
#[tokio::test]
async fn cancellation_during_confirmation_rolls_back() {
    let (data, backend, state, probes) = setup(false, &[false, true, true], 3);
    selector::auto_select(&state, &backend, &probes)
        .await
        .unwrap();
    assert_eq!(backend.lock().await.active, "original");
    assert!(!backend.lock().await.running);
    assert!(state.operation.lock().await.winner.is_none());
    assert!(!data.path().join("selection-recovery.json").exists());
}
#[tokio::test]
async fn failed_candidates_restore_the_original_state() {
    let (_data, backend, state, probes) = setup(true, &[false, false, false], 0);
    assert!(
        selector::auto_select(&state, &backend, &probes)
            .await
            .is_err()
    );
    assert_eq!(backend.lock().await.active, "original");
    assert!(backend.lock().await.running);
}
#[tokio::test]
async fn engine_failure_rolls_back_and_failed_restore_keeps_recovery() {
    let (data, backend, state, probes) = setup(true, &[false], 0);
    backend.lock().await.fail_start = true;
    assert!(
        selector::auto_select(&state, &backend, &probes)
            .await
            .is_err()
    );
    assert_eq!(backend.lock().await.active, "original");
    assert!(backend.lock().await.running);
    assert!(!data.path().join("selection-recovery.json").exists());

    let (data, backend, state, probes) = setup(true, &[false, false, false], 0);
    backend.lock().await.fail_restore = true;
    assert!(
        selector::auto_select(&state, &backend, &probes)
            .await
            .is_err()
    );
    assert!(data.path().join("selection-recovery.json").exists());
    backend.lock().await.fail_restore = false;
    assert!(state.recover(&backend).await.unwrap());
    assert_eq!(backend.lock().await.active, "original");
    assert!(backend.lock().await.running);
    assert!(!state.recover(&backend).await.unwrap());
}

struct PausedProbes {
    started: Notify,
    release: Notify,
}
impl Probes for PausedProbes {
    async fn run(&self) -> Vec<ProbeResult> {
        self.started.notify_one();
        self.release.notified().await;
        vec![]
    }
}
#[tokio::test]
async fn daemon_owns_operation_and_snapshot_remains_readable() {
    let data = tempfile::tempdir().unwrap();
    let state = Arc::new(OperationManager::new(data.path().into()));
    let observer = state.clone();
    let backend = Arc::new(Mutex::new(FakeBackend::new(false)));
    let probes = Arc::new(PausedProbes {
        started: Notify::new(),
        release: Notify::new(),
    });
    state
        .launch(backend.clone(), probes.clone(), false)
        .await
        .unwrap();
    probes.started.notified().await;
    assert!(state.launch(backend, probes.clone(), true).await.is_err());
    assert!(state.gate.try_lock().is_err());
    assert!(
        tokio::time::timeout(std::time::Duration::from_secs(1), state.operation.lock())
            .await
            .unwrap()
            .busy
    );
    drop(state);
    observer.cancel();
    probes.release.notify_one();
    let _finished = observer.gate.lock().await;
    let op = observer.operation.lock().await;
    assert!(!op.busy);
    assert_eq!(op.stage, "Проверка отменена");
}
