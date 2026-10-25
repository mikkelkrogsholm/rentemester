//! Synthetic unit fixtures only. Browser acceptance uses normal verification/MFA.
use super::*;
use rentemester_application::backup_original122::original717::{Archive, Create};
use rentemester_runtime::auth::{AuthError, CookieProfile, CookieVerifier};
use serde_json::json;
struct Fixed(i64);
impl Clock for Fixed {
    fn now_unix_ms(&self) -> Result<i64, AuthError> {
        Ok(self.0)
    }
}
const NOW: i64 = 1767225600000;
fn setup() -> (
    tempfile::TempDir,
    Owner,
    CookieVerifier,
    VerifiedCookie,
    u32,
) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap().join("workspace");
    let uid = String::from_utf8(
        std::process::Command::new("id")
            .arg("-u")
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap()
    .trim()
    .parse()
    .unwrap();
    let mut owner = Owner::create_with_chart_profile(
        &root,
        uid,
        "w",
        "a",
        "Synthetic A",
        "2026-01-01T00:00:00.000Z",
        &[],
        NamespaceProfile::LocalUserMacos,
    )
    .unwrap();
    owner.held.0.as_mut().unwrap().control.as_mut().unwrap().execute_batch("INSERT INTO user(id,name,email,emailVerified,twoFactorEnabled,createdAt,updatedAt)VALUES('u','Synthetic','u@example.invalid',1,1,'2026-01-01','2026-01-01');INSERT INTO twoFactor(id,secret,backupCodes,userId,verified)VALUES('f','synthetic','[]','u',1);INSERT INTO session(id,userId,token,createdAt,updatedAt,expiresAt)VALUES('s','u','owner-token','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z','2026-01-01T12:00:00.000Z');INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES('u','activate','workspace_owner','user:u');INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','grant','owner','user:u');").unwrap();
    let cookies = CookieVerifier::new(
        "w",
        CookieProfile::Development,
        "QUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUE",
    )
    .unwrap();
    let cookie = cookies.issue_session("owner-token").unwrap();
    let proof = cookies
        .verify(Some(cookie.set_cookie().split(';').next().unwrap()), None)
        .unwrap();
    (dir, owner, cookies, proof, uid)
}

