use super::*;
use rentemester_runtime::auth::{AuthError, CookieProfile};
use rentemester_storage::workspace_context::RegisteredCompany;
struct Fixed;
impl Clock for Fixed {
    fn now_unix_ms(&self) -> Result<i64, AuthError> {
        Ok(1767225660000)
    }
}
fn fixture() -> (
    tempfile::TempDir,
    Connection,
    CookieVerifier,
    VerifiedCookie,
    Catalog,
) {
    let d = tempfile::tempdir().unwrap();
    let mut c = Connection::open(d.path().join("control")).unwrap();
    let mut l = Connection::open(d.path().join("ledger")).unwrap();
    rentemester_storage::fresh::initialize_empty_pair(&mut c, &mut l, "Synthetic Ownership")
        .unwrap();
    c.execute_batch("INSERT INTO user(id,name,email,emailVerified,twoFactorEnabled,createdAt,updatedAt) VALUES('u','Synthetic','u@example.invalid',1,1,'2026-01-01','2026-01-01');INSERT INTO twoFactor(id,secret,backupCodes,userId,verified) VALUES('f','synthetic','synthetic','u',1);INSERT INTO session(id,expiresAt,token,createdAt,updatedAt,userId) VALUES('s','2026-01-01T12:00:00.000Z','synthetic-token','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z','u');INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor) VALUES('u','activate','member','user:u');INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor) VALUES('u','a','grant','owner','user:u');INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor) VALUES('u','b','grant','owner','user:u');").unwrap();
    let f: Value = serde_json::from_str(include_str!(
        "../../../rentemester-runtime/fixtures/session-cookie.json"
    ))
    .unwrap();
    let cookies =
        CookieVerifier::new("w", CookieProfile::Secure, f["key"].as_str().unwrap()).unwrap();
    let proof = cookies
        .verify(f["vectors"][0]["header"].as_str(), None)
        .unwrap();
    let cat = Catalog {
        workspace: "w".into(),
        companies: ["a", "b"]
            .into_iter()
            .map(|s| RegisteredCompany {
                slug: s.into(),
                name: format!("Synthetic {s}"),
                archived: false,
            })
            .collect(),
    };
    (d, c, cookies, proof, cat)
}
fn reviewer(c: &Connection, cookies: &CookieVerifier) -> VerifiedCookie {
    c.execute_batch("INSERT INTO user(id,name,email,emailVerified,twoFactorEnabled,createdAt,updatedAt) VALUES('v','Synthetic Reviewer','v@example.invalid',1,1,'2026-01-01','2026-01-01');INSERT INTO twoFactor(id,secret,backupCodes,userId,verified) VALUES('fv','synthetic','synthetic','v',1);INSERT INTO session(id,expiresAt,token,createdAt,updatedAt,userId) VALUES('sv','2026-01-01T12:00:00.000Z','reviewer-token','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z','v');INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor) VALUES('v','activate','member','user:u');INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor) VALUES('v','a','grant','owner','user:u'),('v','b','grant','owner','user:u');").unwrap();
    let issued = cookies.issue_session("reviewer-token").unwrap();
    cookies.verify(Some(issued.request_header()), None).unwrap()
}
fn input(id: &str) -> Propose {
    serde_json::from_value(json!({"snapshotId":id,"source":"synthetic","observedAt":"2026-01-01","facts":[{"owner":{"kind":"company","companySlug":"a"},"ownedCompanySlug":"b","validFrom":"2026-01-01","economicBasisPoints":1000,"controlType":"equity","jurisdiction":"DK","evidenceRefs":["opaque-source"]}]})).unwrap()
}
#[test]
fn current_independent_roles_exact_apply_revoke_and_restart() {
    let (d, mut c, cookies, p, cat) = fixture();
    let v = reviewer(&c, &cookies);
    let i = input("initial");
    let s =
        execute(&mut c, &cookies, &Fixed, &p, &cat, "a", Action::Propose(&i)).unwrap()["snapshot"]
            .clone();
    let r = Review {
        snapshot_id: "initial".into(),
        decision: "approved".into(),
    };
    assert!(execute(&mut c, &cookies, &Fixed, &p, &cat, "a", Action::Review(&r)).is_err());
    execute(&mut c, &cookies, &Fixed, &v, &cat, "a", Action::Review(&r)).unwrap();
    let a = Apply {
        snapshot_id: "initial".into(),
        snapshot_hash: s["snapshotHash"].as_str().unwrap().into(),
        diff_hash: s["diffHash"].as_str().unwrap().into(),
    };
    c.execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,actor) VALUES('v','b','revoke','user:u')").unwrap();
    assert!(execute(&mut c, &cookies, &Fixed, &v, &cat, "a", Action::Apply(&a)).is_err());
    c.execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor) VALUES('v','b','grant','owner','user:u')").unwrap();
    assert_eq!(
        execute(&mut c, &cookies, &Fixed, &v, &cat, "a", Action::Apply(&a)).unwrap()["status"],
        "applied"
    );
    let before = execute(
        &mut c,
        &cookies,
        &Fixed,
        &p,
        &cat,
        "a",
        Action::Read("2026-01-01"),
    )
    .unwrap();
    assert_eq!(before["facts"].as_array().unwrap().len(), 1);
    drop(c);
    let mut c = Connection::open(d.path().join("control")).unwrap();
    assert_eq!(
        execute(
            &mut c,
            &cookies,
            &Fixed,
            &p,
            &cat,
            "a",
            Action::Read("2026-01-01")
        )
        .unwrap(),
        before
    );
    c.execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,actor) VALUES('u','b','revoke','user:u')").unwrap();
    let hidden = execute(
        &mut c,
        &cookies,
        &Fixed,
        &p,
        &cat,
        "a",
        Action::Read("2026-01-01"),
    )
    .unwrap();
    assert_eq!(hidden["facts"], json!([]));
    assert_eq!(hidden["partial"], true);
    assert_eq!(
        execute(
            &mut c,
            &cookies,
            &Fixed,
            &p,
            &cat,
            "a",
            Action::History {
                snapshot_id: None,
                as_of: Some("2026-01-01")
            }
        )
        .unwrap()["history"],
        json!([])
    );
    c.execute("DELETE FROM session WHERE id='s'", []).unwrap();
    assert!(
        execute(
            &mut c,
            &cookies,
            &Fixed,
            &p,
            &cat,
            "a",
            Action::Read("2026-01-01")
        )
        .is_err()
    );
}
#[test]
fn company_admin_is_not_inferred_from_workspace_role_and_archived_writes_refuse() {
    let (_d, mut c, cookies, p, mut cat) = fixture();
    let v = reviewer(&c, &cookies);
    c.execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor) VALUES('u','a','grant','bookkeeper','user:u'),('u','b','grant','bookkeeper','user:u');INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor) VALUES('u','activate','workspace_owner','user:u')").unwrap();
    execute(
        &mut c,
        &cookies,
        &Fixed,
        &p,
        &cat,
        "a",
        Action::Propose(&input("roles")),
    )
    .unwrap();
    let r = Review {
        snapshot_id: "roles".into(),
        decision: "approved".into(),
    };
    assert!(execute(&mut c, &cookies, &Fixed, &p, &cat, "a", Action::Review(&r)).is_err());
    cat.companies[1].archived = true;
    assert!(execute(&mut c, &cookies, &Fixed, &v, &cat, "a", Action::Review(&r)).is_err());
    assert!(
        execute(
            &mut c,
            &cookies,
            &Fixed,
            &p,
            &cat,
            "a",
            Action::Propose(&input("archived"))
        )
        .is_err()
    );
}
#[test]
fn sensitive_corporate_and_party_references_recheck_current_source_visibility() {
    let (_d, mut c, cookies, p, cat) = fixture();
    let i:rentemester_storage::corporate_records::Ingest=serde_json::from_value(json!({"recordId":"record-sensitive","type":"articles","bytesBase64":"unused","filename":"synthetic.txt","source":"synthetic","receivedAt":"2026-01-01","uploader":"synthetic","sensitivity":"restricted"})).unwrap();
    crate::corporate_records::execute(
        &mut c,
        &cookies,
        &Fixed,
        &p,
        &cat,
        "a",
        crate::corporate_records::Action::Ingest(&i, b"synthetic evidence"),
    )
    .unwrap();
    let mut input = input("sensitive");
    input.facts[0].evidence_refs = vec!["record-sensitive".into()];
    let receipt = execute(
        &mut c,
        &cookies,
        &Fixed,
        &p,
        &cat,
        "a",
        Action::Propose(&input),
    )
    .unwrap()["snapshot"]
        .clone();
    let v = reviewer(&c, &cookies);
    execute(
        &mut c,
        &cookies,
        &Fixed,
        &v,
        &cat,
        "a",
        Action::Review(&Review {
            snapshot_id: "sensitive".into(),
            decision: "approved".into(),
        }),
    )
    .unwrap();
    execute(
        &mut c,
        &cookies,
        &Fixed,
        &v,
        &cat,
        "a",
        Action::Apply(&Apply {
            snapshot_id: "sensitive".into(),
            snapshot_hash: receipt["snapshotHash"].as_str().unwrap().into(),
            diff_hash: receipt["diffHash"].as_str().unwrap().into(),
        }),
    )
    .unwrap();
    c.execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor) VALUES('u','a','grant','reviewer','user:u')").unwrap();
    assert!(
        execute(
            &mut c,
            &cookies,
            &Fixed,
            &p,
            &cat,
            "a",
            Action::Propose(&input)
        )
        .is_err()
    );
    assert_eq!(
        execute(
            &mut c,
            &cookies,
            &Fixed,
            &p,
            &cat,
            "a",
            Action::History {
                snapshot_id: None,
                as_of: None
            }
        )
        .unwrap()["history"],
        json!([])
    );
    // A new opaque source must not expose the old restricted observation
    // through the proposed diff's conflicts, or end it without current rights.
    input.snapshot_id = Some("hidden-conflict".into());
    input.facts[0].valid_from = "2026-06-01".into();
    input.facts[0].evidence_refs = vec!["opaque".into()];
    assert!(
        execute(
            &mut c,
            &cookies,
            &Fixed,
            &p,
            &cat,
            "a",
            Action::Propose(&input)
        )
        .is_err()
    );
    input.snapshot_id = Some("missing-party".into());
    input.facts[0].evidence_refs = vec!["opaque".into()];
    input.facts[0].owner = Endpoint::Party {
        party_id: "missing".into(),
    };
    assert!(
        execute(
            &mut c,
            &cookies,
            &Fixed,
            &p,
            &cat,
            "a",
            Action::Propose(&input)
        )
        .is_err()
    );
}
