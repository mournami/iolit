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
}