#[test]
fn original717_system_backup_scope_inventory_signature_duplicate() {
    let (_dir, mut owner, _cookies, proof, _uid) = setup();
    let invoke = |owner: &mut Owner| {
        owner
            .backup_original717(
                &proof,
                &Fixed(NOW),
                Operation::Create(Create {
                    confirm: true,
                    at: None,
                    archive: Some(false),
                }),
                None,
            )
            .unwrap()
            .outcome
            .unwrap()
            .value
    };
    let core = invoke(&mut owner);
    assert_eq!(core["ok"], true);
    assert!(core.get("archive").is_none());
    let files = owner
        .held
        .0
        .as_mut()
        .unwrap()
        .guard
        .load_original717(None, None)
        .unwrap();
    assert_eq!(files.manifest().manifest_version, Some(2));
    assert_eq!(
        files.manifest().provenance.as_ref().unwrap()["product"]["runtime"],
        "rust"
    );
    assert!(files.files().contains_key("ledger.sqlite"));
    assert!(
        !files
            .files()
            .keys()
            .any(|p| p.contains("workspace-control"))
    );
    assert_eq!(invoke(&mut owner)["ok"], false);
}
#[test]
fn original717_system_backup_archive_latest_deterministic_and_unknown() {
    let (_dir, mut owner, _cookies, proof, _uid) = setup();
    let created = owner
        .backup_original717(
            &proof,
            &Fixed(NOW),
            Operation::Create(Create {
                confirm: true,
                at: None,
                archive: Some(true),
            }),
            None,
        )
        .unwrap()
        .outcome
        .unwrap()
        .value;
    assert_eq!(created["archive"]["ok"], true);
    let packed = owner
        .backup_original717(
            &proof,
            &Fixed(NOW),
            Operation::Archive(Archive {
                confirm: true,
                backup_id: None,
                out: None,
            }),
            None,
        )
        .unwrap()
        .outcome
        .unwrap()
        .value;
    assert_eq!(packed, created["archive"]);
    let bytes = std::fs::read(packed["archivePath"].as_str().unwrap()).unwrap();
    let proof = owner
        .held
        .0
        .as_mut()
        .unwrap()
        .guard
        .load_original717(None, None)
        .unwrap();
    assert_eq!(codec::read_archive(&bytes).unwrap(), *proof.files());
    let unknown = owner
        .backup_original717(
            &_cookies
                .verify(
                    Some(
                        _cookies
                            .issue_session("owner-token")
                            .unwrap()
                            .set_cookie()
                            .split(';')
                            .next()
                            .unwrap(),
                    ),
                    None,
                )
                .unwrap(),
            &Fixed(NOW),
            Operation::Archive(Archive {
                confirm: true,
                backup_id: Some("unknown".into()),
                out: None,
            }),
            None,
        )
        .unwrap()
        .outcome
        .unwrap()
        .value;
    assert_eq!(unknown["ok"], false);
}
#[test]
fn original717_system_backup_reader_revoked_confirmation() {
    let (_dir, mut owner, _cookies, proof, _uid) = setup();
    let make = || {
        Operation::Create(Create {
            confirm: true,
            at: None,
            archive: None,
        })
    };
    let refusal = owner
        .backup_original717(
            &proof,
            &Fixed(NOW),
            Operation::Create(Create {
                confirm: false,
                at: None,
                archive: None,
            }),
            None,
        )
        .unwrap()
        .outcome
        .unwrap()
        .value;
    assert_eq!(refusal["ok"], false);
    owner.held.0.as_mut().unwrap().control.as_mut().unwrap().execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','grant','reader','user:u');").unwrap();
    assert!(
        owner
            .backup_original717(&proof, &Fixed(NOW), make(), None)
            .unwrap()
            .outcome
            .is_err()
    );
    owner
        .held
        .0
        .as_mut()
        .unwrap()
        .control
        .as_mut()
        .unwrap()
        .execute_batch("DELETE FROM session;")
        .unwrap();
    assert!(
        owner
            .backup_original717(&proof, &Fixed(NOW), make(), None)
            .unwrap()
            .outcome
            .is_err()
    );
}

