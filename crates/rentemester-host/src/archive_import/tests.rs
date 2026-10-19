//! Synthetic unit fixtures only. Browser acceptance uses normal verification/MFA.
use super::*;
use rentemester_application::archive_import::{Action, Output};
use rentemester_runtime::auth::{AuthError, CookieProfile, CookieVerifier};
use rentemester_runtime::document::Prepared;
use serde_json::Value;
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
    let input = serde_json::from_value(json!({"slug":"b","name":"Synthetic B"})).unwrap();
    assert!(
        owner
            .create_company(&cookies, &Fixed(NOW), &proof, &input)
            .unwrap()
            .outcome
            .is_ok()
    );
    owner.select_company("a").unwrap();
    (dir, owner, cookies, proof, uid)
}

// Byte-exact synthetic CRLF posting CSV: bank/equity +/-1.235000; optional LF balances.
// The identical 2025 posting pair is the excluded cut-over marker, never installed.
fn source() -> Prepared {
    Prepared::new("synthetic-history.json".into(),br#"{"format":"rentemester-dinero-archive-source/1","currency":"DKK","files":[{"path":"2024/Posteringer.csv","contentBase64":"S29udG87S29udG9uYXZuO0RhdG87QmlsYWc7QmlsYWdzdHlwZTtUZWtzdDtNb21zdHlwZTtCZWzDuGI7U2FsZG8NCjEwMDA7U3ludGhldGljIGJhbms7MjAyNC0wMi0wMTsxO0ZpbmFucztTeW50aGV0aWMgaGlzdG9yeTs7MSwyMzUwMDA7MSwyMzUwMDANCjgwMDA7U3ludGhldGljIGVxdWl0eTsyMDI0LTAyLTAxOzE7RmluYW5zO1N5bnRoZXRpYyBoaXN0b3J5OzstMSwyMzUwMDA7MA0K"},{"path":"2024/SaldoBalance.csv","contentBase64":"S29udG87S29udG9uYXZuO0JlbMO4YgoxMDAwO1N5bnRoZXRpYyBiYW5rOzEsMjM1MDAwCjgwMDA7U3ludGhldGljIGVxdWl0eTstMSwyMzUwMDAK"},{"path":"2025/Posteringer.csv","contentBase64":"S29udG87S29udG9uYXZuO0RhdG87QmlsYWc7QmlsYWdzdHlwZTtUZWtzdDtNb21zdHlwZTtCZWzDuGI7U2FsZG8NCjEwMDA7U3ludGhldGljIGJhbms7MjAyNS0wMi0wMTsxO0ZpbmFucztTeW50aGV0aWMgaGlzdG9yeTs7MSwyMzUwMDA7MSwyMzUwMDANCjgwMDA7U3ludGhldGljIGVxdWl0eTsyMDI1LTAyLTAxOzE7RmluYW5zO1N5bnRoZXRpYyBoaXN0b3J5OzstMSwyMzUwMDA7MA0K"}]}"#.to_vec()).unwrap()
}

fn run(
    owner: &mut Owner,
    proof: &VerifiedCookie,
    action: Action<'_>,
) -> Result<Value, rentemester_application::control_fence::Failure> {
    owner
        .archive_import(proof, &Fixed(NOW), action)
        .unwrap()
        .outcome
        .map(|v| match v.value {
            Output::Json(v) => v,
            Output::Original(_) => panic!("JSON expected"),
        })
}
fn input(plan: &Value, key: &str) -> rentemester_storage::archive_import::Apply {
    rentemester_storage::archive_import::Apply {
        plan_hash: plan["plan"]["planHash"].as_str().unwrap().into(),
        idempotency_key: key.into(),
        confirm: true,
    }
}
fn counts(owner: &Owner) -> (i64, i64, i64) {
    let db = owner.held.0.as_ref().unwrap().company.as_ref().unwrap();
    (
        db.query_row("SELECT count(*) FROM import_archive_years", [], |r| {
            r.get(0)
        })
        .unwrap(),
        db.query_row("SELECT count(*) FROM journal_entries", [], |r| r.get(0))
            .unwrap(),
        db.query_row("SELECT count(*) FROM accounts", [], |r| r.get(0))
            .unwrap(),
    )
}
#[test]
fn reviewed_original_install_exact_replay_and_profile_independent_historical_reads() {
    let (_dir, mut owner, _cookies, proof, _uid) = setup();
    let file = source();
    let before = counts(&owner);
    let plan = run(&mut owner, &proof, Action::Plan(&file)).unwrap();
    assert_eq!(plan["plan"]["excludedLatestYear"], 2025);
    assert!(!owner.root.join("a").join(file.relative_path()).exists());
    let request = input(&plan, "synthetic-history-install");
    let applied = run(&mut owner, &proof, Action::Apply(&file, &request)).unwrap();
    assert_eq!(
        applied["archiveImport"]["years"].as_array().unwrap().len(),
        1
    );
    assert_eq!(applied["idempotency"]["replayed"], false);
    assert_eq!(counts(&owner), (before.0 + 1, before.1, before.2));
    let years = run(
        &mut owner,
        &proof,
        Action::FiscalYears {
            today: "2026-01-01",
        },
    )
    .unwrap();
    let years = years["fiscalYears"]["years"].as_array().unwrap();
    assert!(years.iter().any(|y| y["source"] == "live"));
    let archived = years
        .iter()
        .find(|y| y["source"] == "archive" && y["label"] == "2024")
        .unwrap();
    assert!(archived["start"].is_null() && archived["end"].is_null());
    assert_eq!(archived["selectable"], false);
    assert_eq!(counts(&owner), (before.0 + 1, before.1, before.2));
    let detail = run(&mut owner, &proof, Action::Year(2024)).unwrap();
    assert_eq!(detail["years"][0]["postingCount"], 2);
    let replay = run(&mut owner, &proof, Action::Apply(&file, &request)).unwrap();
    assert_eq!(replay["archiveImport"], applied["archiveImport"]);
    assert_eq!(replay["idempotency"]["replayed"], true);
    let id = applied["archiveImport"]["importId"].as_i64().unwrap();
    match owner
        .archive_import(&proof, &Fixed(NOW), Action::Original(id))
        .unwrap()
        .outcome
        .unwrap()
        .value
    {
        Output::Original(bytes) => assert_eq!(bytes, file.bytes()),
        _ => panic!("original expected"),
    };
    owner
        .held
        .0
        .as_mut()
        .unwrap()
        .company
        .as_mut()
        .unwrap()
        .execute("UPDATE companies SET currency='EUR' WHERE id=1", [])
        .unwrap();
    let historical = run(&mut owner, &proof, Action::Year(2024)).unwrap();
    assert_eq!(historical, detail);
    let replay = run(&mut owner, &proof, Action::Apply(&file, &request)).unwrap();
    assert_eq!(replay["archiveImport"], applied["archiveImport"]);
    assert_eq!(counts(&owner), (before.0 + 1, before.1, before.2));
}
#[test]
fn stale_review_and_current_admin_refusals_publish_nothing() {
    let (_dir, mut owner, _cookies, proof, _uid) = setup();
    let file = source();
    let plan = run(&mut owner, &proof, Action::Plan(&file)).unwrap();
    let mut request = input(&plan, "synthetic-stale");
    request.plan_hash = "0".repeat(64);
    assert_eq!(
        run(&mut owner, &proof, Action::Apply(&file, &request))
            .unwrap_err()
            .error
            .code,
        "archive_plan_changed"
    );
    assert_eq!(counts(&owner).0, 0);
    assert!(!owner.root.join("a").join(file.relative_path()).exists());
    owner.held.0.as_mut().unwrap().control.as_mut().unwrap().execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','grant','bookkeeper','user:synthetic');").unwrap();
    let request = input(&plan, "synthetic-no-admin");
    assert_eq!(
        run(&mut owner, &proof, Action::Apply(&file, &request))
            .unwrap_err()
            .error
            .code,
        "access_denied"
    );
    assert!(run(&mut owner, &proof, Action::Plan(&file)).is_ok());
    assert_eq!(counts(&owner).0, 0);
    assert!(!owner.root.join("a").join(file.relative_path()).exists());
}
fn period_errors(owner: &mut Owner) -> Vec<Value> {
    let slots = owner.held.0.as_mut().unwrap();
    let tx = slots.company.as_mut().unwrap().transaction().unwrap();
    let errors = rentemester_application::periods::verify_sources(&tx, &mut slots.guard).unwrap();
    tx.rollback().unwrap();
    errors
}
fn signing_key() -> rentemester_runtime::backup_artifact::SigningKey {
    let hex = concat!(
        "3051020101300506032b657004220420",
        "9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60",
        "812100",
        "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a"
    );
    let bytes = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect::<Vec<_>>();
    rentemester_runtime::backup_artifact::SigningKey::from_pkcs8(&bytes).unwrap()
}
#[test]
fn signed_original_inventory_and_live_period_replay_refuse_tampered_original() {
    use rentemester_runtime::backup_artifact as artifact;
    let (_dir, mut owner, cookies, proof, _uid) = setup();
    let file = source();
    let plan = run(&mut owner, &proof, Action::Plan(&file)).unwrap();
    let request = input(&plan, "synthetic-signed-history");
    run(&mut owner, &proof, Action::Apply(&file, &request)).unwrap();
    assert!(period_errors(&mut owner).is_empty());
    let key = signing_key();
    let trust = artifact::TrustedPublicKey::from_bytes(&key.public_key_bytes()).unwrap();
    let wire = owner
        .backup_workspace(&cookies, &Fixed(NOW), &proof, &key)
        .unwrap()
        .outcome
        .unwrap();
    let verified = artifact::verify(&wire, &trust).unwrap();
    Owner::validate_backup(&verified).unwrap();
    // backup iterates legal ledgers; every later company command explicitly selects its anchor.
    owner.select_company("a").unwrap();
    let path = format!("a/{}", file.relative_path());
    let entries = verified
        .entries()
        .iter()
        .map(|e| artifact::Snapshot {
            kind: e.kind().clone(),
            path: e.path().into(),
            bytes: if e.path() == path {
                let mut bytes = e.bytes().to_vec();
                bytes.push(b' ');
                bytes
            } else {
                e.bytes().to_vec()
            },
        })
        .collect();
    // The signed format itself refuses a changed original at its retained hash path.
    assert!(matches!(
        artifact::create(verified.manifest().context.clone(), entries, &key),
        Err(artifact::Error::UnsafeEntry)
    ));
    let full = owner.root.join("a").join(file.relative_path());
    std::fs::write(&full, b"{}").unwrap();
    assert!(!period_errors(&mut owner).is_empty());
    assert!(run(&mut owner, &proof, Action::Apply(&file, &request)).is_err());
    assert!(
        owner
            .backup_workspace(&cookies, &Fixed(NOW), &proof, &key)
            .unwrap()
            .outcome
            .is_err()
    );
}
