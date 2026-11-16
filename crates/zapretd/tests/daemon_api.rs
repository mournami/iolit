use anyhow::Result;
use std::{path::Path, sync::Arc};
use tokio::sync::Mutex;
use zapret_core::{api::*, ipc::MAX_MESSAGE};
use zapretd::{
    engine::{Engine, EngineState},
    operation::OperationManager,
    service::{Controller, Daemon},
};

#[derive(Default)]
struct TestEngine {
    running: bool,
}
impl Engine for TestEngine {
    const PLATFORM: &'static str = "test";
    const NAME: &'static str = "test engine";
    const CAPABILITIES: Capabilities = Capabilities {
        tcp: true,
        udp: true,
        ipv6: false,
        packet_injection: false,
    };
    async fn start(&mut self, _: &Strategy, _: &Path, _: &Path) -> Result<()> {
        self.running = true;
        Ok(())
    }
    fn stop(&mut self) -> Result<()> {
        self.running = false;
        Ok(())
    }
    fn is_ready(&mut self) -> Result<EngineState> {
        Ok(EngineState {
            running: self.running,
            uptime_secs: 0,
            warnings: vec!["192.0.2.4 private.example".into()],
        })
    }
    fn cleanup(&mut self) -> Result<()> {
        self.stop()
    }
}

#[tokio::test]
async fn commands_preserve_preferences_busy_gate_and_report_privacy() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("bundle");
    let data = dir.path().join("data");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join("strategies.json"),
        serde_json::to_vec(&vec![Strategy {
            id: "general".into(),
            name: "General".into(),
            description: String::new(),
            supported: true,
            arguments: vec!["--test".into()],
        }])
        .unwrap(),
    )
    .unwrap();
    let controller = Controller::new(root.clone(), data.clone(), TestEngine::default()).unwrap();
    let daemon = Daemon {
        controller: Arc::new(Mutex::new(controller)),
        operations: Arc::new(OperationManager::new(data.clone())),
        telegram: Arc::new(zapretd::telegram::Manager::new(data.clone())),
    };
    let DaemonResponse::Snapshot(initial) = daemon.dispatch(DaemonCommand::Snapshot).await.unwrap()
    else {
        panic!()
    };
    assert!(!initial.service.running);
    assert!(!data.join("enabled").exists());
    daemon.dispatch(DaemonCommand::Enable).await.unwrap();
    assert_eq!(std::fs::read(data.join("enabled")).unwrap(), b"1");
    daemon
        .dispatch(DaemonCommand::SetStrategy {
            strategy_id: "general".into(),
        })
        .await
        .unwrap();
    assert_eq!(
        std::fs::read_to_string(data.join("strategy")).unwrap(),
        "general"
    );
    assert!(
        daemon
            .dispatch(DaemonCommand::SetStrategy {
                strategy_id: "../escape".into()
            })
            .await
            .is_err()
    );
    assert!(
        daemon
            .dispatch(DaemonCommand::AddDomain {
                domain: "example.com\n--debug".into()
            })
            .await
            .is_err()
    );
    daemon
        .dispatch(DaemonCommand::AddDomain {
            domain: "private.example".into(),
        })
        .await
        .unwrap();
    std::fs::write(data.join("engine.log"), "192.0.2.4 private.example raw log").unwrap();
    daemon.operations.operation.lock().await.results = vec![ProbeResult {
        id: "youtube".into(),
        name: "private.example".into(),
        protocol: "TCP".into(),
        ok: false,
        latency_ms: 1,
        detail: "192.0.2.4 private.example".into(),
        warning: None,
    }];
    let report =
        serde_json::to_string(&daemon.dispatch(DaemonCommand::Report).await.unwrap()).unwrap();
    assert!(!report.contains("192.0.2.4"));
    assert!(!report.contains("private.example"));
    assert!(!report.contains("raw log"));
    assert!(report.contains("youtube"));
    assert!(report.len() < MAX_MESSAGE);
    let _gate = daemon.operations.gate.lock().await;
    assert!(daemon.dispatch(DaemonCommand::Disable).await.is_err());
    assert!(daemon.dispatch(DaemonCommand::Snapshot).await.is_ok());
    assert!(
        daemon
            .dispatch(DaemonCommand::CancelOperation)
            .await
            .is_ok()
    );
    drop(_gate);
    daemon.dispatch(DaemonCommand::Disable).await.unwrap();
    assert_eq!(std::fs::read(data.join("enabled")).unwrap(), b"0");
    // Disabling still stops the engine when persisting the preference fails.
    daemon.dispatch(DaemonCommand::Enable).await.unwrap();
    std::fs::remove_file(data.join("enabled")).unwrap();
    std::fs::create_dir(data.join("enabled")).unwrap();
    assert!(daemon.dispatch(DaemonCommand::Disable).await.is_err());
    let DaemonResponse::Snapshot(final_state) =
        daemon.dispatch(DaemonCommand::Snapshot).await.unwrap()
    else {
        panic!()
    };
    assert!(!final_state.service.running);
}