fn create(owner: &mut Owner, proof: &VerifiedCookie) -> Value {
    owner
        .backup_original717(
            proof,
            &Fixed(NOW),
            Operation::Create(Create {
                confirm: true,
                at: None,
                archive: Some(true),
            }),
            None,
        )
        .unwrap()
        .outcome
        .unwrap()
        .value
}
fn destination(owner: &mut Owner, kind: &str, location: &str) {
    use rentemester_storage::backup_registration::{Request, apply};
    let tx = owner
        .held
        .0
        .as_mut()
        .unwrap()
        .control
        .as_mut()
        .unwrap()
        .transaction()
        .unwrap();
    apply(
        &tx,
        "a",
        &Request::Add {
            operation_id: "add-original717".into(),
            destination_id: "synthetic-target".into(),
            label: "Synthetic".into(),
            kind: kind.into(),
            location: location.into(),
            attested_by: "Synthetic operator".into(),
            in_eea_or_eu: true,
            non_related_party: true,
            region_country: Some("DK".into()),
            region_note: None,
            it_security_meets_standards: Some(true),
            it_security_note: None,
            operator_target_id: Some("synthetic-local".into()),
            confirm: true,
        },
        "user:u",
        "2026-01-01T00:00:00.000Z",
        None,
    )
    .unwrap();
    tx.commit().unwrap();
}
#[test]
fn original717_system_backup_place_copy_duplicate_unknown_and_scope() {
    use std::os::unix::fs::PermissionsExt;
    let (dir, mut owner, _cookies, proof, uid) = setup();
    let core = create(&mut owner, &proof);
    let target = dir.path().canonicalize().unwrap().join("placement");
    std::fs::create_dir(&target).unwrap();
    std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o700)).unwrap();
    destination(&mut owner, "local-folder", target.to_str().unwrap());
    let config = rentemester_runtime::backup_destination::Config {
        id: "synthetic-local".into(),
        label: "Synthetic".into(),
        kind: "local-folder".into(),
        directory: target.clone(),
        non_related_party: true,
        in_eea_or_eu: true,
        country: "DK".into(),
        meets_recognised_standards: true,
        attested_by: "Synthetic operator".into(),
        attested_at: "2026-01-01T00:00:00.000Z".into(),
    };
    let input = |destination: &str| original::Place {
        archive_path: core["archive"]["archivePath"].as_str().unwrap().into(),
        destination_id: destination.into(),
        actor_kind: Some("human".into()),
        at: None,
        note: Some(" synthetic ".into()),
        confirm: true,
    };
    for _ in 0..2 {
        let outcome = owner
            .place_original717(
                &proof,
                &Fixed(NOW),
                input("synthetic-target"),
                std::slice::from_ref(&config),
                uid,
            )
            .unwrap()
            .outcome
            .unwrap()
            .value;
        assert_eq!(outcome["placement"]["verified"], true);
        assert_eq!(outcome["placement"]["actor"], "user:u");
    }
    assert_eq!(
        owner
            .place_original717(
                &proof,
                &Fixed(NOW),
                input("unknown"),
                std::slice::from_ref(&config),
                uid
            )
            .unwrap()
            .outcome
            .unwrap()
            .value["ok"],
        false
    );
    let expected = std::fs::read(core["archive"]["archivePath"].as_str().unwrap()).unwrap();
    assert_eq!(
        std::fs::read(target.join(format!("{}.tar", core["backupId"].as_str().unwrap()))).unwrap(),
        expected
    );
    let mut wrong = input("synthetic-target");
    wrong.archive_path = wrong.archive_path.replace("/a/backups/", "/b/backups/");
    assert_eq!(
        owner
            .place_original717(
                &proof,
                &Fixed(NOW),
                wrong,
                std::slice::from_ref(&config),
                uid
            )
            .unwrap()
            .outcome
            .unwrap()
            .value["ok"],
        false
    );
}
#[test]
fn original717_system_restore_backup_fresh_target_confirmation_tamper_source_unchanged() {
    let (dir, mut owner, cookies, proof, uid) = setup();
    let core = create(&mut owner, &proof);
    let operator =
        rentemester_runtime::bootstrap::Operator::from_policy("w", "user:u", &["user:u".into()])
            .unwrap();
    let target = dir.path().canonicalize().unwrap().join("restored");
    let input = |confirmation: bool| original::Restore {
        backup_dir: core["backupDir"].as_str().unwrap().into(),
        target_company: target.to_str().unwrap().into(),
        verify_key: None,
        public_key: None,
        confirm: true,
        confirm_text: confirmation.then(|| format!("RESTORE {}", target.display())),
    };
    let before = std::fs::read(core["manifestPath"].as_str().unwrap()).unwrap();
    let refused = owner
        .restore_original717(&cookies, &Fixed(NOW), &proof, &operator, uid, input(false))
        .unwrap()
        .outcome
        .unwrap();
    assert_eq!(refused["code"], "CONFIRMTEXT_MISMATCH");
    assert!(!target.exists());
    let restored = owner
        .restore_original717(&cookies, &Fixed(NOW), &proof, &operator, uid, input(true))
        .unwrap()
        .outcome
        .unwrap();
    assert_eq!(restored["ok"], true);
    assert!(target.join("data/ledger.sqlite").exists());
    assert!(
        !target
            .join(".rentemester/workspace-control.sqlite")
            .exists()
    );
    assert_eq!(
        std::fs::read(core["manifestPath"].as_str().unwrap()).unwrap(),
        before
    );
    let again = owner
        .restore_original717(&cookies, &Fixed(NOW), &proof, &operator, uid, input(true))
        .unwrap();
    assert_eq!(again.outcome.unwrap()["ok"], false);
    let mut tampered = before;
    tampered[0] ^= 1;
    std::fs::write(core["manifestPath"].as_str().unwrap(), tampered).unwrap();
    let mut fresh = input(true);
    fresh.target_company = dir
        .path()
        .canonicalize()
        .unwrap()
        .join("tampered-target")
        .to_str()
        .unwrap()
        .into();
    fresh.confirm_text = Some(format!("RESTORE {}", fresh.target_company));
    assert_eq!(
        owner
            .restore_original717(&cookies, &Fixed(NOW), &proof, &operator, uid, fresh)
            .unwrap()
            .outcome
            .unwrap()["ok"],
        false
    );
}
#[test]
fn original717_system_backup_verify_remote_placement_fake_provider_happy_failure_and_revoked() {
    use rentemester_runtime::backup_remote::{Expected, Observation, Verifier};
    struct Fake(bool);
    impl Verifier for Fake {
        fn verify(
            &self,
            e: &Expected,
            _: &dyn Clock,
        ) -> Result<Observation, rentemester_engine::Error> {
            if !self.0 {
                return Err(invalid());
            }
            Ok(Observation {
                object_id: e.object_id.clone(),
                folder_id: e.folder_id.clone(),
                file_name: e.file_name.clone(),
                archive_sha256: e.archive_sha256.clone(),
                archive_size_bytes: e.archive_size_bytes,
                observed_at_ms: NOW,
            })
        }
    }
    let (_dir, mut owner, _cookies, proof, uid) = setup();
    let core = create(&mut owner, &proof);
    destination(
        &mut owner,
        "google-drive",
        "google-drive://synthetic-folder",
    );
    let bindings = [rentemester_application::backup_remote::Binding {
        destination_id: "synthetic-local".into(),
        folder_id: "synthetic-folder".into(),
    }];
    let input = || original::Remote {
        destination_id: "synthetic-target".into(),
        backup_id: core["backupId"].as_str().unwrap().into(),
        remote_object_id: "synthetic-object".into(),
        max_metadata_age_ms: None,
        actor_kind: None,
        at: None,
        note: None,
        confirm: true,
    };
    let prepared = owner
        .prepare_remote_original717(&proof, &Fixed(NOW), input(), &bindings, uid)
        .unwrap()
        .outcome
        .unwrap()
        .value;
    let original::RemotePreparation::Ready(prepared) = prepared else {
        panic!("expected admitted remote preparation")
    };
    let observed = Fake(true).verify(&prepared.expected, &Fixed(NOW)).unwrap();
    let happy = owner
        .finish_remote_original717(
            &proof,
            &Fixed(NOW),
            &prepared,
            Ok(&observed),
            &bindings,
            uid,
        )
        .unwrap()
        .outcome
        .unwrap()
        .value;
    assert_eq!(happy["placement"]["verifyMethod"], "remote-provider");
    let failed = Fake(false).verify(&prepared.expected, &Fixed(NOW));
    assert!(failed.is_err());
    assert_eq!(
        owner
            .finish_remote_original717(&proof, &Fixed(NOW), &prepared, Err(()), &bindings, uid)
            .unwrap()
            .outcome
            .unwrap()
            .value["ok"],
        false
    );
    let mut unknown = input();
    unknown.destination_id = "unknown".into();
    assert!(matches!(
        owner
            .prepare_remote_original717(&proof, &Fixed(NOW), unknown, &bindings, uid)
            .unwrap()
            .outcome
            .unwrap()
            .value,
        original::RemotePreparation::Complete(_)
    ));
    owner
        .held
        .0
        .as_mut()
        .unwrap()
        .control
        .as_mut()
        .unwrap()
        .execute_batch("DELETE FROM session;")
        .unwrap();
    assert!(
        owner
            .finish_remote_original717(
                &proof,
                &Fixed(NOW),
                &prepared,
                Ok(&observed),
                &bindings,
                uid
            )
            .unwrap()
            .outcome
            .is_err()
    );
}
