use super::*;
use rentemester_runtime::{
    auth::{AuthError, Clock, CookieProfile, CookieVerifier, VerifiedCookie},
    backup_artifact as artifact,
};
use rentemester_storage::{backup_inventory, backup_remote};
use std::os::unix::fs::PermissionsExt;
struct Fixed;
impl Clock for Fixed {
    fn now_unix_ms(&self) -> Result<i64, AuthError> {
        Ok(1_767_226_200_000)
    }
}
struct Fixture {
    root: tempfile::TempDir,
    control: Connection,
    company: Connection,
    cookies: CookieVerifier,
    proof: VerifiedCookie,
    configs: Vec<Config>,
    key: artifact::SigningKey,
    uuid: String,
}
impl Fixture {
    fn run(&mut self) -> Result<Committed<Value>, Failure> {
        let _ = &self.cookies;
        execute(
            &mut self.control,
            &mut self.company,
            Context {
                cookie: &self.proof,
                clock: &Fixed,
                selection: control_fence::CompanySelection {
                    workspace: "w",
                    slug: "a",
                    registered: true,
                    archived: false,
                },
            },
            &self.configs,
        )
    }
    fn retain(&mut self, at: &str) -> (String, u64) {
        let context = artifact::Context {
            workspace_id: "w".into(),
            created_at: at.into(),
            control_schema_version: 31,
            companies: vec![artifact::Company {
                slug: "a".into(),
                ledger_uuid: self.uuid.clone(),
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
        // Signature/inventory fixture only: opaque unit bytes do not claim full SQLite restore qualification.
        let bytes = artifact::create(
            context,
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
            &self.key,
        )
        .unwrap();
        let hash = rentemester_runtime::backup_keys::digest(&bytes);
        let path = self
            .root
            .path()
            .join("native-backups")
            .join(format!("{hash}.native-backup.json"));
        std::fs::write(&path, &bytes).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
        let stamp = if at == "2026-01-01T00:00:00.000Z" {
            1_767_225_600_000
        } else {
            1_767_225_660_000
        };
        let public = self.key.public_key_bytes();
        let snapshot = backup_inventory::Snapshot {
            version: 1,
            workspace_id: "w".into(),
            archive_sha256: hash.clone(),
            archive_size_bytes: bytes.len() as u64,
            signing_public_key_sha256: rentemester_runtime::backup_keys::digest(&public),
            signing_public_key: Some(public.to_vec()),
            created_at: at.into(),
            created_at_ms: stamp,
            companies: vec![backup_inventory::Company {
                slug: "a".into(),
                ledger_uuid: self.uuid.clone(),
                snapshot_sha256: rentemester_runtime::backup_keys::digest(
                    b"synthetic signed unit ledger",
                ),
                activity_max_ids: Some([0, 0, 0]),
            }],
        };
        let tx = self.control.transaction().unwrap();
        backup_inventory::append(&tx, &snapshot, "user:u").unwrap();
        tx.commit().unwrap();
        (hash, bytes.len() as u64)
    }
    fn evidence(&mut self, hash: &str, size: u64) {
        let placement = json!({"format":"rentemester-native-backup-placement/1","destinationId":"synthetic-drive","operationId":"synthetic-placement","archiveSha256":hash,"archiveSizeBytes":size,"backupCreatedAt":"2026-01-01T00:00:00.000Z","verified":true,"verifyMethod":"sha256-reread","signatureVerified":true,"remoteSyncVerified":false,"destination":self.configs[0].view(),"placedBy":"user:u","placedAt":"2026-01-01T00:01:00.000Z"});
        let tx = self.control.transaction().unwrap();
        tx.execute("INSERT INTO workspace_audit(event_type,entity_type,entity_id,actor)VALUES('native_backup_placed','native_backup_placement',?1,'user:u')",[placement.to_string()]).unwrap();
        let receipt = backup_remote::Receipt {
            format: backup_remote::FORMAT.into(),
            destination_id: "synthetic-drive".into(),
            operation_id: "synthetic-verification".into(),
            placement_operation_id: "synthetic-placement".into(),
            remote_object_id: "Synthetic_Object".into(),
            archive_sha256: hash.into(),
            archive_size_bytes: size,
            backup_created_at: "2026-01-01T00:00:00.000Z".into(),
            provider: "google-drive".into(),
            remote_file_name: "synthetic-placement.native-backup.json".into(),
            remote_parent_id: "Synthetic_Folder".into(),
            observed_at: "2026-01-01T00:02:00.000Z".into(),
            verified_at: "2026-01-01T00:03:00.000Z".into(),
            verified_by: "user:u".into(),
            remote_sync_verified: true,
            verify_method: "google-drive-metadata-and-sha256-reread".into(),
            signature_verified: true,
            attestation_independently_verified: false,
        };
        backup_remote::append(&tx, &receipt, "user:u").unwrap();
        tx.commit().unwrap();
    }
}
fn fixture() -> Fixture {
    let home = std::path::PathBuf::from(std::env::var_os("HOME").unwrap());
    let parent = if cfg!(target_os = "macos") {
        home.join("Library/Application Support")
    } else {
        home
    };
    let root = tempfile::Builder::new()
        .prefix(".rm-governance-test-")
        .tempdir_in(parent)
        .unwrap();
    for path in [
        root.path().join(".rentemester"),
        root.path().join("a"),
        root.path().join("a/data"),
        root.path().join("native-backups"),
    ] {
        std::fs::create_dir(&path).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let mut control =
        Connection::open(root.path().join(".rentemester/workspace-control.sqlite")).unwrap();
    let mut company = Connection::open(root.path().join("a/data/ledger.sqlite")).unwrap();
    rentemester_storage::fresh::initialize_empty_pair(
        &mut control,
        &mut company,
        "Synthetic governance",
    )
    .unwrap();
    control.execute_batch("INSERT INTO user(id,name,email,emailVerified,twoFactorEnabled,createdAt,updatedAt)VALUES('u','Synthetic','u@example.invalid',1,1,'2026-01-01','2026-01-01');INSERT INTO session(id,userId,token,createdAt,updatedAt,expiresAt)VALUES('s','u','synthetic-token','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z','2026-01-01T12:00:00.000Z');INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES('u','activate','workspace_owner','user:u');INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','grant','owner','user:u');").unwrap();
    let cookies = CookieVerifier::new(
        "w",
        CookieProfile::Secure,
        "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
    )
    .unwrap();
    let issued = cookies.issue_session("synthetic-token").unwrap();
    let proof = cookies.verify(Some(issued.request_header()), None).unwrap();
    let der = [
        48, 81, 2, 1, 1, 48, 5, 6, 3, 43, 101, 112, 4, 34, 4, 32, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7,
        7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 129, 33, 0, 234, 74, 108,
        99, 226, 156, 82, 10, 190, 245, 80, 123, 19, 46, 197, 249, 149, 71, 118, 174, 190, 190,
        123, 146, 66, 30, 234, 105, 20, 70, 210, 44,
    ];
    let key = artifact::SigningKey::from_pkcs8(&der).unwrap();
    let uuid = company
        .query_row(
            "SELECT ledger_uuid FROM ledger_identity WHERE id=1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let configs = vec![Config {
        id: "synthetic-drive".into(),
        label: "Synthetic operator declaration".into(),
        kind: "google-drive".into(),
        directory: root.path().join("operator-private-path-must-not-leak"),
        non_related_party: true,
        in_eea_or_eu: true,
        country: "DK".into(),
        meets_recognised_standards: true,
        attested_by: "Synthetic operator".into(),
        attested_at: "2026-01-01T00:00:00Z".into(),
    }];
    Fixture {
        root,
        control,
        company,
        cookies,
        proof,
        configs,
        key,
        uuid,
    }
}
#[test]
fn exact_current_view_joins_historical_observation_and_separates_operator_declarations() {
    let mut f = fixture();
    let (hash, size) = f.retain("2026-01-01T00:00:00.000Z");
    f.evidence(&hash, size);
    let changes = (f.control.total_changes(), f.company.total_changes());
    let value = f.run().unwrap().value;
    assert_eq!(value["backup"]["latestBackupId"], hash);
    assert_eq!(
        value["governance"]["latestBackupExternalCopyObserved"],
        true
    );
    assert_eq!(
        value["governance"]["latestBackupExternalCopyAtDeclaredDestination"],
        true
    );
    assert_eq!(
        value["governance"]["attestationIndependentlyVerified"],
        false
    );
    assert_eq!(value["governance"]["recoveryVerified"], false);
    assert_eq!(
        value["governance"]["checkedAt"],
        value["backup"]["checkedAt"]
    );
    assert_eq!(
        value["governance"]["destinations"][0]["remoteVerifications"][0]["observedAt"],
        "2026-01-01T00:02:00.000Z"
    );
    assert!(
        !value
            .to_string()
            .contains("operator-private-path-must-not-leak")
    );
    assert_eq!(
        (f.control.total_changes(), f.company.total_changes()),
        changes
    );
    let (new_hash, _) = f.retain("2026-01-01T00:01:00.000Z");
    let newer = f.run().unwrap().value;
    assert_eq!(newer["backup"]["latestBackupId"], new_hash);
    assert_eq!(
        newer["governance"]["latestBackupRemoteVerificationCount"],
        0
    );
    assert_eq!(
        newer["governance"]["latestBackupExternalCopyObserved"],
        false
    );
}
#[test]
fn missing_tampered_or_old_instance_local_copy_never_claims_external_copy() {
    let mut f = fixture();
    let (hash, size) = f.retain("2026-01-01T00:00:00.000Z");
    f.evidence(&hash, size);
    let path = f
        .root
        .path()
        .join("native-backups")
        .join(format!("{hash}.native-backup.json"));
    let original = std::fs::read(&path).unwrap();
    std::fs::remove_file(&path).unwrap();
    let missing = f.run().unwrap().value;
    assert_eq!(
        missing["governance"]["latestBackupExternalCopyObserved"],
        false
    );
    assert!(
        !missing["governance"]["errors"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    std::fs::write(&path, b"synthetic tampered archive").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    assert_eq!(
        f.run().unwrap().value["governance"]["latestBackupExternalCopyObserved"],
        false
    );
    std::fs::write(path, original).unwrap();
    f.control.execute("INSERT INTO workspace_audit(event_type,entity_type,entity_id,actor)VALUES('native_portable_restore','backup','synthetic','system:restore')",[]).unwrap();
    assert_eq!(
        f.run().unwrap().value["governance"]["latestBackupExternalCopyObserved"],
        false
    );
}
#[test]
fn current_role_refused_before_retained_files_or_evidence_are_observed() {
    let mut f = fixture();
    f.control.execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','grant','reader','user:u')",[]).unwrap();
    let failure = f.run().unwrap_err();
    assert_eq!(failure.error.code, "access_denied");
}
