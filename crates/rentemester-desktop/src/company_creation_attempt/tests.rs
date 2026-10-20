use super::*;
use crate::company_creation_state::tests as state;
use std::os::unix::fs::{PermissionsExt, symlink};
fn attempt() -> Attempt {
    Attempt {
        origin: "http://127.0.0.1:1234".into(),
        principal: state::snapshot().principal,
        local_identity: Some("/private/synthetic-profile:workspace".into()),
        body: state::review().bytes,
        receipt: None,
    }
}
fn dir() -> PathBuf {
    std::env::temp_dir().canonicalize().unwrap().join(format!(
        "rm-create-state-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}
#[test]
fn no_retry_marker_survives_restart_then_knownreceipt_is_durable_before_reads() {
    let d = dir();
    let a = attempt();
    a.save(&d).unwrap();
    assert!(a.save(&d).is_err());
    assert_eq!(Attempt::load(&d).unwrap().unwrap(), a);
    assert!(a.finish(&d).is_err());
    let known = a.record_receipt(&d, &state::receipt()).unwrap();
    assert_eq!(known.body, a.body);
    assert_eq!(
        Attempt::load(&d).unwrap().unwrap().receipt,
        Some(state::receipt())
    );
    assert!(a.finish(&d).is_err());
    let reviewed = state::review();
    assert_eq!(
        reviewed
            .canonical(
                known.receipt.as_ref().unwrap(),
                &state::catalog(),
                &state::profile()
            )
            .unwrap(),
        "synthetic-new"
    );
    known.finish(&d).unwrap();
    assert!(Attempt::load(&d).unwrap().is_none());
    let archives = fs::read_dir(&d)
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(archives.len(), 1);
    let value: Value = serde_json::from_slice(&fs::read(archives[0].path()).unwrap()).unwrap();
    assert_eq!(value["attempt"]["body"], String::from_utf8(a.body).unwrap());
    assert_eq!(value["attempt"]["origin"], a.origin);
    fs::remove_dir_all(d).unwrap();
}
#[test]
fn explicit_managed_read_resolution_keeps_unknown_evidence_and_never_confirms_creation() {
    let d = dir();
    let a = attempt();
    a.save(&d).unwrap();
    assert!(!a.matches(
        "http://127.0.0.1:4321",
        &a.principal,
        a.local_identity.as_deref()
    ));
    assert!(a.can_resolve_at(
        "http://127.0.0.1:4321",
        &a.principal,
        a.local_identity.as_deref()
    ));
    assert!(!a.can_resolve_at(
        "http://other",
        "different-owner",
        a.local_identity.as_deref()
    ));
    assert!(!a.can_resolve_at("http://other", &a.principal, None));
    assert!(!a.can_resolve_at("http://other", &a.principal, Some("different-workspace")));
    let mut now = state::snapshot();
    now.catalog = state::catalog();
    a.acknowledge(
        &d,
        "Reviewed server catalog manually",
        &now,
        "http://127.0.0.1:4321",
        a.local_identity.as_deref(),
    )
    .unwrap();
    let file = fs::read_dir(&d).unwrap().next().unwrap().unwrap().path();
    let value: Value = serde_json::from_slice(&fs::read(file).unwrap()).unwrap();
    assert_eq!(value["createdCompanyConfirmed"], false);
    assert_eq!(value["attempt"]["origin"], a.origin);
    assert_eq!(value["observedOrigin"], "http://127.0.0.1:4321");
    assert!(value["attempt"]["receipt"].is_null());
    fs::remove_dir_all(d).unwrap();
}
#[test]
fn corrupt_shared_symlink_and_crossprincipal_records_block_without_deleting() {
    let d = dir();
    let a = attempt();
    a.save(&d).unwrap();
    let path = d.join(FILE);
    let bytes = fs::read(&path).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(Attempt::load(&d).is_err());
    assert_eq!(fs::read(&path).unwrap(), bytes);
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    let mut other = state::snapshot();
    other.principal = "another-owner".into();
    assert!(
        a.acknowledge(
            &d,
            "Reviewed but wrong principal",
            &other,
            &a.origin,
            a.local_identity.as_deref()
        )
        .is_err()
    );
    let mut value: Value = serde_json::from_slice(&bytes).unwrap();
    value["body"] = json!("{}");
    fs::write(&path, value.to_string()).unwrap();
    assert!(Attempt::load(&d).is_err());
    assert!(a.finish(&d).is_err());
    fs::remove_file(&path).unwrap();
    symlink(d.join("missing"), &path).unwrap();
    assert!(Attempt::load(&d).is_err());
    fs::remove_file(path).unwrap();
    fs::remove_dir(d).unwrap();
}
#[test]
fn only_definite_original_post_refusals_can_clear_without_a_receipt() {
    let d = dir();
    let a = attempt();
    a.save(&d).unwrap();
    for code in [200, 201, 408, 429, 500, 503] {
        assert!(a.definite_refusal(&d, code).is_err());
        assert_eq!(Attempt::load(&d).unwrap().as_ref(), Some(&a));
    }
    a.definite_refusal(&d, 403).unwrap();
    assert!(Attempt::load(&d).unwrap().is_none());
    let file = fs::read_dir(&d).unwrap().next().unwrap().unwrap().path();
    let value: Value = serde_json::from_slice(&fs::read(file).unwrap()).unwrap();
    assert_eq!(value["httpStatus"], 403);
    assert_eq!(value["knownNoAcceptance"], true);
    assert_eq!(value["attempt"]["body"], String::from_utf8(a.body).unwrap());
    fs::remove_dir_all(d).unwrap();
}
