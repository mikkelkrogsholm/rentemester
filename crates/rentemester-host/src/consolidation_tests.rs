//! Synthetic unit fixtures only. Browser acceptance uses normal verification/MFA.
use super::*;
use rentemester_application::consolidation::{Action, Exact, Id};
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
fn reviewed_profile_real_ready_report_current_permissions_and_immutable_evidence() {
    let (dir, mut owner, cookies, proof, uid) = setup();
    let manifest:rentemester_engine::intercompany::Manifest=serde_json::from_value(json!({"version":1,"groups":[{"id":"synthetic","name":"Synthetic","memberships":[{"id":"a","companySlug":"a","validFrom":"2026-01-01"},{"id":"b","companySlug":"b","validFrom":"2026-01-01"}],"ownership":[{"id":"owns-b","parentCompanySlug":"a","childCompanySlug":"b","basisPoints":10000,"evidenceRefs":["synthetic"],"validFrom":"2026-01-01"}]}]})).unwrap();
    owner
        .intercompany(
            &cookies,
            &Fixed(NOW),
            &proof,
            Some("a"),
            rentemester_application::intercompany::Action::Manifest(&manifest),
        )
        .unwrap()
        .outcome
        .unwrap();
    let profile=serde_json::from_value(json!({"version":1,"id":"synthetic-profile","groupId":"synthetic","currency":"DKK","reportingLines":[{"id":"assets","label":"Assets","section":"asset","displayOrder":1},{"id":"debt","label":"Debt","section":"liability","displayOrder":2},{"id":"equity","label":"Equity","section":"equity","displayOrder":3},{"id":"result","label":"Result","section":"equity","displayOrder":4,"role":"current-result"},{"id":"sales","label":"Sales","section":"income","displayOrder":5},{"id":"costs","label":"Costs","section":"expense","displayOrder":6}],"accountMappings":[{"id":"a-bank","companySlug":"a","accountNo":"1000","reportingLineId":"assets","validFrom":"2026-01-01"},{"id":"a-sales","companySlug":"a","accountNo":"8000","reportingLineId":"equity","validFrom":"2026-01-01"},{"id":"b-bank","companySlug":"b","accountNo":"1000","reportingLineId":"assets","validFrom":"2026-01-01"},{"id":"b-sales","companySlug":"b","accountNo":"8000","reportingLineId":"equity","validFrom":"2026-01-01"}],"evidenceRefs":["synthetic"],"validFrom":"2026-01-01"})).unwrap();
    let proposed = owner
        .consolidation(
            &cookies,
            &Fixed(NOW),
            &proof,
            Some("a"),
            Action::Propose(&profile),
        )
        .unwrap()
        .outcome
        .unwrap();
    let exact = Exact {
        profile_id: "synthetic-profile".into(),
        profile_hash: proposed["profileHash"].as_str().unwrap().into(),
    };
    assert!(
        owner
            .consolidation(
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
    let wrong = Exact {
        profile_id: exact.profile_id.clone(),
        profile_hash: "0".repeat(64),
    };
    assert!(
        owner
            .consolidation(
                &cookies,
                &Fixed(NOW),
                &review,
                Some("a"),
                Action::Approve(&wrong)
            )
            .unwrap()
            .outcome
            .is_err()
    );
    owner
        .consolidation(
            &cookies,
            &Fixed(NOW),
            &review,
            Some("a"),
            Action::Approve(&exact),
        )
        .unwrap()
        .outcome
        .unwrap();
    let mut heads = vec![];
    for slug in ["a", "b"] {
        owner.select_company(slug).unwrap();
        let c = owner.held.0.as_mut().unwrap().company.as_mut().unwrap();
        c.execute_batch("INSERT INTO accounts(account_no,name,type,normal_balance)VALUES('1000','Synthetic bank','asset','debit'),('8000','Synthetic equity','equity','credit')").unwrap();
        let tx = c
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        let input=serde_json::from_value(json!({"transactionDate":"2026-01-01","text":"Synthetic source","currency":"DKK","lines":[{"accountNo":"1000","debitAmount":"12.34"},{"accountNo":"8000","creditAmount":"12.34"}]})).unwrap();
        rentemester_storage::journal_post::post(
            &tx,
            &input,
            &rentemester_storage::journal_preview::manual_journal_rule_version().unwrap(),
            rentemester_engine::fiscal_year::Date::parse("2026-01-01").unwrap(),
            rentemester_storage::journal_post::Actor {
                subject: "user:synthetic",
                program: "test",
            },
        )
        .unwrap();
        tx.commit().unwrap();
        heads.push(
            c.query_row("SELECT entry_hash FROM journal_entries", [], |r| {
                r.get::<_, String>(0)
            })
            .unwrap(),
        );
    }
    let action = Action::Plan {
        profile_id: "synthetic-profile",
        from: "2026-01-01",
        as_of: "2026-12-31",
    };
    let r = owner
        .consolidation(&cookies, &Fixed(NOW), &proof, Some("a"), action)
        .unwrap()
        .outcome
        .unwrap();
    assert_eq!(r["status"], "ready");
    assert_eq!(r["consolidatedFigures"][0]["consolidatedAmount"], "24.68");
    assert_eq!(r["sourceSnapshots"].as_array().unwrap().len(), 2);
    assert_eq!(owner.slug, "b");
    let mut minority = manifest.clone();
    minority.groups[0].ownership[0].basis_points = 5000;
    owner
        .intercompany(
            &cookies,
            &Fixed(NOW),
            &proof,
            Some("a"),
            rentemester_application::intercompany::Action::Manifest(&minority),
        )
        .unwrap()
        .outcome
        .unwrap();
    let blocked = owner
        .consolidation(&cookies, &Fixed(NOW), &proof, Some("a"), action)
        .unwrap()
        .outcome
        .unwrap();
    assert!(blocked["consolidatedFigures"].is_null());
    assert!(blocked["blockers"].to_string().contains("100 percent"));
    assert_eq!(blocked["sourceSnapshots"], json!([]));
    owner
        .intercompany(
            &cookies,
            &Fixed(NOW),
            &proof,
            Some("a"),
            rentemester_application::intercompany::Action::Manifest(&manifest),
        )
        .unwrap()
        .outcome
        .unwrap();
    let root = dir.path().canonicalize().unwrap().join("workspace");
    drop(owner);
    let mut owner =
        Owner::open_with_profile(&root, uid, "w", "a", NamespaceProfile::LocalUserMacos).unwrap();
    assert_eq!(
        owner
            .consolidation(&cookies, &Fixed(NOW), &proof, Some("a"), action)
            .unwrap()
            .outcome
            .unwrap(),
        r
    );
    let c = owner.held.0.as_mut().unwrap().control.as_mut().unwrap();
    let before: String = c
        .query_row(
            "SELECT group_concat(event_hash) FROM rm_consolidation_profile_events",
            [],
            |r| r.get(0),
        )
        .unwrap();
    c.execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','b','revoke',NULL,'user:r')").unwrap();
    let hidden = owner
        .consolidation(&cookies, &Fixed(NOW), &proof, Some("a"), action)
        .unwrap()
        .outcome
        .unwrap();
    assert!(hidden.get("profileId").is_none());
    assert_eq!(hidden["sourceSnapshots"], json!([]));
    assert_eq!(
        hidden["blockers"],
        json!(["all active group companies must be visible"])
    );
    let id = Id {
        profile_id: "synthetic-profile".into(),
    };
    assert!(
        owner
            .consolidation(
                &cookies,
                &Fixed(NOW),
                &proof,
                Some("a"),
                Action::Revoke(&id)
            )
            .unwrap()
            .outcome
            .is_err()
    );
    let current = owner
        .consolidation(
            &cookies,
            &Fixed(NOW),
            &proof,
            Some("a"),
            Action::Current("2026-12-31"),
        )
        .unwrap()
        .outcome
        .unwrap();
    assert_eq!(current["profiles"], json!([]));
    assert_eq!(current["availableProfiles"], json!([]));
    let c = owner.held.0.as_mut().unwrap().control.as_mut().unwrap();
    assert_eq!(
        before,
        c.query_row(
            "SELECT group_concat(event_hash) FROM rm_consolidation_profile_events",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap()
    );
    owner
        .consolidation(
            &cookies,
            &Fixed(NOW),
            &review,
            Some("a"),
            Action::Revoke(&id),
        )
        .unwrap()
        .outcome
        .unwrap();
    assert!(
        owner
            .consolidation(&cookies, &Fixed(NOW), &review, Some("a"), action)
            .unwrap()
            .outcome
            .is_err()
    );
    for (index, slug) in ["a", "b"].iter().enumerate() {
        owner.select_company(slug).unwrap();
        let c = owner.held.0.as_mut().unwrap().company.as_mut().unwrap();
        assert_eq!(
            c.query_row("SELECT count(*) FROM journal_entries", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            c.query_row("SELECT entry_hash FROM journal_entries", [], |r| r
                .get::<_, String>(0))
                .unwrap(),
            heads[index]
        );
    }
    let c = owner.held.0.as_mut().unwrap().control.as_mut().unwrap();
    assert!(
        c.execute(
            "UPDATE rm_consolidation_profile_events SET actor='user:changed'",
            []
        )
        .is_err()
    );
    c.execute("INSERT INTO rm_consolidation_profile_events(id,profile_id,event_type,profile_hash,canonical_profile,previous_hash,event_hash,actor,created_at) SELECT 4,profile_id,'proposed',profile_hash,canonical_profile,NULL,?,actor,created_at FROM rm_consolidation_profile_events WHERE id=1",["0".repeat(64)]).unwrap();
    assert!(
        owner
            .consolidation(
                &cookies,
                &Fixed(NOW),
                &review,
                Some("a"),
                Action::Current("2026-12-31")
            )
            .unwrap()
            .outcome
            .is_err()
    );
}
