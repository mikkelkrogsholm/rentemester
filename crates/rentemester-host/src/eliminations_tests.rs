//! Synthetic unit fixtures only. Browser acceptance uses normal verification/MFA.
use super::*;
use rentemester_application::{
    eliminations::{Action, Exact, Proposal},
    intercompany as ic,
};
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
fn fixture(
    reverse: bool,
) -> (
    tempfile::TempDir,
    Owner,
    CookieVerifier,
    VerifiedCookie,
    VerifiedCookie,
) {
    let (dir, mut owner, cookies, proof, _) = setup();
    let c = owner.held.0.as_mut().unwrap().control.as_mut().unwrap();
    c.execute_batch("INSERT INTO user(id,name,email,emailVerified,twoFactorEnabled,createdAt,updatedAt)VALUES('r','Reviewer','r@example.invalid',1,1,'2026-01-01','2026-01-01');INSERT INTO twoFactor(id,secret,backupCodes,userId,verified)VALUES('rf','synthetic','[]','r',1);INSERT INTO session(id,userId,token,createdAt,updatedAt,expiresAt)VALUES('rs','r','reviewer-token','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z','2026-01-01T12:00:00.000Z');INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES('r','activate','workspace_owner','user:u');INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('r','a','grant','owner','user:u'),('r','b','grant','owner','user:u');").unwrap();
    let cookie = cookies.issue_session("reviewer-token").unwrap();
    let review = cookies
        .verify(Some(cookie.set_cookie().split(';').next().unwrap()), None)
        .unwrap();
    let m=serde_json::from_value(json!({"version":1,"groups":[{"id":"synthetic","name":"Synthetic","memberships":[{"id":"a-member","companySlug":"a","validFrom":"2026-01-01"},{"id":"b-member","companySlug":"b","validFrom":"2026-01-01"}],"ownership":[]}]})).unwrap();
    owner
        .intercompany(
            &cookies,
            &Fixed(NOW),
            &proof,
            Some("a"),
            ic::Action::Manifest(&m),
        )
        .unwrap()
        .outcome
        .unwrap();
    let map=serde_json::from_value(json!({"id":"mapping","groupId":"synthetic","leftCompanySlug":"a","rightCompanySlug":"b","leftAccountNos":["1400"],"rightAccountNos":["1400"],"leftPosition":if reverse{"payable"}else{"receivable"},"rightPosition":if reverse{"receivable"}else{"payable"},"evidenceRefs":["synthetic"],"validFrom":"2026-01-01"})).unwrap();
    let p = owner
        .intercompany(
            &cookies,
            &Fixed(NOW),
            &proof,
            Some("a"),
            ic::Action::Propose(&map),
        )
        .unwrap()
        .outcome
        .unwrap();
    let exact = ic::Exact {
        mapping_id: "mapping".into(),
        mapping_hash: p["mappingHash"].as_str().unwrap().into(),
    };
    owner
        .intercompany(
            &cookies,
            &Fixed(NOW),
            &review,
            Some("a"),
            ic::Action::Approve(&exact),
        )
        .unwrap()
        .outcome
        .unwrap();
    for slug in ["a", "b"] {
        owner.select_company(slug).unwrap();
        let payable = (slug == "a") == reverse;
        let c = owner.held.0.as_mut().unwrap().company.as_mut().unwrap();
        c.execute("INSERT INTO accounts(account_no,name,type,normal_balance)VALUES('1400','Synthetic reciprocal',?,?)",rusqlite::params![if payable{"liability"}else{"asset"},if payable{"credit"}else{"debit"}]).unwrap();
        c.execute_batch("INSERT INTO accounts(account_no,name,type,normal_balance)VALUES('8000','Synthetic equity','equity','credit')").unwrap();
        let tx = c
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        let lines = if payable {
            json!([{"accountNo":"1400","creditAmount":"12.34"},{"accountNo":"8000","debitAmount":"12.34"}])
        } else {
            json!([{"accountNo":"1400","debitAmount":"12.34"},{"accountNo":"8000","creditAmount":"12.34"}])
        };
        let request=serde_json::from_value(json!({"transactionDate":"2026-01-01","text":"Synthetic reciprocal","currency":"DKK","lines":lines})).unwrap();
        rentemester_storage::journal_post::post(
            &tx,
            &request,
            &rentemester_storage::journal_preview::manual_journal_rule_version().unwrap(),
            rentemester_engine::fiscal_year::Date::parse("2026-01-01").unwrap(),
            rentemester_storage::journal_post::Actor {
                subject: "user:synthetic",
                program: "test",
            },
        )
        .unwrap();
        tx.commit().unwrap();
    }
    owner.select_company("a").unwrap();
    (dir, owner, cookies, proof, review)
}
fn input(id: &str) -> rentemester_engine::eliminations::Input {
    serde_json::from_value(
        json!({"id":id,"mappingId":"mapping","asOf":"2026-01-01","evidenceRefs":["synthetic"]}),
    )
    .unwrap()
}
#[test]
fn owned_v2_both_orientations_four_eyes_apply_reverse_no_journal_effects() {
    for reverse in [false, true] {
        let (_dir, mut owner, cookies, proof, review) = fixture(reverse);
        let source = owner
            .intercompany(
                &cookies,
                &Fixed(NOW),
                &proof,
                None,
                ic::Action::Reconciliation("2026-01-01"),
            )
            .unwrap()
            .outcome
            .unwrap();
        let i = input("eliminate");
        let count = owner
            .held
            .0
            .as_ref()
            .unwrap()
            .control
            .as_ref()
            .unwrap()
            .query_row(
                "SELECT count(*) FROM rm_consolidation_elimination_events",
                [],
                |r| r.get::<_, i64>(0),
            )
            .unwrap();
        let plan = owner
            .eliminations(&cookies, &Fixed(NOW), &proof, Some("a"), Action::Plan(&i))
            .unwrap()
            .outcome
            .unwrap();
        assert_eq!(
            owner
                .held
                .0
                .as_ref()
                .unwrap()
                .control
                .as_ref()
                .unwrap()
                .query_row(
                    "SELECT count(*) FROM rm_consolidation_elimination_events",
                    [],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
            count
        );
        assert_eq!(plan["plan"]["payload"]["version"], 2);
        assert_eq!(plan["plan"]["payload"]["left"]["companySlug"], "a");
        assert_eq!(
            plan["plan"]["payload"]["left"]["debitOre"],
            if reverse { "1234" } else { "0" }
        );
        let hash = plan["plan"]["payloadHash"].as_str().unwrap().to_owned();
        let wrong = Proposal {
            input: i.clone(),
            expected_payload_hash: "0".repeat(64),
        };
        assert!(
            owner
                .eliminations(
                    &cookies,
                    &Fixed(NOW),
                    &proof,
                    Some("a"),
                    Action::Propose(&wrong)
                )
                .unwrap()
                .outcome
                .is_err()
        );
        let proposal = Proposal {
            input: i.clone(),
            expected_payload_hash: hash.clone(),
        };
        owner
            .eliminations(
                &cookies,
                &Fixed(NOW),
                &proof,
                Some("a"),
                Action::Propose(&proposal),
            )
            .unwrap()
            .outcome
            .unwrap();
        let e = Exact {
            elimination_id: i.id.clone(),
            payload_hash: hash,
        };
        assert!(
            owner
                .eliminations(
                    &cookies,
                    &Fixed(NOW),
                    &proof,
                    Some("a"),
                    Action::Approve(&e)
                )
                .unwrap()
                .outcome
                .is_err()
        );
        owner
            .eliminations(
                &cookies,
                &Fixed(NOW),
                &review,
                Some("a"),
                Action::Approve(&e),
            )
            .unwrap()
            .outcome
            .unwrap();
        assert!(
            owner
                .eliminations(&cookies, &Fixed(NOW), &proof, Some("a"), Action::Apply(&e))
                .unwrap()
                .outcome
                .is_err()
        );
        owner
            .eliminations(&cookies, &Fixed(NOW), &review, Some("a"), Action::Apply(&e))
            .unwrap()
            .outcome
            .unwrap();
        let rows = owner
            .eliminations(
                &cookies,
                &Fixed(NOW),
                &proof,
                None,
                Action::Overview("2026-01-01"),
            )
            .unwrap()
            .outcome
            .unwrap();
        assert_eq!(rows["rows"].as_array().unwrap().len(), 1);
        owner
            .eliminations(
                &cookies,
                &Fixed(NOW),
                &proof,
                Some("a"),
                Action::Reverse(&e),
            )
            .unwrap()
            .outcome
            .unwrap();
        assert!(
            owner
                .eliminations(
                    &cookies,
                    &Fixed(NOW),
                    &proof,
                    None,
                    Action::Overview("2026-01-01")
                )
                .unwrap()
                .outcome
                .unwrap()["rows"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            owner
                .intercompany(
                    &cookies,
                    &Fixed(NOW),
                    &proof,
                    None,
                    ic::Action::Reconciliation("2026-01-01")
                )
                .unwrap()
                .outcome
                .unwrap(),
            source
        );
    }
}
#[test]
fn stale_source_after_approval_and_current_membership_refusal_have_no_effect() {
    let (_dir, mut owner, cookies, proof, review) = fixture(false);
    let i = input("stale");
    let plan = owner
        .eliminations(&cookies, &Fixed(NOW), &proof, Some("a"), Action::Plan(&i))
        .unwrap()
        .outcome
        .unwrap();
    let p = Proposal {
        input: i,
        expected_payload_hash: plan["plan"]["payloadHash"].as_str().unwrap().into(),
    };
    owner
        .eliminations(
            &cookies,
            &Fixed(NOW),
            &proof,
            Some("a"),
            Action::Propose(&p),
        )
        .unwrap()
        .outcome
        .unwrap();
    let e = Exact {
        elimination_id: "stale".into(),
        payload_hash: p.expected_payload_hash,
    };
    owner
        .eliminations(
            &cookies,
            &Fixed(NOW),
            &review,
            Some("a"),
            Action::Approve(&e),
        )
        .unwrap()
        .outcome
        .unwrap();
    // A real new independently posted journal moves the global head even though selected balance stays equal.
    owner.select_company("a").unwrap();
    let c = owner.held.0.as_mut().unwrap().company.as_mut().unwrap();
    c.execute_batch("INSERT INTO accounts(account_no,name,type,normal_balance)VALUES('1800','Synthetic bank','asset','debit')").unwrap();
    let tx = c
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    let request=serde_json::from_value(json!({"transactionDate":"2026-01-01","text":"Synthetic later head","currency":"DKK","lines":[{"accountNo":"1800","debitAmount":"1.00"},{"accountNo":"8000","creditAmount":"1.00"}]})).unwrap();
    rentemester_storage::journal_post::post(
        &tx,
        &request,
        &rentemester_storage::journal_preview::manual_journal_rule_version().unwrap(),
        rentemester_engine::fiscal_year::Date::parse("2026-01-01").unwrap(),
        rentemester_storage::journal_post::Actor {
            subject: "user:synthetic",
            program: "test",
        },
    )
    .unwrap();
    tx.commit().unwrap();
    assert!(
        owner
            .eliminations(&cookies, &Fixed(NOW), &review, Some("a"), Action::Apply(&e))
            .unwrap()
            .outcome
            .is_err()
    );
    owner.held.0.as_mut().unwrap().control.as_mut().unwrap().execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('r','b','revoke',NULL,'user:u')").unwrap();
    let hidden = owner
        .eliminations(&cookies, &Fixed(NOW), &review, Some("a"), Action::Current)
        .unwrap()
        .outcome
        .unwrap();
    assert_eq!(hidden["rows"], json!([]));
    assert_eq!(
        owner
            .eliminations(
                &cookies,
                &Fixed(NOW),
                &review,
                Some("a"),
                Action::Reject(&e)
            )
            .unwrap()
            .outcome
            .unwrap_err()
            .error
            .code,
        "access_denied"
    );
    let current = owner
        .eliminations(&cookies, &Fixed(NOW), &proof, Some("a"), Action::Current)
        .unwrap()
        .outcome
        .unwrap();
    assert_eq!(current["rows"][0]["status"], "approved");
}
