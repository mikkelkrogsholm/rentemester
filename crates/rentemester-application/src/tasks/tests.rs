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
    rentemester_storage::fresh::initialize_empty_pair(&mut c, &mut l, "Synthetic Knowledge")
        .unwrap();
    c.execute_batch("INSERT INTO user(id,name,email,emailVerified,twoFactorEnabled,createdAt,updatedAt) VALUES('human-one','Synthetic','u@example.invalid',1,1,'2026-01-01','2026-01-01');INSERT INTO twoFactor(id,secret,backupCodes,userId,verified) VALUES('f','synthetic','synthetic','human-one',1);INSERT INTO session(id,expiresAt,token,createdAt,updatedAt,userId) VALUES('s','2026-01-01T12:00:00.000Z','synthetic-token','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z','human-one');INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor) VALUES('human-one','activate','member','user:human-one');INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor) VALUES('human-one','a','grant','bookkeeper','user:human-one');INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor) VALUES('human-one','b','grant','bookkeeper','user:human-one');").unwrap();
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
                purpose: Default::default(),
                archived: false,
            })
            .collect(),
    };
    (d, c, cookies, proof, cat)
}

fn call(
    c: &mut Connection,
    cookies: &CookieVerifier,
    p: &VerifiedCookie,
    cat: &Catalog,
    action: Action<'_>,
    key: Option<&str>,
    version: Option<u64>,
) -> Result<Value, Failure> {
    execute(
        c,
        cat,
        &Fixed,
        Authority::Cookie { cookies, proof: p },
        action,
        key,
        version,
    )
}
fn draft() -> Value {
    json!({"title":"Kontrollér perioden","scope":{"kind":"company","companySlug":"a"},"evidenceRequired":true})
}
#[test]
fn manual_journey_replays_exact_result_survives_restart_and_preserves_audit() {
    let (d, mut c, cookies, p, cat) = fixture();
    let input = draft();
    let first = call(
        &mut c,
        &cookies,
        &p,
        &cat,
        Action::Create(&input),
        Some("create"),
        None,
    )
    .unwrap();
    let id = first["task"]["taskId"].as_str().unwrap().to_owned();
    let patch = json!({"description":"Bilag kontrolleret","references":[{"kind":"document","ref":"doc:a:7","companySlug":"a"}]});
    call(
        &mut c,
        &cookies,
        &p,
        &cat,
        Action::Update {
            id: &id,
            patch: &patch,
        },
        Some("update"),
        Some(1),
    )
    .unwrap();
    let mov = json!({"status":"in_progress"});
    call(
        &mut c,
        &cookies,
        &p,
        &cat,
        Action::Move {
            id: &id,
            input: &mov,
        },
        Some("move"),
        Some(2),
    )
    .unwrap();
    let complete = json!({"outcome":"completed","note":"Kontrolleret mod bilaget","references":[{"kind":"document","ref":"doc:a:7","companySlug":"a"}]});
    call(
        &mut c,
        &cookies,
        &p,
        &cat,
        Action::Complete {
            id: &id,
            input: &complete,
        },
        Some("complete"),
        Some(3),
    )
    .unwrap();
    call(
        &mut c,
        &cookies,
        &p,
        &cat,
        Action::Reopen {
            id: &id,
            reason: &json!("Nyt bilag kræver kontrol"),
        },
        Some("reopen"),
        Some(4),
    )
    .unwrap();
    assert_eq!(
        call(
            &mut c,
            &cookies,
            &p,
            &cat,
            Action::Create(&input),
            Some("create"),
            None
        )
        .unwrap(),
        first
    );
    drop(c);
    let mut c = Connection::open(d.path().join("control")).unwrap();
    let result = call(&mut c, &cookies, &p, &cat, Action::Get(&id), None, None).unwrap();
    assert_eq!(result["task"]["version"], 5);
    assert_eq!(result["history"].as_array().unwrap().len(), 5);
    assert_eq!(
        call(
            &mut c,
            &cookies,
            &p,
            &cat,
            Action::Receipt("create"),
            None,
            None
        )
        .unwrap()["receipt"]["version"],
        1
    );
    let view = call(
        &mut c,
        &cookies,
        &p,
        &cat,
        Action::List(&json!({"from":"2027-01-01"})),
        None,
        None,
    )
    .unwrap();
    assert_eq!(view["count"], 1);
    assert_eq!(view["currentUserId"], "human-one");
    assert_eq!(
        c.query_row("SELECT count(*) FROM rm_task_receipts", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        5
    );
    assert_eq!(
        Connection::open(d.path().join("ledger"))
            .unwrap()
            .query_row("SELECT count(*) FROM journal_entries", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
}
#[test]
fn fresh_session_and_every_company_scope_are_checked_before_receipt() {
    let (_d, mut c, cookies, p, cat) = fixture();
    let input =
        json!({"title":"Fælles kontrol","scope":{"kind":"workspace","companySlugs":["a","b"]}});
    call(
        &mut c,
        &cookies,
        &p,
        &cat,
        Action::Create(&input),
        Some("create"),
        None,
    )
    .unwrap();
    c.execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor) VALUES('human-one','b','revoke',NULL,'user:human-one');").unwrap();
    assert_eq!(
        call(
            &mut c,
            &cookies,
            &p,
            &cat,
            Action::Create(&input),
            Some("create"),
            None
        )
        .unwrap_err()
        .error
        .code,
        "access_denied"
    );
    assert_eq!(
        call(
            &mut c,
            &cookies,
            &p,
            &cat,
            Action::Receipt("create"),
            None,
            None
        )
        .unwrap_err()
        .error
        .code,
        "access_denied"
    );
    c.execute(
        "UPDATE user SET twoFactorEnabled=0 WHERE id='human-one'",
        [],
    )
    .unwrap();
    assert!(
        call(
            &mut c,
            &cookies,
            &p,
            &cat,
            Action::List(&json!({})),
            None,
            None
        )
        .is_err()
    );
    assert_eq!(
        c.query_row("SELECT count(*) FROM rm_task_events", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
}
#[test]
fn task_roles_archived_live_and_nonlive_are_distinct_from_financial_access() {
    let (_d, mut c, cookies, p, mut cat) = fixture();
    cat.companies[0].archived = true;
    assert!(
        call(
            &mut c,
            &cookies,
            &p,
            &cat,
            Action::Create(&draft()),
            Some("archived"),
            None
        )
        .is_ok()
    );
    cat.companies[0].purpose = rentemester_engine::workspace_manifest::Purpose::Test;
    assert_eq!(
        call(
            &mut c,
            &cookies,
            &p,
            &cat,
            Action::Create(&draft()),
            Some("nonlive"),
            None
        )
        .unwrap_err()
        .error
        .code,
        "access_denied"
    );
    cat.companies[0].purpose = Default::default();
    c.execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor) VALUES('human-one','a','grant','reader','user:human-one');").unwrap();
    assert_eq!(
        call(
            &mut c,
            &cookies,
            &p,
            &cat,
            Action::Create(&draft()),
            Some("reader"),
            None
        )
        .unwrap_err()
        .error
        .code,
        "access_denied"
    );
    assert_eq!(
        call(
            &mut c,
            &cookies,
            &p,
            &cat,
            Action::List(&json!({"includeArchived":true})),
            None,
            None
        )
        .unwrap()["count"],
        1
    );
}
#[test]
fn rejects_internal_authority_fields_and_inaccessible_assignees_without_effect() {
    let (_d, mut c, cookies, p, cat) = fixture();
    for (key, value) in [
        ("actor", json!("user:owner")),
        ("local", json!(true)),
        ("taskId", json!("chosen")),
        ("source", json!({})),
        ("status", json!("done")),
    ] {
        let mut i = draft();
        i[key] = value;
        assert!(
            call(
                &mut c,
                &cookies,
                &p,
                &cat,
                Action::Create(&i),
                Some("invalid"),
                None
            )
            .is_err()
        );
    }
    let mut i = draft();
    i["assignee"] = json!({"kind":"member","userId":"hidden-user","name":"Synthetic recipient"});
    assert_eq!(
        call(
            &mut c,
            &cookies,
            &p,
            &cat,
            Action::Create(&i),
            Some("invalid"),
            None
        )
        .unwrap_err()
        .error
        .code,
        "invalid_assignee"
    );
    assert_eq!(
        c.query_row("SELECT count(*) FROM rm_task_events", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
}
#[test]
fn scope_changes_filter_history_but_never_move_completion_evidence_outside_scope() {
    let (_d, mut c, cookies, p, cat) = fixture();
    let create = json!({"title":"Kontrol","scope":{"kind":"company","companySlug":"a"}});
    let first = call(
        &mut c,
        &cookies,
        &p,
        &cat,
        Action::Create(&create),
        Some("create"),
        None,
    )
    .unwrap();
    let id = first["task"]["taskId"].as_str().unwrap();
    let complete = json!({"outcome":"completed","note":"Bevis kontrolleret","references":[{"kind":"document","ref":"doc:a:7","companySlug":"a"}]});
    call(
        &mut c,
        &cookies,
        &p,
        &cat,
        Action::Complete {
            id,
            input: &complete,
        },
        Some("complete"),
        Some(1),
    )
    .unwrap();
    let patch = json!({"scope":{"kind":"company","companySlug":"b"}});
    assert!(
        call(
            &mut c,
            &cookies,
            &p,
            &cat,
            Action::Update { id, patch: &patch },
            Some("move-scope"),
            Some(2)
        )
        .is_err()
    );
    let second = call(
        &mut c,
        &cookies,
        &p,
        &cat,
        Action::Create(&create),
        Some("second"),
        None,
    )
    .unwrap();
    let id = second["task"]["taskId"].as_str().unwrap();
    call(
        &mut c,
        &cookies,
        &p,
        &cat,
        Action::Update { id, patch: &patch },
        Some("move-second"),
        Some(1),
    )
    .unwrap();
    c.execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor) VALUES('human-one','a','revoke',NULL,'user:human-one');").unwrap();
    let visible = call(&mut c, &cookies, &p, &cat, Action::Get(id), None, None).unwrap();
    assert_eq!(visible["history"].as_array().unwrap().len(), 1);
    assert_eq!(visible["history"][0]["version"], 2);
    assert_eq!(
        call(
            &mut c,
            &cookies,
            &p,
            &cat,
            Action::Receipt("second"),
            None,
            None
        )
        .unwrap_err()
        .error
        .code,
        "access_denied"
    );
}
#[test]
fn revoked_assignee_can_be_removed_and_historical_receipt_still_replays() {
    let (_d, mut c, cookies, p, cat) = fixture();
    c.execute_batch("INSERT INTO user(id,name,email,emailVerified,twoFactorEnabled,createdAt,updatedAt)VALUES('recipient','Synthetic recipient','recipient@example.invalid',1,1,'2026-01-01','2026-01-01');INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES('recipient','activate','member','user:human-one');INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('recipient','a','grant','reader','user:human-one');").unwrap();
    let mut input = draft();
    input["assignee"] = json!({"kind":"member","userId":"recipient","name":"Synthetic recipient"});
    let first = call(
        &mut c,
        &cookies,
        &p,
        &cat,
        Action::Create(&input),
        Some("create"),
        None,
    )
    .unwrap();
    let id = first["task"]["taskId"].as_str().unwrap();
    c.execute(
        "UPDATE user SET twoFactorEnabled=0 WHERE id='recipient'",
        [],
    )
    .unwrap();
    assert!(
        call(
            &mut c,
            &cookies,
            &p,
            &cat,
            Action::Create(&input),
            Some("another"),
            None
        )
        .is_err()
    );
    assert_eq!(
        call(
            &mut c,
            &cookies,
            &p,
            &cat,
            Action::Create(&input),
            Some("create"),
            None
        )
        .unwrap(),
        first
    );
    let completion = json!({"outcome":"exception","note":"Synthetic recipient left; current writer documents the exception"});
    assert!(
        call(
            &mut c,
            &cookies,
            &p,
            &cat,
            Action::Complete {
                id,
                input: &completion
            },
            Some("complete-after-revoke"),
            Some(1)
        )
        .is_ok()
    );
    let patch = json!({"assignee":null});
    assert!(
        call(
            &mut c,
            &cookies,
            &p,
            &cat,
            Action::Update { id, patch: &patch },
            Some("unassign"),
            Some(2)
        )
        .is_ok()
    );
    let view = call(
        &mut c,
        &cookies,
        &p,
        &cat,
        Action::List(&json!({})),
        None,
        None,
    )
    .unwrap();
    assert!(
        view["assignees"]
            .as_array()
            .unwrap()
            .iter()
            .all(|a| a["userId"] != "recipient")
    );
}
#[test]
fn scoped_service_has_no_human_mfa_requirement_and_charges_each_admitted_attempt_once() {
    use rentemester_runtime::service_key::ServiceKey;
    let (_d, mut c, cookies, p, cat) = fixture();
    c.execute_batch("INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES('human-one','activate','workspace_owner','user:human-one');").unwrap();
    let receipt = crate::service_lifecycle::execute(
        &mut c,
        &cookies,
        &Fixed,
        &p,
        &crate::service_lifecycle::Operation::Create {
            display_name: "Synthetic task agent".into(),
            operation_id: None,
            expires_in_seconds: None,
        },
    )
    .unwrap();
    let issued: Value = serde_json::from_str(receipt.as_json()).unwrap();
    let subject = issued["serviceAccountId"].as_str().unwrap();
    let credential = issued["credentialId"].as_str().unwrap();
    let key = ServiceKey::new("w", issued["secret"].as_str().unwrap()).unwrap();
    c.execute("INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES(?1,'activate','member','user:human-one')",[subject]).unwrap();
    c.execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES(?1,'a','grant','bookkeeper','user:human-one')",[subject]).unwrap();
    c.execute("UPDATE apikey SET remaining=10 WHERE id=?1", [credential])
        .unwrap();
    let input = draft();
    let first = execute(
        &mut c,
        &cat,
        &Fixed,
        Authority::Service(&key),
        Action::Create(&input),
        Some("create"),
        None,
    )
    .unwrap();
    assert_eq!(
        execute(
            &mut c,
            &cat,
            &Fixed,
            Authority::Service(&key),
            Action::Create(&input),
            Some("create"),
            None
        )
        .unwrap(),
        first
    );
    let id = first["task"]["taskId"].as_str().unwrap();
    assert_eq!(
        execute(
            &mut c,
            &cat,
            &Fixed,
            Authority::Service(&key),
            Action::Update {
                id,
                patch: &json!({"title":"Changed"})
            },
            Some("stale"),
            Some(99)
        )
        .unwrap_err()
        .error
        .code,
        "version_conflict"
    );
    assert_eq!(
        c.query_row(
            "SELECT remaining FROM apikey WHERE id=?1",
            [credential],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        7
    );
    c.execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES(?1,'a','revoke',NULL,'user:human-one')",[subject]).unwrap();
    assert!(
        execute(
            &mut c,
            &cat,
            &Fixed,
            Authority::Service(&key),
            Action::Create(&input),
            Some("create"),
            None
        )
        .is_err()
    );
    assert_eq!(
        c.query_row(
            "SELECT remaining FROM apikey WHERE id=?1",
            [credential],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        7
    );
    assert_eq!(
        c.query_row("SELECT count(*) FROM rm_task_events", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
}
