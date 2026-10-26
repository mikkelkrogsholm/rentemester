use super::*;
use rentemester_runtime::auth::{CookieProfile, CookieVerifier, SystemClock};
use serde_json::json;
#[test]
fn source_owner_real_bank_original_all_pages_receipt_revocation_and_selection() {
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
        "Synthetic sources",
        "2026-10-26T00:00:00.000Z",
        &[],
        NamespaceProfile::LocalUserMacos,
    )
    .unwrap();
    let cookies = CookieVerifier::new(
        "w",
        CookieProfile::Development,
        "QUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUE",
    )
    .unwrap();
    owner.held.0.as_mut().unwrap().control.as_mut().unwrap().execute_batch("INSERT INTO user(id,name,email,emailVerified,twoFactorEnabled,createdAt,updatedAt)VALUES('author','Synthetic','author@example.invalid',1,1,'2026-10-26','2026-10-26');INSERT INTO twoFactor(id,secret,backupCodes,userId,verified)VALUES('author','synthetic','[]','author',1);INSERT INTO session(id,userId,token,createdAt,updatedAt,expiresAt)VALUES('author','author','author-token',strftime('%Y-%m-%dT%H:%M:%fZ','now','-1 minute'),strftime('%Y-%m-%dT%H:%M:%fZ','now','-1 minute'),strftime('%Y-%m-%dT%H:%M:%fZ','now','+11 hours'));INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES('author','activate','workspace_owner','user:author');INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('author','a','grant','owner','user:author');").unwrap();
    let cookie = cookies.issue_session("author-token").unwrap();
    let proof = cookies
        .verify(Some(cookie.set_cookie().split(';').next().unwrap()), None)
        .unwrap();
    owner.held.0.as_mut().unwrap().company.as_mut().unwrap().execute_batch("UPDATE companies SET vat_period_type='quarter';INSERT INTO accounts(account_no,name,type,normal_balance)VALUES('A','Synthetic bank','asset','debit');INSERT INTO bank_accounts(slug,name,ledger_account_no)VALUES('synthetic','Synthetic bank','A');").unwrap();
    let mut csv = "date;text;amount\n".to_owned();
    for n in 0..101 {
        csv += &format!("2026-10-20;Synthetic source {n};-10\n");
    }
    owner
        .bank(
            &proof,
            &SystemClock,
            rentemester_application::bank::Action::Import {
                content: &csv,
                profile: None,
                account: Some("synthetic"),
                now: "2026-10-26T00:00:00Z",
            },
        )
        .unwrap()
        .outcome
        .unwrap();
    let input = json!({"companySlugs":["a"],"asOfDate":"2026-10-26"});
    let result = owner
        .tasks(
            &SystemClock,
            Authority::Cookie {
                cookies: &cookies,
                proof: &proof,
            },
            Action::Sync(&input),
            Some("all-pages"),
            None,
        )
        .unwrap()
        .outcome
        .unwrap();
    assert!(
        result["sync"]["errors"].as_array().unwrap().is_empty(),
        "{result}"
    );
    assert!(result["sync"]["created"].as_u64().unwrap() >= 101);
    let listed = owner
        .tasks(
            &SystemClock,
            Authority::Cookie {
                cookies: &cookies,
                proof: &proof,
            },
            Action::List(&json!({})),
            None,
            None,
        )
        .unwrap()
        .outcome
        .unwrap();
    assert_eq!(
        listed["tasks"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|t| t["source"]["kind"] == "bank_work")
            .count(),
        101
    );
    assert_eq!(owner.slug, "a");
    let before = owner
        .held
        .0
        .as_mut()
        .unwrap()
        .control
        .as_mut()
        .unwrap()
        .query_row::<i64, _, _>("SELECT count(*) FROM rm_task_events", [], |r| r.get(0))
        .unwrap();
    let replay = owner
        .tasks(
            &SystemClock,
            Authority::Cookie {
                cookies: &cookies,
                proof: &proof,
            },
            Action::Sync(&input),
            Some("all-pages"),
            None,
        )
        .unwrap()
        .outcome
        .unwrap();
    assert_eq!(replay, result);
    let connection = owner.held.0.as_mut().unwrap().control.as_mut().unwrap();
    assert_eq!(
        connection
            .query_row::<i64, _, _>("SELECT count(*) FROM rm_task_events", [], |r| r.get(0))
            .unwrap(),
        before
    );
    connection.execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('author','a','revoke',NULL,'user:author')").unwrap();
    let refused = owner
        .tasks(
            &SystemClock,
            Authority::Cookie {
                cookies: &cookies,
                proof: &proof,
            },
            Action::Sync(&input),
            Some("all-pages"),
            None,
        )
        .unwrap()
        .outcome
        .unwrap_err();
    assert_eq!(refused.error.code, "access_denied");
    assert_eq!(owner.slug, "a");
}
