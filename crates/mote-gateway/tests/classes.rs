use mote_gateway::classes::{allowed, Class, Scope};

#[test]
fn remote_allowlist_refuses_local_unknown_and_watch_control_calls() {
    assert_eq!(allowed("state", Scope::Watch).unwrap(), Class::Read);
    assert_eq!(allowed("ui.event", Scope::Full).unwrap(), Class::Control);
    assert!(allowed("ui.event", Scope::Watch).is_err());
    for method in [
        "gateway.pair_start",
        "gateway.device_revoke",
        "gateway.enable",
        "gateway.disable",
        "daemon.shutdown",
        "config.set",
        "unknown",
    ] {
        assert!(allowed(method, Scope::Full).is_err(), "{method}");
    }
}
