use super::*;
use rentemester_runtime::{
    auth::{AuthError, CookieProfile},
    backup_artifact as artifact,
};
use std::cell::RefCell;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
struct Fixed(i64);
impl Clock for Fixed {
    fn now_unix_ms(&self) -> Result<i64, AuthError> {
        Ok(self.0)
    }
}
const NOW: i64 = 1_767_226_200_000;
struct Fixture {
    _directory: tempfile::TempDir,
    control: RefCell<Connection>,
    cookies: CookieVerifier,
    proof: VerifiedCookie,
    catalog: Catalog,
    trusted: artifact::TrustedPublicKey,
    configs: Vec<destination::Config>,
    bindings: Vec<Binding>,
    uid: u32,
}
impl Fixture {
    fn runtime(&self) -> crate::backup_destination::Runtime<'_> {
        crate::backup_destination::Runtime {
            cookies: &self.cookies,
            clock: &Fixed(NOW),
            configs: &self.configs,
            uid: self.uid,
            trusted: &self.trusted,
        }
    }
    fn request(&self) -> Request {
        Request {
            destination_id: self.configs[0].id.clone(),
            operation_id: "synthetic-remote-operation".into(),
            placement_operation_id: "synthetic-placement-operation".into(),
            remote_object_id: "Synthetic_Drive_Object".into(),
        }
    }
}
fn fixture() -> Fixture {
    let parent = std::path::PathBuf::from(std::env::var_os("HOME").unwrap());
    let parent = if cfg!(target_os = "macos") {
        parent.join("Library/Application Support")
    } else {
        parent
    };
    let directory = tempfile::Builder::new()
        .prefix(".rm-remote-receipt-test-")
        .tempdir_in(parent)
        .unwrap();
    let path = directory.path().canonicalize().unwrap();
    let uid = std::fs::metadata(&path).unwrap().uid();
    let destination_path = path.join("destination");
    std::fs::create_dir(&destination_path).unwrap();
    std::fs::set_permissions(&destination_path, std::fs::Permissions::from_mode(0o700)).unwrap();
    let mut control = Connection::open(path.join("control.sqlite")).unwrap();
    let mut company = Connection::open(path.join("company.sqlite")).unwrap();
    rentemester_storage::fresh::initialize_empty_pair(
        &mut control,
        &mut company,
        "Synthetic remote backup",
    )
    .unwrap();
    control.execute_batch("INSERT INTO user(id,name,email,emailVerified,twoFactorEnabled,createdAt,updatedAt)VALUES('u','Synthetic','u@example.invalid',1,1,'2026-01-01','2026-01-01');INSERT INTO twoFactor(id,secret,backupCodes,userId,verified)VALUES('f','synthetic','synthetic','u',1);INSERT INTO rm_workspace_mfa_events(user_id,event_type,actor,created_at)VALUES('u','mfa_enabled','system:better-auth','2026-01-01T00:00:00.000Z');INSERT INTO session(id,expiresAt,token,createdAt,updatedAt,userId)VALUES('s','2026-01-01T12:00:00.000Z','synthetic-token','2026-01-01T00:01:00.000Z','2026-01-01T00:01:00.000Z','u');INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES('u','activate','workspace_owner','system:synthetic');INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','grant','owner','system:synthetic');").unwrap();
    let cookies = CookieVerifier::new(
        "w",
        CookieProfile::Secure,
        "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
    )
    .unwrap();
    let issued = cookies.issue_session("synthetic-token").unwrap();
    let proof = cookies.verify(Some(issued.request_header()), None).unwrap();
    let catalog = Catalog {
        workspace: "w".into(),
        companies: vec![rentemester_storage::workspace_context::RegisteredCompany {
            slug: "a".into(),
            name: "Synthetic".into(),
            archived: false,
        }],
    };
    // Deliberately synthetic deterministic signing material; never a production credential.
    let der = [
        48, 81, 2, 1, 1, 48, 5, 6, 3, 43, 101, 112, 4, 34, 4, 32, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7,
        7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 129, 33, 0, 234, 74, 108,
        99, 226, 156, 82, 10, 190, 245, 80, 123, 19, 46, 197, 249, 149, 71, 118, 174, 190, 190,
        123, 146, 66, 30, 234, 105, 20, 70, 210, 44,
    ];
    let key = artifact::SigningKey::from_pkcs8(&der).unwrap();
    let trusted = artifact::TrustedPublicKey::from_bytes(&key.public_key_bytes()).unwrap();
    let ctx = artifact::Context {
        workspace_id: "w".into(),
        created_at: "2026-01-01T00:00:00.000Z".into(),
        control_schema_version: 31,
        companies: vec![artifact::Company {
            slug: "a".into(),
            ledger_uuid: "a".repeat(32),
            schema_version: 36,
        }],
        coverage: artifact::Coverage {
            included: vec![artifact::BusinessSection::WorkspaceAudit],
            unsupported: artifact::BusinessSection::ALL
                .into_iter()
                .filter(|s| *s != artifact::BusinessSection::WorkspaceAudit)
                .map(|section| artifact::UnsupportedSection {
                    section,
                    record_count: 0,
                })
                .collect(),
            excluded: artifact::Exclusion::ALL.to_vec(),
        },
    };
    // Runtime signature/inventory tests use opaque bytes, not a SQLite semantics claim.
    let wire = artifact::create(
        ctx,
        vec![
            artifact::Snapshot {
                kind: artifact::EntryKind::WorkspaceManifest,
                path: "workspace.json".into(),
                bytes: b"synthetic signed namespace".to_vec(),
            },
            artifact::Snapshot {
                kind: artifact::EntryKind::CompanyLedger {
                    company: "a".into(),
                },
                path: "a/data/ledger.sqlite".into(),
                bytes: b"synthetic signed unit ledger".to_vec(),
            },
            artifact::Snapshot {
                kind: artifact::EntryKind::ControlBusiness {
                    section: artifact::BusinessSection::WorkspaceAudit,
                },
                path: "control/workspace-audit.json".into(),
                bytes: b"{}".to_vec(),
            },
        ],
        &key,
    )
    .unwrap();
    let config = destination::Config {
        id: "synthetic-google-drive".into(),
        label: "Synthetic Drive folder".into(),
        kind: "google-drive".into(),
        directory: destination_path,
        non_related_party: false,
        in_eea_or_eu: false,
        country: "DK".into(),
        meets_recognised_standards: false,
        attested_by: "Synthetic operator".into(),
        attested_at: "2026-01-01T00:00:00Z".into(),
    };
    let bindings = vec![Binding {
        destination_id: config.id.clone(),
        folder_id: "Synthetic_Drive_Folder".into(),
    }];
    let f = Fixture {
        _directory: directory,
        control: RefCell::new(control),
        cookies,
        proof,
        catalog,
        trusted,
        configs: vec![config],
        bindings,
        uid,
    };
    crate::backup_destination::execute(
        &mut f.control.borrow_mut(),
        &f.proof,
        &f.catalog,
        &f.runtime(),
        crate::backup_destination::Action::Place {
            destination: &f.configs[0].id,
            operation: "synthetic-placement-operation",
            wire: &wire,
        },
    )
    .unwrap();
    f
}
fn ready(f: &Fixture) -> Prepared {
    let local = f.runtime();
    let runtime = Runtime {
        local: &local,
        bindings: &f.bindings,
    };
    match prepare(
        &mut f.control.borrow_mut(),
        &f.proof,
        &f.catalog,
        &runtime,
        &f.request(),
    )
    .unwrap()
    {
        Preparation::Ready(value) => *value,
        Preparation::Complete(_) => panic!("fresh operation expected"),
    }
}
fn observation(p: &Prepared) -> Observation {
    Observation {
        object_id: p.expected.object_id.clone(),
        folder_id: p.expected.folder_id.clone(),
        file_name: p.expected.file_name.clone(),
        archive_sha256: p.expected.archive_sha256.clone(),
        archive_size_bytes: p.expected.archive_size_bytes,
        observed_at_ms: NOW,
    }
}
#[test]
fn completed_replay_and_reconciliation_are_exact_without_local_file_or_provider_config() {
    let f = fixture();
    let p = ready(&f);
    assert!(f.control.borrow().is_autocommit());
    let local = f.runtime();
    let runtime = Runtime {
        local: &local,
        bindings: &f.bindings,
    };
    let value = finish(
        &mut f.control.borrow_mut(),
        &f.proof,
        &f.catalog,
        &runtime,
        &p,
        &observation(&p),
    )
    .unwrap();
    assert_eq!(value["verification"]["remoteSyncVerified"], true);
    assert_eq!(
        value["verification"]["attestationIndependentlyVerified"],
        false
    );
    std::fs::remove_file(
        f.configs[0]
            .directory
            .join("synthetic-placement-operation.native-backup.json"),
    )
    .unwrap();
    let empty = crate::backup_destination::Runtime {
        configs: &[],
        ..f.runtime()
    };
    let removed = Runtime {
        local: &empty,
        bindings: &[],
    };
    let before = f.control.borrow().total_changes();
    match prepare(
        &mut f.control.borrow_mut(),
        &f.proof,
        &f.catalog,
        &removed,
        &f.request(),
    )
    .unwrap()
    {
        Preparation::Complete(v) => assert_eq!(v, value),
        Preparation::Ready(_) => panic!("must not request another provider read"),
    }
    assert_eq!(
        inspect(
            &mut f.control.borrow_mut(),
            &f.proof,
            &f.catalog,
            &f.cookies,
            &Fixed(NOW),
            &p.request.destination_id,
            &p.request.operation_id
        )
        .unwrap(),
        value
    );
    assert_eq!(f.control.borrow().total_changes(), before);
    let mut conflict = f.request();
    conflict.remote_object_id = "Other_Object".into();
    assert!(
        prepare(
            &mut f.control.borrow_mut(),
            &f.proof,
            &f.catalog,
            &removed,
            &conflict
        )
        .is_err()
    );
}
#[test]
fn finish_rechecks_current_authority_and_observation_then_records_no_failed_receipt() {
    let f = fixture();
    let p = ready(&f);
    let local = f.runtime();
    let runtime = Runtime {
        local: &local,
        bindings: &f.bindings,
    };
    let before = f.control.borrow().total_changes();
    let mut bad = observation(&p);
    bad.archive_sha256 = "f".repeat(64);
    assert!(
        finish(
            &mut f.control.borrow_mut(),
            &f.proof,
            &f.catalog,
            &runtime,
            &p,
            &bad
        )
        .is_err()
    );
    let mut stale = observation(&p);
    stale.observed_at_ms -= 300_001;
    assert!(
        finish(
            &mut f.control.borrow_mut(),
            &f.proof,
            &f.catalog,
            &runtime,
            &p,
            &stale
        )
        .is_err()
    );
    let mut future = observation(&p);
    future.observed_at_ms += 1;
    assert!(
        finish(
            &mut f.control.borrow_mut(),
            &f.proof,
            &f.catalog,
            &runtime,
            &p,
            &future
        )
        .is_err()
    );
    assert_eq!(f.control.borrow().total_changes(), before);
    f.control.borrow().execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','grant','reader','system:synthetic')").unwrap();
    assert_eq!(
        finish(
            &mut f.control.borrow_mut(),
            &f.proof,
            &f.catalog,
            &runtime,
            &p,
            &observation(&p)
        )
        .unwrap_err()
        .error
        .code,
        "access_denied"
    );
}
#[test]
fn missing_placement_audit_new_restore_boundary_and_changed_operator_binding_refuse() {
    let f = fixture();
    let p = ready(&f);
    let local = f.runtime();
    let changed = [Binding {
        destination_id: f.configs[0].id.clone(),
        folder_id: "Changed_Folder".into(),
    }];
    let runtime = Runtime {
        local: &local,
        bindings: &changed,
    };
    assert!(
        finish(
            &mut f.control.borrow_mut(),
            &f.proof,
            &f.catalog,
            &runtime,
            &p,
            &observation(&p)
        )
        .is_err()
    );
    let runtime = Runtime {
        local: &local,
        bindings: &f.bindings,
    };
    f.control.borrow().execute_batch("INSERT INTO workspace_audit(event_type,entity_type,entity_id,actor)VALUES('native_portable_restore','backup','synthetic','system:restore')").unwrap();
    assert!(
        finish(
            &mut f.control.borrow_mut(),
            &f.proof,
            &f.catalog,
            &runtime,
            &p,
            &observation(&p)
        )
        .is_err()
    );
    assert!(
        prepare(
            &mut f.control.borrow_mut(),
            &f.proof,
            &f.catalog,
            &runtime,
            &f.request()
        )
        .is_err()
    );
}

