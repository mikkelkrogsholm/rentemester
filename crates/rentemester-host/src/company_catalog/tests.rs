use super::*;
use rentemester_runtime::auth::{AuthError, CookieProfile};
const KEY: &str = "QUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUE";
struct Fixed;
impl Clock for Fixed {
    fn now_unix_ms(&self) -> Result<i64, AuthError> {
        Ok(1767225600000)
    }
}
fn input(slug: &str) -> app::Input {
    serde_json::from_value(json!({"name":format!("Synthetic {slug}"),"slug":slug,"fiscalYearStartMonth":"4","vatPeriodType":null})).unwrap()
}
fn setup() -> (
    tempfile::TempDir,
    Owner,
    CookieVerifier,
    VerifiedCookie,
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
    let c = owner.held.0.as_mut().unwrap().control.as_mut().unwrap();
    c.execute_batch("INSERT INTO user(id,name,email,emailVerified,twoFactorEnabled,createdAt,updatedAt)VALUES('u','Synthetic Owner','owner@example.invalid',1,1,'2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z'),('r','Synthetic Reader','reader@example.invalid',1,1,'2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z');INSERT INTO session(id,userId,token,createdAt,updatedAt,expiresAt)VALUES('s','u','owner-token','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z','2026-01-01T12:00:00.000Z'),('t','r','reader-token','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z','2026-01-01T12:00:00.000Z');INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES('u','activate','workspace_owner','user:u'),('r','activate','member','user:u');INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','grant','owner','user:u'),('r','a','grant','reader','user:u');").unwrap();
    c.execute_batch("INSERT INTO twoFactor(id,secret,backupCodes,userId,verified) VALUES('factor-u','synthetic','[]','u',1),('factor-r','synthetic','[]','r',1)").unwrap();
    let cookies = CookieVerifier::new("w", CookieProfile::Development, KEY).unwrap();
    let proof = |token| {
        let session = cookies.issue_session(token).unwrap();
        let header = session.set_cookie().split(';').next().unwrap();
        cookies.verify(Some(header), None).unwrap()
    };
    let u = proof("owner-token");
    let r = proof("reader-token");
    (dir, owner, cookies, u, r, uid)
}
fn uuid(owner: &mut Owner) -> String {
    owner
        .held
        .0
        .as_mut()
        .unwrap()
        .company
        .as_ref()
        .unwrap()
        .query_row("SELECT ledger_uuid FROM ledger_identity", [], |r| r.get(0))
        .unwrap()
}
#[test]
fn company_creation_current_permissions_distinct_ledgers_switch_reopen_and_concurrent_dispatch() {
    let (dir, mut owner, cookies, proof, reader, uid) = setup();
    let primary = uuid(&mut owner);
    let denied = owner
        .create_company(&cookies, &Fixed, &reader, &input("denied"))
        .unwrap();
    assert_eq!(denied.outcome.unwrap_err().error.code, "access_denied");
    assert!(!denied.retired);
    assert!(!dir.path().join("workspace/denied").exists());
    assert!(
        owner
            .create_company(&cookies, &Fixed, &proof, &input("b"))
            .unwrap()
            .outcome
            .is_ok()
    );
    assert_eq!(owner.catalog().unwrap().companies.len(), 2);
    owner.select_company("b").unwrap();
    let second = uuid(&mut owner);
    assert_ne!(primary, second);
    assert!(
        owner
            .company_setup(
                &reader,
                &Fixed,
                rentemester_application::company_setup::Action::Profile
            )
            .unwrap()
            .outcome
            .is_err()
    );
    assert!(
        owner
            .company_setup(
                &proof,
                &Fixed,
                rentemester_application::company_setup::Action::Profile
            )
            .unwrap()
            .outcome
            .is_ok()
    );
    assert_eq!(
        owner
            .create_company(&cookies, &Fixed, &proof, &input("b"))
            .unwrap()
            .outcome
            .unwrap_err()
            .error
            .code,
        "company_exists"
    );
    for (slug, amount) in [("a", "12.34"), ("b", "56.78")] {
        owner.select_company(slug).unwrap();
        for (no, kind, normal) in [("1759", "asset", "debit"), ("8759", "equity", "credit")] {
            let account=serde_json::from_value(json!({"accountNo":no,"name":format!("Synthetic {slug} {no}"),"type":kind,"normalBalance":normal})).unwrap();
            assert!(
                owner
                    .company_setup(
                        &proof,
                        &Fixed,
                        rentemester_application::company_setup::Action::Create(&account)
                    )
                    .unwrap()
                    .outcome
                    .is_ok()
            );
        }
        let input=serde_json::from_value(json!({"format":journal_post::FORMAT,"expectedRuleVersion":rentemester_storage::manual_journal_rule_version().unwrap(),"idempotencyKey":"synthetic-same-key-distinct-ledgers","request":{"transactionDate":"2026-01-01","text":format!("Synthetic isolated {slug}"),"currency":"DKK","lines":[{"accountNo":"1759","debitAmount":amount,"creditAmount":"0.00"},{"accountNo":"8759","debitAmount":"0.00","creditAmount":amount}]}})).unwrap();
        assert!(
            owner
                .post_human(&proof, &Fixed, &journal_post::Policy::default(), input)
                .unwrap()
                .outcome
                .is_ok()
        );
    }
    for (slug, amount) in [("a", 12.34), ("b", 56.78)] {
        owner.select_company(slug).unwrap();
        let v = owner
            .cockpit_snapshot(
                &proof,
                &Fixed,
                rentemester_application::owned_report::CockpitRequest {
                    today: "2026-01-01",
                    year: None,
                    account: None,
                },
            )
            .unwrap()
            .outcome
            .unwrap()
            .value;
        assert_eq!(v.journal["slug"], slug);
        assert_eq!(v.journal["entries"].as_array().unwrap().len(), 1);
        assert_eq!(v.journal["entries"][0]["total"], amount);
    }
    let shared = std::sync::Arc::new(std::sync::Mutex::new(owner));
    let mut handles = vec![];
    for slug in ["a", "b"] {
        let shared = shared.clone();
        let proof = cookies
            .verify(
                Some(
                    cookies
                        .issue_session("owner-token")
                        .unwrap()
                        .set_cookie()
                        .split(';')
                        .next()
                        .unwrap(),
                ),
                None,
            )
            .unwrap();
        let expected = if slug == "a" {
            primary.clone()
        } else {
            second.clone()
        };
        handles.push(std::thread::spawn(move || {
            for _ in 0..4 {
                let mut o = shared.lock().unwrap();
                o.select_company(slug).unwrap();
                assert_eq!(uuid(&mut o), expected);
                let v = o
                    .company_setup(
                        &proof,
                        &Fixed,
                        rentemester_application::company_setup::Action::Accounts,
                    )
                    .unwrap()
                    .outcome
                    .unwrap()
                    .value;
                assert_eq!(v["slug"], slug);
            }
        }));
    }
    for h in handles {
        h.join().unwrap();
    }
    let owner = std::sync::Arc::try_unwrap(shared)
        .ok()
        .unwrap()
        .into_inner()
        .unwrap();
    owner.try_close().unwrap();
    let mut reopened = Owner::open_with_profile(
        &dir.path().canonicalize().unwrap().join("workspace"),
        uid,
        "w",
        "a",
        NamespaceProfile::LocalUserMacos,
    )
    .unwrap();
    assert_eq!(uuid(&mut reopened), primary);
    reopened.select_company("b").unwrap();
    assert_eq!(uuid(&mut reopened), second);
    assert_eq!(reopened.catalog().unwrap().companies.len(), 2);
    let v = reopened
        .company_setup(
            &proof,
            &Fixed,
            rentemester_application::company_setup::Action::Profile,
        )
        .unwrap()
        .outcome
        .unwrap()
        .value;
    assert_eq!(v["vatPeriodType"], Value::Null);
    assert_eq!(v["fiscalYearStartMonth"], 4);
}

#[test]
fn private_partial_company_is_never_adopted_or_registered_after_restart() {
    let (dir, mut owner, cookies, proof, _, uid) = setup();
    let root = dir.path().canonicalize().unwrap().join("workspace");
    std::fs::create_dir(root.join("partial")).unwrap();
    let attempt = owner
        .create_company(&cookies, &Fixed, &proof, &input("partial"))
        .unwrap();
    assert!(attempt.outcome.is_err());
    assert!(attempt.retired);
    owner.try_close().unwrap();
    let mut owner =
        Owner::open_with_profile(&root, uid, "w", "a", NamespaceProfile::LocalUserMacos).unwrap();
    assert_eq!(owner.catalog().unwrap().companies.len(), 1);
    assert!(!root.join("partial/data/ledger.sqlite").exists());
    assert!(
        owner
            .create_company(&cookies, &Fixed, &proof, &input("partial"))
            .unwrap()
            .outcome
            .is_err()
    );
}
