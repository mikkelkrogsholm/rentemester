use super::*;
use std::os::unix::fs::{PermissionsExt, symlink};
fn attempt() -> Attempt {
    let r = crate::opening_balance_state::tests::review();
    Attempt {
        origin: "http://127.0.0.1:1234".into(),
        company: "synthetic".into(),
        principal: r.before.principal.clone(),
        ledger_uuid: r.before.ledger.clone(),
        local_identity: Some("/private/synthetic-profile:workspace".into()),
        body: r.apply_body("original-opening-key").unwrap(),
        plan: r.plan,
    }
}
fn dir(name: &str) -> PathBuf {
    std::env::temp_dir().canonicalize().unwrap().join(format!(
        "rm-opening-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}
#[test]
fn exact_private_attempt_survives_restart_refuses_overwrite_and_only_explicit_rebind() {
    let dir = dir("restart");
    let original = attempt();
    original.save(&dir).unwrap();
    assert_eq!(fs::metadata(&dir).unwrap().mode() & 0o777, 0o700);
    assert_eq!(fs::metadata(dir.join(FILE)).unwrap().mode() & 0o777, 0o600);
    assert!(original.save(&dir).is_err());
    let loaded = Attempt::load(&dir).unwrap().unwrap();
    assert_eq!(loaded, original);
    assert!(!loaded.matches(
        &loaded.origin,
        &loaded.company,
        "restored-new-owner",
        &loaded.ledger_uuid,
        loaded.local_identity.as_deref()
    ));
    assert!(!loaded.can_rebind(
        &loaded.company,
        "restored-new-owner",
        &loaded.ledger_uuid,
        loaded.local_identity.as_deref()
    ));
    assert!(!loaded.can_rebind(
        &loaded.company,
        &loaded.principal,
        "different-ledger",
        loaded.local_identity.as_deref()
    ));
    assert!(!loaded.can_rebind(
        &loaded.company,
        &loaded.principal,
        &loaded.ledger_uuid,
        None
    ));
    let moved = loaded
        .rebind(
            &dir,
            "http://127.0.0.1:4321",
            &loaded.company,
            &loaded.principal,
            &loaded.ledger_uuid,
            loaded.local_identity.as_deref(),
        )
        .unwrap();
    assert_eq!(moved.body, original.body);
    assert_eq!(moved.plan, original.plan);
    assert!(original.finish(&dir).is_err());
    assert_eq!(Attempt::load(&dir).unwrap().unwrap(), moved);
    moved.finish(&dir).unwrap();
    assert!(Attempt::load(&dir).unwrap().is_none());
    fs::remove_dir(&dir).unwrap();
}
#[test]
fn corrupt_or_shared_or_symlink_status_stays_blocked_and_preserved() {
    let dir = dir("tamper");
    let original = attempt();
    original.save(&dir).unwrap();
    let path = dir.join(FILE);
    let bytes = fs::read(&path).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(Attempt::load(&dir).is_err());
    assert_eq!(fs::read(&path).unwrap(), bytes);
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    let mut value: Value = serde_json::from_slice(&bytes).unwrap();
    value["body"] = json!("{\"confirm\":true}");
    fs::write(&path, value.to_string()).unwrap();
    assert!(Attempt::load(&dir).is_err());
    assert!(original.finish(&dir).is_err());
    fs::remove_file(&path).unwrap();
    symlink(dir.join("nonexistent"), &path).unwrap();
    assert!(Attempt::load(&dir).is_err());
    fs::remove_file(&path).unwrap();
    fs::remove_dir(dir).unwrap();
}
