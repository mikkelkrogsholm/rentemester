use super::*;
use crate::archive_import_state::tests::{package, review, scope};
use std::sync::atomic::{AtomicU64, Ordering};
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "rm-archive-attempt-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::DirBuilder::new().mode(0o700).create(&dir).unwrap();
        Self(dir)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
#[test]
fn private_marker_is_small_payload_free_preserved_and_no_overwrite_or_auto_clear() {
    let temp = Temp::new();
    let r = review();
    let attempt = Attempt::new(&r).unwrap();
    attempt.save(&temp.0).unwrap();
    attempt.save(&temp.0).unwrap();
    let bytes = fs::read(temp.0.join(FILE)).unwrap();
    let value: Value = serde_json::from_slice(&bytes).unwrap();
    assert!(bytes.len() < 4096);
    assert!(value.get("contentBase64").is_none());
    assert!(value.get("body").is_none());
    assert_eq!(value["bodySha256"], state::sha(r.body()));
    assert_eq!(
        fs::metadata(temp.0.join(FILE)).unwrap().mode() & 0o777,
        0o600
    );
    let loaded = Attempt::load(&temp.0).unwrap().unwrap();
    assert!(!loaded.scope.can_apply);
    assert!(loaded.matches(&scope()));
    assert_eq!(
        crate::archive_import_state::Review::recover(package(), scope(), &loaded)
            .unwrap()
            .body(),
        r.body()
    );
    let mut changed = attempt.clone();
    changed.idempotency_key = "another-key".into();
    assert!(changed.save(&temp.0).is_err());
    assert_eq!(fs::read(temp.0.join(FILE)).unwrap(), bytes);
    assert!(changed.finish(&temp.0).is_err());
    attempt.finish(&temp.0).unwrap();
    assert!(Attempt::load(&temp.0).unwrap().is_none());
}
#[test]
fn explicit_local_rebind_preserves_original_intent_and_remote_or_identity_changes_refuse() {
    let temp = Temp::new();
    let attempt = Attempt::new(&review()).unwrap();
    attempt.save(&temp.0).unwrap();
    let mut next = scope();
    next.origin = "http://127.0.0.1:32101".into();
    assert!(!attempt.matches(&next));
    assert!(attempt.can_rebind(&next));
    let rebound = attempt.rebind(&temp.0, &next).unwrap();
    assert_eq!(rebound.body_sha256, attempt.body_sha256);
    assert_eq!(rebound.plan_hash, attempt.plan_hash);
    assert_eq!(rebound.idempotency_key, attempt.idempotency_key);
    assert_eq!(rebound.summary, attempt.summary);
    assert!(rebound.matches(&next));
    next.principal = "another".into();
    assert!(rebound.rebind(&temp.0, &next).is_err());
    let mut remote = rebound.clone();
    remote.scope.local_identity = None;
    assert!(!remote.can_rebind(&next));
}
#[test]
fn corrupt_unsafe_symlink_and_hardlink_records_refuse_without_writes() {
    let temp = Temp::new();
    let attempt = Attempt::new(&review()).unwrap();
    attempt.save(&temp.0).unwrap();
    let path = temp.0.join(FILE);
    let bytes = fs::read(&path).unwrap();
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(Attempt::load(&temp.0).is_err());
    assert_eq!(fs::read(&path).unwrap(), bytes);
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    let link = temp.0.join("hardlink");
    fs::hard_link(&path, &link).unwrap();
    assert!(Attempt::load(&temp.0).is_err());
    fs::remove_file(link).unwrap();
    let target = temp.0.join("original");
    fs::rename(&path, &target).unwrap();
    std::os::unix::fs::symlink(&target, &path).unwrap();
    assert!(Attempt::load(&temp.0).is_err());
    fs::remove_file(&path).unwrap();
    fs::rename(target, &path).unwrap();
    let mut bad: Value = serde_json::from_slice(&bytes).unwrap();
    bad["summary"]["planHash"] = json!("0".repeat(64));
    fs::write(&path, bad.to_string()).unwrap();
    let corrupt = fs::read(&path).unwrap();
    assert!(Attempt::load(&temp.0).is_err());
    assert_eq!(fs::read(path).unwrap(), corrupt);
}
