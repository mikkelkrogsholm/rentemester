//! Synthetic unit fixtures only. Browser acceptance uses normal verification/MFA.
use super::*;
use rentemester_runtime::auth::{AuthError, CookieProfile, CookieVerifier};
use rentemester_runtime::document::Prepared;
use serde_json::Value;
use serde_json::json;
use storage::{Action, Exact};
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
fn call(
    o: &mut Owner,
    c: &CookieVerifier,
    p: &VerifiedCookie,
    anchor: Option<&str>,
    a: Action,
) -> Result<Value, Failure> {
    o.intercompany_dispositions(c, &Fixed(NOW), p, anchor, a)
        .unwrap()
        .outcome
}
type Fixture = (
    tempfile::TempDir,
    Owner,
    CookieVerifier,
    VerifiedCookie,
    VerifiedCookie,
    Value,
    Vec<(i64, String)>,
    Vec<std::path::PathBuf>,
);
fn fixture(restricted: bool) -> Fixture {
    let (dir, mut o, c, p, _) = setup();
    let control = o.held.0.as_mut().unwrap().control.as_mut().unwrap();
    control.execute_batch("INSERT INTO user(id,name,email,emailVerified,twoFactorEnabled,createdAt,updatedAt)VALUES('r','Reviewer','r@example.invalid',1,1,'2026-01-01','2026-01-01');INSERT INTO twoFactor(id,secret,backupCodes,userId,verified)VALUES('rf','synthetic','[]','r',1);INSERT INTO session(id,userId,token,createdAt,updatedAt,expiresAt)VALUES('rs','r','reviewer-token','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z','2026-01-01T12:00:00.000Z');INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES('r','activate','workspace_owner','user:u');INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('r','a','grant','reviewer','user:u'),('r','b','grant','reviewer','user:u');").unwrap();
    let tx = control
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let party=serde_json::from_value(json!({"partyId":"party-disposition","kind":"organization","name":"Synthetic intercompany","source":"synthetic","observedAt":"2026-01-01T00:00:00Z","reviewAssertion":"Synthetic reviewed source","role":"related_company"})).unwrap();
    let id = rentemester_storage::parties::create(
        &tx,
        &party,
        "unused",
        "user:u",
        "2026-01-01T00:00:00Z",
    )
    .unwrap();
    let role = serde_json::from_value(json!({"role":"related_company"})).unwrap();
    for slug in ["a", "b"] {
        rentemester_storage::parties::role(&tx, &id, slug, &role, "user:u", "2026-01-01T00:00:00Z")
            .unwrap();
    }
    let record=serde_json::from_value(json!({"recordId":"record-disposition","type":"intercompany_agreement","bytesBase64":"eA==","filename":"agreement.txt","source":"synthetic","receivedAt":"2026-01-01T00:00:00Z","uploader":"Synthetic","sensitivity":if restricted{"restricted"}else{"normal"}})).unwrap();
    let links = [
        rentemester_storage::corporate_records::Link {
            r#type: "company".into(),
            id: "a".into(),
        },
        rentemester_storage::corporate_records::Link {
            r#type: "company".into(),
            id: "b".into(),
        },
    ];
    rentemester_storage::corporate_records::ingest(&tx, &record, b"x", "unused", &links, "user:u")
        .unwrap();
    tx.commit().unwrap();
    let cookie = c.issue_session("reviewer-token").unwrap();
    let reviewer = c
        .verify(Some(cookie.set_cookie().split(';').next().unwrap()), None)
        .unwrap();
    let mut journals = vec![];
    let mut paths = vec![];
    for slug in ["a", "b"] {
        o.select_company(slug).unwrap();
        let file = Prepared::new(
            "synthetic-source.txt".into(),
            format!("Synthetic {slug} immutable original").into_bytes(),
        )
        .unwrap();
        let meta = serde_json::from_value(
            json!({"source":"synthetic","documentType":"cash_register_receipt","currency":"DKK"}),
        )
        .unwrap();
        o.ingest_document(
            &p,
            &Fixed(NOW),
            rentemester_application::documents::Ingest {
                file: &file,
                metadata: &meta,
                today: "2026-01-01",
            },
        )
        .unwrap()
        .outcome
        .unwrap();
        paths.push(o.root.join(slug).join(file.relative_path()));
        let db = o.held.0.as_mut().unwrap().company.as_mut().unwrap();
        db.execute_batch("INSERT INTO accounts(account_no,name,type,normal_balance)VALUES('1400','Synthetic reciprocal','asset','debit'),('8000','Synthetic equity','equity','credit')").unwrap();
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        let lines = if slug == "a" {
            json!([{"accountNo":"1400","debitAmount":"12.34"},{"accountNo":"8000","creditAmount":"12.34"}])
        } else {
            json!([{"accountNo":"1400","creditAmount":"12.34"},{"accountNo":"8000","debitAmount":"12.34"}])
        };
        let request=serde_json::from_value(json!({"transactionDate":"2026-01-01","documentId":"1","text":"Synthetic disposition","currency":"DKK","lines":lines})).unwrap();
        let out = rentemester_storage::journal_post::post(
            &tx,
            &request,
            &rentemester_storage::journal_preview::manual_journal_rule_version().unwrap(),
            rentemester_engine::fiscal_year::Date::parse("2026-01-01").unwrap(),
            rentemester_storage::journal_post::Actor {
                subject: "user:u",
                program: "synthetic-unit",
            },
        )
        .unwrap();
        assert_eq!(out["ok"], true);
        journals.push((
            out["entryId"].as_i64().unwrap(),
            out["entryHash"].as_str().unwrap().into(),
        ));
        tx.commit().unwrap();
    }
    o.select_company("a").unwrap();
    let d = json!({"dispositionId":"disposition-synthetic","type":"loan","economicDate":"2026-01-01","amount":12.34,"currency":"DKK","partyIds":["party-disposition"],"evidenceRecordIds":["record-disposition"],"left":{"companySlug":"a","role":"lender","expectedSide":"receivable"},"right":{"companySlug":"b","role":"borrower","expectedSide":"payable"}});
    (dir, o, c, p, reviewer, d, journals, paths)
}
fn approve(id: &str, hash: &str) -> Action {
    Action::Approve(Exact {
        disposition_id: id.into(),
        payload_hash: hash.into(),
        confirm: true,
    })
}
fn propose(d: &Value) -> Action {
    Action::Propose(serde_json::from_value(json!({"disposition":d,"confirm":true})).unwrap())
}
fn status() -> Action {
    Action::Status {
        disposition_id: "disposition-synthetic".into(),
        as_of: "2026-01-01".into(),
    }
}
#[test]
fn current_two_company_four_eyes_physical_originals_and_historical_heads() {
    let (_dir, mut o, c, p, r, d, journals, paths) = fixture(false);
    let plan = call(
        &mut o,
        &c,
        &p,
        Some("a"),
        Action::Plan(serde_json::from_value(json!({"disposition":d})).unwrap()),
    )
    .unwrap();
    assert_eq!(plan["ledgerEffects"], json!([]));
    let original = std::fs::read(&paths[1]).unwrap();
    std::fs::write(&paths[1], b"changed original").unwrap();
    assert_eq!(
        call(&mut o, &c, &p, Some("a"), propose(&d))
            .unwrap_err()
            .error
            .code,
        "disposition_source_unavailable"
    );
    std::fs::write(&paths[1], original).unwrap();
    let proposed = call(&mut o, &c, &p, Some("a"), propose(&d)).unwrap();
    let hash = proposed["payloadHash"].as_str().unwrap();
    assert!(
        call(
            &mut o,
            &c,
            &p,
            Some("a"),
            approve("disposition-synthetic", hash)
        )
        .is_err()
    );
    // Reviewer has OwnershipManage but no CompanyAdmin: four-eyes approval remains supported.
    call(
        &mut o,
        &c,
        &r,
        Some("a"),
        approve("disposition-synthetic", hash),
    )
    .unwrap();
    for (n, side) in ["left", "right"].into_iter().enumerate() {
        let action=Action::Link(serde_json::from_value(json!({"dispositionId":"disposition-synthetic","payloadHash":hash,"confirm":true,"side":side,"journalEntryId":journals[n].0,"ledgerHeadHash":journals[n].1})).unwrap());
        let result = call(&mut o, &c, &p, Some("a"), action).unwrap();
        assert_eq!(
            result["status"],
            if n == 0 { "partly_posted" } else { "posted" }
        );
    }
    let ready = call(&mut o, &c, &p, None, status()).unwrap();
    assert_eq!(ready["balanced"], true);
    assert_eq!(ready["exceptions"], json!([]));
    // A control audit failure after a lifecycle insert rolls back that insert and its audit atomically.
    o.held.0.as_mut().unwrap().control.as_mut().unwrap().execute_batch("CREATE TEMP TRIGGER synthetic_audit_failure BEFORE INSERT ON workspace_audit BEGIN SELECT RAISE(ABORT,'synthetic rollback');END;").unwrap();
    let settle = || {
        Action::Settle(serde_json::from_value(json!({"dispositionId":"disposition-synthetic","payloadHash":hash,"confirm":true,"settlementEvidenceRecordIds":["record-disposition"]})).unwrap())
    };
    assert!(call(&mut o, &c, &p, Some("a"), settle()).is_err());
    o.held
        .0
        .as_mut()
        .unwrap()
        .control
        .as_mut()
        .unwrap()
        .execute_batch("DROP TRIGGER synthetic_audit_failure")
        .unwrap();
    assert_eq!(call(&mut o, &c, &p, None, status()).unwrap(), ready);
    call(&mut o, &c, &p, Some("a"), settle()).unwrap();
    // A later ordinary posting makes CURRENT status stale but remains valid signed historical evidence.
    let db = o.held.0.as_mut().unwrap().company.as_mut().unwrap();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let request=serde_json::from_value(json!({"transactionDate":"2026-01-01","currency":"DKK","text":"Later ordinary source","lines":[{"accountNo":"1400","debitAmount":"1.00"},{"accountNo":"8000","creditAmount":"1.00"}]})).unwrap();
    rentemester_storage::journal_post::post(
        &tx,
        &request,
        &rentemester_storage::journal_preview::manual_journal_rule_version().unwrap(),
        rentemester_engine::fiscal_year::Date::parse("2026-01-01").unwrap(),
        rentemester_storage::journal_post::Actor {
            subject: "user:u",
            program: "synthetic-unit",
        },
    )
    .unwrap();
    tx.commit().unwrap();
    let stale = call(&mut o, &c, &p, None, status()).unwrap();
    assert!(
        stale["exceptions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["kind"] == "stale_or_reversed_journal")
    );
    let slots = o.held.0.as_mut().unwrap();
    let tx = slots
        .control
        .as_mut()
        .unwrap()
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let db = slots.company.as_ref().unwrap();
    let uuid: String = db
        .query_row(
            "SELECT ledger_uuid FROM ledger_identity WHERE id=1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    validate_historical_company(&tx, db, "a", &uuid).unwrap();
    assert!(validate_historical_company(&tx, db, "a", &"0".repeat(32)).is_err());
    tx.rollback().unwrap();
}
#[test]
fn revoked_endpoint_sensitive_source_and_frozen_workspace_refuse_without_new_history() {
    let (_dir, mut o, c, p, r, d, _, _) = fixture(true);
    let proposed = call(&mut o, &c, &p, Some("a"), propose(&d)).unwrap();
    let hash = proposed["payloadHash"].as_str().unwrap();
    assert_eq!(
        call(
            &mut o,
            &c,
            &r,
            Some("a"),
            approve("disposition-synthetic", hash)
        )
        .unwrap_err()
        .error
        .code,
        "corporate_record_not_found"
    );
    assert_eq!(
        call(
            &mut o,
            &c,
            &p,
            Some("unrelated"),
            approve("disposition-synthetic", hash)
        )
        .unwrap_err()
        .error
        .code,
        "access_denied"
    );
    o.held.0.as_mut().unwrap().control.as_mut().unwrap().execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','b','revoke',NULL,'user:u');").unwrap();
    assert_eq!(
        call(
            &mut o,
            &c,
            &p,
            Some("a"),
            approve("disposition-synthetic", hash)
        )
        .unwrap_err()
        .error
        .code,
        "access_denied"
    );
    o.held.0.as_mut().unwrap().control.as_mut().unwrap().execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','b','grant','owner','user:u');").unwrap();
    let transfer = o
        .workspace_transfer_status(&c, &Fixed(NOW), &p)
        .unwrap()
        .outcome
        .unwrap();
    let input=serde_json::from_value(json!({"expectedStateHash":transfer["stateHash"],"destination":"https://synthetic.invalid","reason":"Synthetic frozen disposition proof","confirm":true})).unwrap();
    o.freeze_workspace(&c, &Fixed(NOW), &p, &input)
        .unwrap()
        .outcome
        .unwrap();
    assert_eq!(
        call(
            &mut o,
            &c,
            &p,
            Some("a"),
            approve("disposition-synthetic", hash)
        )
        .unwrap_err()
        .error
        .code,
        "workspace_transfer_fenced"
    );
    let read = call(&mut o, &c, &p, None, status()).unwrap();
    assert_eq!(read["status"], "proposed");
    assert_eq!(read["events"].as_array().unwrap().len(), 1);
}
