#![cfg(windows)]
//! Run explicitly on Windows after installing the generated development package.
use serde_json::json;
use zapret_core::{
    DaemonCommand, Report, Snapshot,
    ipc::{request_command, request_raw},
};

async fn snapshot() -> Snapshot {
    request_command(DaemonCommand::Snapshot)
        .await
        .expect("installed service must respond")
}

#[tokio::test]
#[ignore = "requires the installed Windows service"]
async fn service_rejects_invalid_commands_without_changing_state() {
    let before = snapshot().await;
    assert!(before.service.installed);
    assert!(
        !before.service.running,
        "clean installation must not enable interception"
    );
    assert!(!before.strategies.is_empty());
    assert!(
        request_command::<()>(DaemonCommand::SetStrategy {
            strategy_id: "../../arbitrary.exe".into()
        })
        .await
        .is_err()
    );
    assert!(
        request_command::<()>(DaemonCommand::AddDomain {
            domain: "example.com\n--debug".into()
        })
        .await
        .is_err()
    );
    assert!(
        request_raw(json!({"cmd":"execute", "args":{"command":"anything"}}))
            .await
            .is_err()
    );
    assert!(
        request_raw(json!({"cmd":"enable", "executable":"arbitrary.exe"}))
            .await
            .is_err()
    );
    let after = snapshot().await;
    assert_eq!(after.service.strategy, before.service.strategy);
    assert!(!after.service.running);
    // Exercise the real signed WinDivert driver in this disposable Windows VM.
    // Stop is attempted before asserting, so an unsupported driver cannot leave it enabled.
    let started = request_command::<()>(DaemonCommand::Enable).await;
    // The first enable of a clean installation also starts a selection, which
    // holds the service until it ends; stop it and wait for the restored state.
    if started.is_ok() && snapshot().await.operation.busy {
        let _ = request_command::<()>(DaemonCommand::CancelOperation).await;
        for _ in 0..120 {
            if !snapshot().await.operation.busy {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
    }
    let running = snapshot().await;
    let stopped = request_command::<()>(DaemonCommand::Disable).await;
    if started.is_err() {
        eprintln!(
            "engine startup log: {:?}",
            request_command::<Vec<String>>(DaemonCommand::Logs).await
        );
    }
    assert!(stopped.is_ok(), "must restore disabled state: {stopped:?}");
    assert!(
        started.is_ok(),
        "actual engine/driver start failed: {started:?}"
    );
    assert!(running.service.running);
    assert!(!snapshot().await.service.running);
    let report: Report = request_command(DaemonCommand::Report).await.unwrap();
    assert_eq!(report.strategy, before.service.strategy);
    assert!(!report.running);
}
