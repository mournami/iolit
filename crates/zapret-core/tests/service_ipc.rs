#![cfg(windows)]
//! Run explicitly on Windows after installing the generated development package.
use serde_json::json;
use zapret_core::ipc::request;

#[tokio::test]
#[ignore = "requires the installed Windows service"]
async fn service_rejects_invalid_commands_without_changing_state() {
    let before = request("status", json!({}))
        .await
        .expect("installed service must respond");
    assert_eq!(before["installed"], true);
    assert_eq!(
        before["running"], false,
        "clean installation must not enable interception"
    );
    let list = request("list", json!({})).await.unwrap();
    assert!(!list.as_array().unwrap().is_empty());
    assert!(
        request("use", json!({"name":"../../arbitrary.exe"}))
            .await
            .is_err()
    );
    assert!(
        request("add-domain", json!({"domain":"example.com\n--debug"}))
            .await
            .is_err()
    );
    assert!(
        request("execute", json!({"command":"anything"}))
            .await
            .is_err()
    );
    let after = request("status", json!({})).await.unwrap();
    assert_eq!(after["strategy"], before["strategy"]);
    assert_eq!(after["running"], false);
    // Exercise the real signed WinDivert driver in this disposable Windows VM.
    // Stop is attempted before asserting, so an unsupported driver cannot leave it enabled.
    let started = request("start", json!({})).await;
    let running = request("status", json!({})).await;
    let stopped = request("stop", json!({})).await;
    if started.is_err() {
        eprintln!(
            "engine startup log: {:?}",
            request("logtail", json!({})).await
        );
    }
    assert!(stopped.is_ok(), "must restore disabled state: {stopped:?}");
    assert!(
        started.is_ok(),
        "actual engine/driver start failed: {started:?}"
    );
    assert_eq!(running.unwrap()["running"], true);
    assert_eq!(
        request("status", json!({})).await.unwrap()["running"],
        false
    );
}
