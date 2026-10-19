//! Synthetic unit fixtures only. Browser acceptance uses normal verification/MFA.
use super::*;
use rentemester_application::intercompany::{Action, Exact};
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
#[test]
fn two_company_four_eyes_current_permissions_and_exact_reciprocal_read() {
    let (_dir, mut owner, cookies, proof, _uid) = setup();
    let manifest=serde_json::from_value(json!({"version":1,"groups":[{"id":"synthetic","name":"Synthetic","memberships":[{"id":"a-member","companySlug":"a","validFrom":"2026-01-01"},{"id":"b-member","companySlug":"b","validFrom":"2026-01-01"}],"ownership":[]}]})).unwrap();
    let mapping=serde_json::from_value(json!({"id":"synthetic-map","groupId":"synthetic","leftCompanySlug":"a","rightCompanySlug":"b","leftAccountNos":["1400"],"rightAccountNos":["1400"],"leftPosition":"receivable","rightPosition":"payable","evidenceRefs":["synthetic-source"],"validFrom":"2026-01-01"})).unwrap();
    owner
        .intercompany(
            &cookies,
            &Fixed(NOW),
            &proof,
            Some("a"),
            Action::Manifest(&manifest),
        )
        .unwrap()
        .outcome
        .unwrap();
    let proposed = owner
        .intercompany(
            &cookies,
            &Fixed(NOW),
            &proof,
            Some("a"),
            Action::Propose(&mapping),
        )
        .unwrap()
        .outcome
        .unwrap();
    let exact = Exact {
        mapping_id: "synthetic-map".into(),
        mapping_hash: proposed["mappingHash"].as_str().unwrap().into(),
    };
    assert!(
        owner
            .intercompany(
                &cookies,
                &Fixed(NOW),
                &proof,
                Some("a"),
                Action::Approve(&exact)
            )
            .unwrap()
            .outcome
            .is_err()
    );
    let c = owner.held.0.as_mut().unwrap().control.as_mut().unwrap();
    c.execute_batch("INSERT INTO user(id,name,email,emailVerified,twoFactorEnabled,createdAt,updatedAt)VALUES('r','Reviewer','r@example.invalid',1,1,'2026-01-01','2026-01-01');INSERT INTO twoFactor(id,secret,backupCodes,userId,verified)VALUES('rf','synthetic','[]','r',1);INSERT INTO session(id,userId,token,createdAt,updatedAt,expiresAt)VALUES('rs','r','reviewer-token','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z','2026-01-01T12:00:00.000Z');INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES('r','activate','workspace_owner','user:u');INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('r','a','grant','owner','user:u'),('r','b','grant','owner','user:u');").unwrap();
    let cookie = cookies.issue_session("reviewer-token").unwrap();
    let review = cookies
        .verify(Some(cookie.set_cookie().split(';').next().unwrap()), None)
        .unwrap();
    owner
        .intercompany(
            &cookies,
            &Fixed(NOW),
            &review,
            Some("a"),
            Action::Approve(&exact),
        )
        .unwrap()
        .outcome
        .unwrap();
    for slug in ["a", "b"] {
        owner.select_company(slug).unwrap();
        let c = owner.held.0.as_mut().unwrap().company.as_mut().unwrap();
        c.execute_batch("INSERT INTO accounts(account_no,name,type,normal_balance)VALUES('1400','Synthetic reciprocal','asset','debit'),('8000','Synthetic equity','equity','credit')").unwrap();
        let tx = c
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        let lines = if slug == "a" {
            json!([{"accountNo":"1400","debitAmount":"12.34"},{"accountNo":"8000","creditAmount":"12.34"}])
        } else {
            json!([{"accountNo":"1400","creditAmount":"12.34"},{"accountNo":"8000","debitAmount":"12.34"}])
        };
        let request=serde_json::from_value(json!({"transactionDate":"2026-01-01","text":"Synthetic source","currency":"DKK","lines":lines})).unwrap();
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
    let report = owner
        .intercompany(
            &cookies,
            &Fixed(NOW),
            &proof,
            None,
            Action::Reconciliation("2026-01-01"),
        )
        .unwrap()
        .outcome
        .unwrap();
    assert_eq!(report["rows"][0]["status"], "matched");
    assert_eq!(report["rows"][0]["difference"], "0.00");
    assert_eq!(report["rows"][0]["left"]["balance"], "12.34");
    assert_eq!(owner.slug, "b");
    owner.held.0.as_mut().unwrap().control.as_mut().unwrap().execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','b','revoke',NULL,'user:r')").unwrap();
    let hidden = owner
        .intercompany(
            &cookies,
            &Fixed(NOW),
            &proof,
            None,
            Action::Reconciliation("2026-01-01"),
        )
        .unwrap()
        .outcome
        .unwrap();
    assert_eq!(hidden["rows"], json!([]));
    assert!(
        owner
            .intercompany(
                &cookies,
                &Fixed(NOW),
                &proof,
                Some("a"),
                Action::Revoke(&exact)
            )
            .unwrap()
            .outcome
            .is_err()
    );
    let current = owner
        .intercompany(&cookies, &Fixed(NOW), &proof, Some("a"), Action::Current)
        .unwrap()
        .outcome
        .unwrap();
    assert!(current["manifest"].is_null());
    assert_eq!(current["mappings"], json!([]));
    owner
        .intercompany(
            &cookies,
            &Fixed(NOW),
            &review,
            Some("a"),
            Action::Revoke(&exact),
        )
        .unwrap()
        .outcome
        .unwrap();
    let empty = owner
        .intercompany(
            &cookies,
            &Fixed(NOW),
            &review,
            None,
            Action::Reconciliation("2026-01-01"),
        )
        .unwrap()
        .outcome
        .unwrap();
    assert_eq!(empty["rows"], json!([]));
}
