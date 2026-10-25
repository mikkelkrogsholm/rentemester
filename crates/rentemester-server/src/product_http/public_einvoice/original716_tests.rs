use super::*;
#[test]
fn original716_config_changed_rotate_away_and_back_invalidates_dispatch() {
    fn client(key: &str) -> provider::Client {
        provider::original717::client_from_admitted_secret(
            &serde_json::to_vec(&json!({"apiLicenseKey":key,"environment":"test"})).unwrap(),
        )
        .unwrap()
    }
    let initial = client("synthetic-original716-a");
    let generation = initial.credential_generation();
    assert!(original716_generation_current(Some(&initial), generation));
    assert!(!original716_generation_current(None, generation));
    let changed = client("synthetic-original716-b");
    assert!(!original716_generation_current(Some(&changed), generation));
    let back = client("synthetic-original716-a");
    assert!(!original716_generation_current(Some(&back), generation));
}