#[cfg(not(windows))]
#[tokio::test]
async fn unsupported_platform_never_reports_engine_success() {
    use zapretd::engine::UnsupportedEngine;
    let mut engine = UnsupportedEngine;
    let s = Strategy {
        id: "test".into(),
        name: String::new(),
        description: String::new(),
        supported: true,
        arguments: vec![],
    };
    assert!(
        engine
            .start(&s, Path::new("."), Path::new("."))
            .await
            .is_err()
    );
    assert!(engine.is_ready().is_err());
    assert!(engine.stop().is_err());
    assert!(engine.cleanup().is_err());
    assert!(
        zapret_core::ipc::request_command::<Snapshot>(DaemonCommand::Snapshot)
            .await
            .is_err()
    );
}

/// Counts engine starts so tests can tell an engine restart apart.
struct CountingEngine {
    running: bool,
    starts: Arc<std::sync::atomic::AtomicUsize>,
}
impl Engine for CountingEngine {
    const PLATFORM: &'static str = "test";
    const NAME: &'static str = "counting engine";
    const CAPABILITIES: Capabilities = Capabilities {
        tcp: true,
        udp: true,
        ipv6: false,
        packet_injection: false,
    };
    async fn start(&mut self, _: &Strategy, _: &Path, _: &Path) -> Result<()> {
        self.starts
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.running = true;
        Ok(())
    }
    fn stop(&mut self) -> Result<()> {
        self.running = false;
        Ok(())
    }
    fn is_ready(&mut self) -> Result<EngineState> {
        Ok(EngineState {
            running: self.running,
            uptime_secs: 0,
            warnings: vec![],
        })
    }
    fn cleanup(&mut self) -> Result<()> {
        self.stop()
    }
}

#[tokio::test]
async fn user_domains_are_listed_removed_and_applied_without_restart() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("bundle");
    let data = dir.path().join("data");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join("strategies.json"),
        serde_json::to_vec(&vec![Strategy {
            id: "general".into(),
            name: "General".into(),
            description: String::new(),
            supported: true,
            arguments: vec!["--test".into()],
        }])
        .unwrap(),
    )
    .unwrap();
    let starts = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let engine = CountingEngine {
        running: false,
        starts: starts.clone(),
    };
    let daemon = Daemon {
        controller: Arc::new(Mutex::new(
            Controller::new(root, data.clone(), engine).unwrap(),
        )),
        operations: Arc::new(OperationManager::new(data.clone())),
        telegram: Arc::new(zapretd::telegram::Manager::new(data.clone())),
    };
    let domains = || async {
        match daemon.dispatch(DaemonCommand::Domains).await.unwrap() {
            DaemonResponse::Domains(list) => list,
            other => panic!("{other:?}"),
        }
    };
    assert!(domains().await.is_empty());
    daemon.dispatch(DaemonCommand::Enable).await.unwrap();
    for domain in ["Example.com", "пример.рф", "example.com."] {
        daemon
            .dispatch(DaemonCommand::AddDomain {
                domain: domain.into(),
            })
            .await
            .unwrap();
    }
    // winws reloads the changed file itself; a restart would drop bypass.
    assert_eq!(starts.load(std::sync::atomic::Ordering::SeqCst), 1);
    assert_eq!(
        std::fs::read_to_string(data.join("user-domains.txt")).unwrap(),
        "example.com\nxn--e1afmkfd.xn--p1ai\n"
    );
    assert_eq!(domains().await, ["example.com", "пример.рф"]);
    daemon
        .dispatch(DaemonCommand::RemoveDomain {
            domain: "ПРИМЕР.РФ".into(),
        })
        .await
        .unwrap();
    assert_eq!(domains().await, ["example.com"]);
    for bad in ["missing.example", "example.com\n--debug"] {
        assert!(
            daemon
                .dispatch(DaemonCommand::RemoveDomain { domain: bad.into() })
                .await
                .is_err(),
            "{bad}"
        );
    }
    daemon
        .dispatch(DaemonCommand::RemoveDomain {
            domain: "example.com".into(),
        })
        .await
        .unwrap();
    assert_eq!(
        std::fs::read_to_string(data.join("user-domains.txt")).unwrap(),
        ""
    );
    assert_eq!(starts.load(std::sync::atomic::Ordering::SeqCst), 1);
}

