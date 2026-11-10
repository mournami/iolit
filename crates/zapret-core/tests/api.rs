use serde_json::json;
use zapret_core::DaemonCommand;

#[test]
fn fixed_commands_reject_unknown_fields_and_executable_paths() {
    for request in [
        json!({"cmd":"execute","args":{"command":"winws.exe"}}),
        json!({"cmd":"enable","executable":"winws.exe"}),
        json!({"cmd":"set_strategy","args":{"strategy_id":"general","path":"winws.exe"}}),
        json!({"cmd":"report","args":{"path":"outside.json"}}),
    ] {
        assert!(
            serde_json::from_value::<DaemonCommand>(request.clone()).is_err(),
            "{request}"
        );
    }
    let command: DaemonCommand =
        serde_json::from_value(json!({"cmd":"set_strategy","args":{"strategy_id":"general"}}))
            .unwrap();
    assert!(
        matches!(command, DaemonCommand::SetStrategy { strategy_id } if strategy_id == "general")
    );
}
