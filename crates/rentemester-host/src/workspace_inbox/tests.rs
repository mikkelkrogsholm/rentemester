//! Synthetic unit fixtures only. Browser acceptance uses normal verification/MFA.
use super::*;
use rentemester_application::workspace_inbox::{Action, Destination, Ingest};
use rentemester_runtime::{
    auth::{AuthError, CookieProfile, CookieVerifier},
    document::Prepared,
};
use serde_json::{Value, json};
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
fn input(key: &str) -> Ingest {
    serde_json::from_value(json!({"idempotencyKey":key,"bytesBase64":"not-used-by-typed-app","filename":"synthetic.txt","mimeType":"text/plain","transport":"upload","receivedAt":"2026-01-01T00:00:00.000Z","metadata":{"documentType":"cash_register_receipt"},"candidates":[]})).unwrap()
}
fn file() -> Prepared {
    Prepared::new(
        "synthetic.txt".into(),
        b"Synthetic workspace source\n".to_vec(),
    )
    .unwrap()
}
fn call(
    owner: &mut Owner,
    cookies: &CookieVerifier,
    proof: &VerifiedCookie,
    clock: i64,
    action: Action<'_>,
) -> Result<Value, rentemester_application::control_fence::Failure> {
    owner
        .workspace_inbox(cookies, proof, &Fixed(clock), "a", action)
        .unwrap()
        .outcome
        .map(|v| v.value)
}
fn source(owner: &mut Owner, cookies: &CookieVerifier, proof: &VerifiedCookie) -> String {
    let i = input("synthetic-upload-key");
    let f = file();
    let first = call(owner, cookies, proof, NOW, Action::Ingest(&i, &f)).unwrap();
    let again = call(owner, cookies, proof, NOW, Action::Ingest(&i, &f)).unwrap();
    assert_eq!(first, again);
    first["source"]["sourceId"].as_str().unwrap().into()
}
fn counts(owner: &Owner) -> (i64, i64) {
    let l = owner.held.0.as_ref().unwrap().company.as_ref().unwrap();
    (
        l.query_row("SELECT count(*) FROM documents", [], |r| r.get(0))
            .unwrap(),
        l.query_row("SELECT count(*) FROM journal_entries", [], |r| r.get(0))
            .unwrap(),
    )
}
#[test]
fn workspace_inbox_real_owner_handoff_no_journal_original_exact_and_reopen() {
    let (dir, mut owner, cookies, proof, uid) = setup();
    let id = source(&mut owner, &cookies, &proof);
    assert_eq!(counts(&owner), (0, 0));
    let b = Destination {
        company_slug: "b".into(),
    };
    call(&mut owner, &cookies, &proof, NOW, Action::Assign(&id, &b)).unwrap();
    let a = Destination {
        company_slug: "a".into(),
    };
    assert!(call(&mut owner, &cookies, &proof, NOW, Action::Assign(&id, &a)).is_err());
    // Physical selected ledger must agree with the explicit target.
    assert!(call(&mut owner, &cookies, &proof, NOW, Action::Complete(&id, &b)).is_err());
    owner.select_company("b").unwrap();
    let completed = call(&mut owner, &cookies, &proof, NOW, Action::Complete(&id, &b)).unwrap();
    assert_eq!(completed["source"]["assignments"][0]["state"], "completed");
    assert_eq!(counts(&owner), (1, 0));
    let doc = completed["source"]["assignments"][0]["documentId"]
        .as_i64()
        .unwrap();
    let original = owner
        .document_original(&proof, &Fixed(NOW), doc)
        .unwrap()
        .outcome
        .unwrap()
        .value;
    assert_eq!(original.bytes, file().bytes());
    assert_eq!(
        call(&mut owner, &cookies, &proof, NOW, Action::Complete(&id, &b)).unwrap(),
        completed
    );
    assert_eq!(counts(&owner), (1, 0));
    owner.select_company("a").unwrap();
    assert_eq!(counts(&owner), (0, 0));
    owner.try_close().unwrap();
    let mut owner = Owner::open_with_profile(
        &dir.path().canonicalize().unwrap().join("workspace"),
        uid,
        "w",
        "b",
        NamespaceProfile::LocalUserMacos,
    )
    .unwrap();
    assert_eq!(
        call(
            &mut owner,
            &cookies,
            &proof,
            NOW + 180_000,
            Action::Complete(&id, &b)
        )
        .unwrap(),
        completed
    );
    assert_eq!(counts(&owner), (1, 0));
    let list = call(
        &mut owner,
        &cookies,
        &proof,
        NOW,
        Action::List {
            cursor: 0,
            limit: 25,
        },
    )
    .unwrap();
    assert_eq!(list["count"], 1);
    assert_eq!(list["rows"][0], completed["source"]);
}
#[test]
fn workspace_inbox_claim_split_recovers_same_committed_document_after_expiry() {
    let (_dir, mut owner, cookies, proof, _) = setup();
    let id = source(&mut owner, &cookies, &proof);
    let b = Destination {
        company_slug: "b".into(),
    };
    call(&mut owner, &cookies, &proof, NOW, Action::Assign(&id, &b)).unwrap();
    // Reproduce the actual durable split state using the public claim primitive and
    // normal owned company ingestion, as if process death preceded control finalize.
    {
        let tx = owner
            .held
            .0
            .as_mut()
            .unwrap()
            .control
            .as_mut()
            .unwrap()
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        rentemester_storage::workspace_inbox::claim(
            &tx,
            &id,
            "b",
            "synthetic-lost-claim",
            "2026-01-01T00:00:00.000Z",
            "2026-01-01T00:02:00.000Z",
        )
        .unwrap();
        tx.commit().unwrap();
    }
    owner.select_company("b").unwrap();
    let metadata = serde_json::from_value(
        json!({"source":format!("workspace-inbox:{id}"),"documentType":"cash_register_receipt"}),
    )
    .unwrap();
    let f = file();
    let original = owner
        .ingest_document(
            &proof,
            &Fixed(NOW),
            rentemester_application::documents::Ingest {
                file: &f,
                metadata: &metadata,
                today: "2026-01-01",
            },
        )
        .unwrap()
        .outcome
        .unwrap()
        .value;
    assert_eq!(counts(&owner), (1, 0));
    assert!(
        call(
            &mut owner,
            &cookies,
            &proof,
            NOW + 60_000,
            Action::Complete(&id, &b)
        )
        .is_err()
    );
    let recovered = call(
        &mut owner,
        &cookies,
        &proof,
        NOW + 180_000,
        Action::Complete(&id, &b),
    )
    .unwrap();
    assert_eq!(
        recovered["source"]["assignments"][0]["documentId"],
        original["document"]["id"]
    );
    assert_eq!(counts(&owner), (1, 0));
}
#[test]
fn workspace_inbox_current_rights_cross_anchor_intent_and_corrupt_original_fail_closed() {
    let (_dir, mut owner, cookies, proof, _) = setup();
    let id = source(&mut owner, &cookies, &proof);
    let b = Destination {
        company_slug: "b".into(),
    };
    {
        let c = owner.held.0.as_mut().unwrap().control.as_mut().unwrap();
        c.execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,actor)VALUES('u','b','revoke','user:u')").unwrap();
    }
    assert_eq!(
        call(&mut owner, &cookies, &proof, NOW, Action::Assign(&id, &b))
            .unwrap_err()
            .error
            .code,
        "access_denied"
    );
    assert!(
        call(
            &mut owner,
            &cookies,
            &proof,
            NOW,
            Action::List {
                cursor: 0,
                limit: 25
            }
        )
        .is_ok()
    );
    let mut changed = input("synthetic-upload-key");
    changed.metadata =
        json!({"documentType":"cash_register_receipt","deliveryDescription":"different intent"});
    assert!(
        call(
            &mut owner,
            &cookies,
            &proof,
            NOW,
            Action::Ingest(&changed, &file())
        )
        .is_err()
    );
    let cross = owner
        .workspace_inbox(&cookies, &proof, &Fixed(NOW), "b", Action::Inspect(&id))
        .unwrap()
        .outcome;
    assert!(cross.is_err());
    let a = Destination {
        company_slug: "a".into(),
    };
    call(&mut owner, &cookies, &proof, NOW, Action::Assign(&id, &a)).unwrap();
    owner
        .held
        .0
        .as_mut()
        .unwrap()
        .control
        .as_mut()
        .unwrap()
        .execute(
            "UPDATE rm_workspace_inbox_sources SET original_bytes=x'00' WHERE source_id=?",
            [&id],
        )
        .unwrap();
    assert!(call(&mut owner, &cookies, &proof, NOW, Action::Complete(&id, &a)).is_err());
    assert_eq!(counts(&owner), (0, 0));
}
#[test]
fn workspace_inbox_competing_targets_have_one_durable_winner() {
    let (_dir, mut owner, cookies, proof, _) = setup();
    let id = source(&mut owner, &cookies, &proof);
    let shared = std::sync::Arc::new(std::sync::Mutex::new(owner));
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let mut handles = vec![];
    for target in ["a", "b"] {
        let shared = shared.clone();
        let barrier = barrier.clone();
        let id = id.clone();
        let cookie = cookies.issue_session("owner-token").unwrap();
        let cookies = CookieVerifier::new(
            "w",
            CookieProfile::Development,
            "QUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUE",
        )
        .unwrap();
        let proof = cookies
            .verify(Some(cookie.set_cookie().split(';').next().unwrap()), None)
            .unwrap();
        handles.push(std::thread::spawn(move || {
            barrier.wait();
            let mut owner = shared.lock().unwrap();
            call(
                &mut owner,
                &cookies,
                &proof,
                NOW,
                Action::Assign(
                    &id,
                    &Destination {
                        company_slug: target.into(),
                    },
                ),
            )
            .is_ok()
        }));
    }
    assert_eq!(
        handles
            .into_iter()
            .map(|h| usize::from(h.join().unwrap()))
            .sum::<usize>(),
        1
    );
    let mut owner = shared.lock().unwrap();
    let source = call(&mut owner, &cookies, &proof, NOW, Action::Inspect(&id)).unwrap();
    assert_eq!(source["source"]["assignments"].as_array().unwrap().len(), 1);
    assert_eq!(
        source["source"]["history"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["event_type"] == "assigned")
            .count(),
        1
    );
    assert_eq!(counts(&owner), (0, 0));
}