#[tokio::test]
async fn doctor_reports_a_broken_engine_and_missing_files() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("bundle");
    let data = dir.path().join("data");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join("strategies.json"),
        serde_json::to_vec(&vec![Strategy {
            id: "general".into(),
            name: "General".into(),
            description: String::new(),
            supported: true,
            arguments: vec!["--test".into()],
        }])
        .unwrap(),
    )
    .unwrap();
    std::fs::create_dir_all(&data).unwrap();
    // Turned on earlier, but the engine is not running now.
    std::fs::write(data.join("enabled"), b"1").unwrap();
    let daemon = Daemon {
        controller: Arc::new(Mutex::new(
            Controller::new(root, data.clone(), TestEngine::default()).unwrap(),
        )),
        operations: Arc::new(OperationManager::new(data.clone())),
        telegram: Arc::new(zapretd::telegram::Manager::new(data.clone())),
    };
    let DaemonResponse::Doctor(result) = daemon.dispatch(DaemonCommand::Doctor).await.unwrap()
    else {
        panic!("not a Doctor result");
    };
    let find = |name: &str| result.checks.iter().find(|c| c.name == name).unwrap();
    assert!(find("Системная служба").ok);
    assert!(!find("Движок").ok);
    assert!(!find("Файлы движка").ok);
    assert!(find("Другие программы обхода").ok);
}

#[tokio::test]
async fn composite_strategy_survives_a_service_restart() {
    use zapretd::selector::Backend;
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("bundle");
    let data = dir.path().join("data");
    std::fs::create_dir_all(&root).unwrap();
    let general = Strategy {
        id: "general".into(),
        name: "General".into(),
        description: String::new(),
        supported: true,
        arguments: vec!["--test".into()],
    };
    std::fs::write(
        root.join("strategies.json"),
        serde_json::to_vec(&vec![general.clone()]).unwrap(),
    )
    .unwrap();
    let mut controller =
        Controller::new(root.clone(), data.clone(), TestEngine::default()).unwrap();
    let composite = Strategy {
        id: zapret_core::composite::ID.into(),
        name: zapret_core::composite::NAME.into(),
        description: "основа general".into(),
        ..general.clone()
    };
    // A file with a bad argument is refused, not stored.
    let bad = Strategy {
        arguments: vec!["rm -rf".into()],
        ..composite.clone()
    };
    assert!(controller.set_composite(Some(bad)).is_err());
    controller.set_composite(Some(composite)).unwrap();
    controller.set_strategy("composite").await.unwrap();
    drop(controller);
    let mut controller = Controller::new(root, data.clone(), TestEngine::default()).unwrap();
    assert_eq!(controller.status().unwrap().strategy, "composite");
    assert_eq!(controller.strategies().unwrap().len(), 2);
    controller.set_composite(None).unwrap();
    assert!(!data.join("composite.json").exists());
}

#[tokio::test]
async fn update_app_only_when_a_download_waits() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("bundle");
    let data = dir.path().join("data");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join("strategies.json"),
        serde_json::to_vec(&vec![Strategy {
            id: "general".into(),
            name: "General".into(),
            description: String::new(),
            supported: true,
            arguments: vec!["--test".into()],
        }])
        .unwrap(),
    )
    .unwrap();
    let controller = Controller::new(root, data.clone(), TestEngine::default()).unwrap();
    let daemon = Daemon {
        controller: Arc::new(Mutex::new(controller)),
        operations: Arc::new(OperationManager::new(data.clone())),
        telegram: Arc::new(zapretd::telegram::Manager::new(data.clone())),
    };
    assert!(daemon.dispatch(DaemonCommand::UpdateApp).await.is_err());

    daemon.operations.app.lock().await.available = Some("0.2.0".into());
    {
        let _selection = daemon.operations.gate.lock().await;
        assert!(daemon.dispatch(DaemonCommand::UpdateApp).await.is_err());
    }
    daemon.dispatch(DaemonCommand::UpdateApp).await.unwrap();
    tokio::time::timeout(
        std::time::Duration::from_secs(1),
        daemon.operations.install_app.notified(),
    )
    .await
    .unwrap();
    let DaemonResponse::Snapshot(snapshot) =
        daemon.dispatch(DaemonCommand::Snapshot).await.unwrap()
    else {
        panic!()
    };
    assert_eq!(snapshot.app.available.as_deref(), Some("0.2.0"));
}
