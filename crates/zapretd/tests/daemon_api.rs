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