#[test]
fn every_registered_company_admin_is_checked_before_initial_local_read() {
    let mut f = fixture();
    f.catalog
        .companies
        .push(rentemester_storage::workspace_context::RegisteredCompany {
            slug: "archived-b".into(),
            name: "Synthetic archived company".into(),
            archived: true,
        });
    std::fs::remove_file(
        f.configs[0]
            .directory
            .join("synthetic-placement-operation.native-backup.json"),
    )
    .unwrap();
    let local = f.runtime();
    let runtime = Runtime {
        local: &local,
        bindings: &f.bindings,
    };
    let failure = prepare(
        &mut f.control.borrow_mut(),
        &f.proof,
        &f.catalog,
        &runtime,
        &f.request(),
    )
    .err()
    .unwrap();
    assert_eq!(failure.error.code, "access_denied");
    assert!(f.control.borrow().is_autocommit());
}
#[test]
fn unsigned_placement_history_and_changed_local_bytes_cannot_become_remote_receipts() {
    let f = fixture();
    let p = ready(&f);
    let local = f.runtime();
    let runtime = Runtime {
        local: &local,
        bindings: &f.bindings,
    };
    let original = f.configs[0]
        .directory
        .join("synthetic-placement-operation.native-backup.json");
    let unconfirmed = f.configs[0]
        .directory
        .join("unconfirmed-placement.native-backup.json");
    std::fs::copy(&original, &unconfirmed).unwrap();
    let mut request = f.request();
    request.placement_operation_id = "unconfirmed-placement".into();
    assert_eq!(
        prepare(
            &mut f.control.borrow_mut(),
            &f.proof,
            &f.catalog,
            &runtime,
            &request
        )
        .err()
        .unwrap()
        .error
        .code,
        "backup_remote_placement_unconfirmed"
    );
    let bytes = std::fs::read(&original).unwrap();
    let mut changed = bytes.clone();
    changed[0] ^= 1;
    std::fs::write(&original, changed).unwrap();
    assert!(
        finish(
            &mut f.control.borrow_mut(),
            &f.proof,
            &f.catalog,
            &runtime,
            &p,
            &observation(&p)
        )
        .is_err()
    );
    std::fs::write(&original, bytes).unwrap();
    assert_eq!(
        inspect(
            &mut f.control.borrow_mut(),
            &f.proof,
            &f.catalog,
            &f.cookies,
            &Fixed(NOW),
            &p.request.destination_id,
            &p.request.operation_id
        )
        .unwrap()["verification"],
        Value::Null
    );
}
