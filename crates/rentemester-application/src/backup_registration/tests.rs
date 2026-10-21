use super::*;
use rentemester_runtime::auth::{AuthError, Clock, CookieProfile, CookieVerifier};
use std::os::unix::fs::PermissionsExt;
struct Fixed;
impl Clock for Fixed {
    fn now_unix_ms(&self) -> Result<i64, AuthError> {
        Ok(1_767_226_200_000)
    }
}
fn add(target: Option<&str>) -> Request {
    serde_json::from_value(json!({"action":"add","operationId":"synthetic-add-operation","destinationId":"synthetic-declared-target","label":"Synthetic SSH registration","kind":if target.is_some(){"google-drive"}else{"ssh"},"location":"ssh://synthetic.example.invalid/inert-description","attestedBy":"Synthetic operator","inEeaOrEu":false,"nonRelatedParty":false,"operatorTargetId":target,"confirm":true})).unwrap()
}
fn fixture() -> (
    tempfile::TempDir,
    Connection,
    Connection,
    CookieVerifier,
    rentemester_runtime::auth::VerifiedCookie,
) {
    let home = std::path::PathBuf::from(std::env::var_os("HOME").unwrap());
    let parent = if cfg!(target_os = "macos") {
        home.join("Library/Application Support")
    } else {
        home
    };
    let dir = tempfile::Builder::new()
        .prefix(".rm-registration-test-")
        .tempdir_in(parent)
        .unwrap();
    std::fs::create_dir(dir.path().join(".rentemester")).unwrap();
    std::fs::set_permissions(
        dir.path().join(".rentemester"),
        std::fs::Permissions::from_mode(0o700),
    )
    .unwrap();
    std::fs::create_dir(dir.path().join("native-backups")).unwrap();
    std::fs::set_permissions(
        dir.path().join("native-backups"),
        std::fs::Permissions::from_mode(0o700),
    )
    .unwrap();
    let mut c = Connection::open(dir.path().join(".rentemester/workspace-control.sqlite")).unwrap();
    let mut l = Connection::open(dir.path().join("company.sqlite")).unwrap();
    rentemester_storage::fresh::initialize_empty_pair(&mut c, &mut l, "Synthetic registrations")
        .unwrap();
    c.execute_batch("INSERT INTO user(id,name,email,emailVerified,twoFactorEnabled,createdAt,updatedAt)VALUES('u','Synthetic','u@example.invalid',1,1,'2026-01-01','2026-01-01');INSERT INTO session(id,userId,token,createdAt,updatedAt,expiresAt)VALUES('s','u','synthetic-token','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z','2026-01-01T12:00:00.000Z');INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES('u','activate','workspace_owner','user:u');INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','grant','owner','user:u');").unwrap();
    let cookies = CookieVerifier::new(
        "w",
        CookieProfile::Secure,
        "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
    )
    .unwrap();
    let issued = cookies.issue_session("synthetic-token").unwrap();
    let proof = cookies.verify(Some(issued.request_header()), None).unwrap();
    (dir, c, l, cookies, proof)
}
#[test]
fn current_admin_adds_lists_reconciles_and_removes_without_ledger_or_provider_effects() {
    let (_dir, mut c, mut l, _cookies, proof) = fixture();
    let context = || Context {
        cookie: &proof,
        clock: &Fixed,
        selection: control_fence::CompanySelection {
            workspace: "w",
            slug: "a",
            registered: true,
            archived: false,
        },
    };
    let changes = l.total_changes();
    let input = add(None);
    let value = execute(&mut c, &mut l, context(), &[], Action::Apply(&input))
        .unwrap()
        .value;
    assert_eq!(value["workspaceId"], "w");
    assert_eq!(value["operation"]["actor"], "user:u");
    let total = c.total_changes();
    assert_eq!(
        execute(&mut c, &mut l, context(), &[], Action::Apply(&input))
            .unwrap()
            .value,
        value
    );
    assert_eq!(c.total_changes(), total);
    assert_eq!(
        execute(
            &mut c,
            &mut l,
            context(),
            &[],
            Action::Inspect {
                operation: input.operation_id()
            }
        )
        .unwrap()
        .value,
        value
    );
    assert_eq!(
        execute(&mut c, &mut l, context(), &[], Action::List)
            .unwrap()
            .value["destinations"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let remove = Request::Remove {
        operation_id: "synthetic-remove-operation".into(),
        destination_id: input.destination_id().into(),
        confirm: true,
    };
    execute(&mut c, &mut l, context(), &[], Action::Apply(&remove)).unwrap();
    assert!(
        execute(&mut c, &mut l, context(), &[], Action::List)
            .unwrap()
            .value["destinations"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(l.total_changes(), changes);
    c.execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','grant','reader','user:u')",[]).unwrap();
    assert_eq!(
        execute(
            &mut c,
            &mut l,
            context(),
            &[],
            Action::Inspect {
                operation: input.operation_id()
            }
        )
        .unwrap_err()
        .error
        .code,
        "access_denied"
    );
}
#[test]
fn unknown_operator_reference_and_unqualified_archive_do_not_append_receipts() {
    let (_dir, mut c, mut l, _cookies, proof) = fixture();
    let context = || Context {
        cookie: &proof,
        clock: &Fixed,
        selection: control_fence::CompanySelection {
            workspace: "w",
            slug: "a",
            registered: true,
            archived: false,
        },
    };
    let before = c.total_changes();
    assert!(
        execute(
            &mut c,
            &mut l,
            context(),
            &[],
            Action::Apply(&add(Some("unknown-operator-target")))
        )
        .is_err()
    );
    assert_eq!(c.total_changes(), before);
    let input = add(None);
    execute(&mut c, &mut l, context(), &[], Action::Apply(&input)).unwrap();
    let declaration = Request::Declare {
        operation_id: "synthetic-declare-operation".into(),
        destination_id: input.destination_id().into(),
        backup_id: "a".repeat(64),
        note: None,
        confirm: true,
    };
    let before = c.total_changes();
    assert!(execute(&mut c, &mut l, context(), &[], Action::Apply(&declaration)).is_err());
    assert_eq!(c.total_changes(), before);
    assert_eq!(
        execute(
            &mut c,
            &mut l,
            context(),
            &[],
            Action::Inspect {
                operation: declaration.operation_id()
            }
        )
        .unwrap()
        .value["operation"],
        Value::Null
    );
}
#[test]
fn history_replay_remains_inert_after_restore_without_retained_files() {
    let (_dir, mut c, mut l, _cookies, proof) = fixture();
    let context = || Context {
        cookie: &proof,
        clock: &Fixed,
        selection: control_fence::CompanySelection {
            workspace: "w",
            slug: "a",
            registered: true,
            archived: false,
        },
    };
    let input = add(None);
    execute(&mut c, &mut l, context(), &[], Action::Apply(&input)).unwrap();
    let declaration = Request::Declare {
        operation_id: "synthetic-declare-history".into(),
        destination_id: input.destination_id().into(),
        backup_id: "a".repeat(64),
        note: None,
        confirm: true,
    };
    let tx = c.transaction().unwrap();
    let b = store::ProtectedBackup {
        backup_id: "a".repeat(64),
        archive_sha256: "a".repeat(64),
        archive_size_bytes: 123,
        backup_created_at: "2026-01-01T00:00:00.000Z".into(),
    };
    let receipt = store::apply(
        &tx,
        "a",
        &declaration,
        "user:u",
        "2026-01-01T00:10:00.000Z",
        Some(&b),
    )
    .unwrap();
    tx.commit().unwrap();
    c.execute("INSERT INTO workspace_audit(event_type,entity_type,entity_id,actor)VALUES('native_portable_restore','backup','synthetic','system:restore')",[]).unwrap();
    let replay = execute(&mut c, &mut l, context(), &[], Action::Apply(&declaration))
        .unwrap()
        .value;
    assert_eq!(replay["operation"], serde_json::to_value(receipt).unwrap());
    assert_eq!(replay["operation"]["result"]["remoteSyncVerified"], false);
    assert!(
        execute(&mut c, &mut l, context(), &[], Action::List)
            .unwrap()
            .value["backups"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}
#[test]
fn missing_tampered_and_pre_restore_inventory_cannot_qualify_a_new_declaration() {
    let (dir, mut c, mut l, _cookies, proof) = fixture();
    let context = || Context {
        cookie: &proof,
        clock: &Fixed,
        selection: control_fence::CompanySelection {
            workspace: "w",
            slug: "a",
            registered: true,
            archived: false,
        },
    };
    let input = add(None);
    execute(&mut c, &mut l, context(), &[], Action::Apply(&input)).unwrap();
    let bytes = b"Synthetic invalid signature/archive bytes";
    let hash = rentemester_runtime::backup_keys::digest(bytes);
    let public = vec![1u8; 32];
    let uuid = l
        .query_row(
            "SELECT ledger_uuid FROM ledger_identity WHERE id=1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let snapshot = rentemester_storage::backup_inventory::Snapshot {
        version: 1,
        workspace_id: "w".into(),
        archive_sha256: hash.clone(),
        archive_size_bytes: bytes.len() as u64,
        signing_public_key_sha256: rentemester_runtime::backup_keys::digest(&public),
        signing_public_key: Some(public),
        created_at: "2026-01-01T00:00:00.000Z".into(),
        created_at_ms: 1_767_225_600_000,
        companies: vec![rentemester_storage::backup_inventory::Company {
            slug: "a".into(),
            ledger_uuid: uuid,
            snapshot_sha256: "a".repeat(64),
            activity_max_ids: Some([0, 0, 0]),
        }],
    };
    let tx = c.transaction().unwrap();
    rentemester_storage::backup_inventory::append(&tx, &snapshot, "user:u").unwrap();
    tx.commit().unwrap();
    let declaration = Request::Declare {
        operation_id: "synthetic-declare-invalid".into(),
        destination_id: input.destination_id().into(),
        backup_id: hash.clone(),
        note: None,
        confirm: true,
    };
    let missing = execute(&mut c, &mut l, context(), &[], Action::List)
        .unwrap()
        .value;
    assert!(missing["backups"].as_array().unwrap().is_empty());
    assert_eq!(missing["errors"].as_array().unwrap().len(), 1);
    assert!(execute(&mut c, &mut l, context(), &[], Action::Apply(&declaration)).is_err());
    let path = dir
        .path()
        .join("native-backups")
        .join(format!("{hash}.native-backup.json"));
    std::fs::write(&path, bytes).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
    let tampered = execute(&mut c, &mut l, context(), &[], Action::List)
        .unwrap()
        .value;
    assert!(tampered["backups"].as_array().unwrap().is_empty());
    assert_eq!(tampered["errors"].as_array().unwrap().len(), 1);
    assert!(execute(&mut c, &mut l, context(), &[], Action::Apply(&declaration)).is_err());
    c.execute("INSERT INTO workspace_audit(event_type,entity_type,entity_id,actor)VALUES('native_portable_restore','backup','synthetic','system:restore')",[]).unwrap();
    let restored = execute(&mut c, &mut l, context(), &[], Action::List)
        .unwrap()
        .value;
    assert!(restored["backups"].as_array().unwrap().is_empty());
    assert!(restored["errors"].as_array().unwrap().is_empty());
    assert_eq!(restored["destinations"].as_array().unwrap().len(), 1);
}
