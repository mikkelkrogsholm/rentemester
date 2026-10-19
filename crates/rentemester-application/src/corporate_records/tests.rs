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
    rentemester_storage::fresh::initialize_empty_pair(&mut c, &mut l, "Synthetic Corporate")
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
fn ingest(id: &str) -> Ingest {
    serde_json::from_value(json!({"recordId":id,"type":"articles","bytesBase64":"unused","filename":"synthetic.txt","source":"synthetic","receivedAt":"2026-01-01","uploader":"synthetic"})).unwrap()
}
#[test]
fn normal_current_permissions_sensitive_scope_revocation_and_rollback() {
    let (_d, mut c, cookies, p, cat) = fixture();
    let i = ingest("record-normal");
    let call = |c: &mut Connection, action| execute(c, &cookies, &Fixed, &p, &cat, "a", action);
    let first = call(&mut c, Action::Ingest(&i, b"synthetic original")).unwrap();
    let id = first["record"]["recordId"].as_str().unwrap();
    let link = LinkInput {
        record_id: Some(id.into()),
        r#type: "company".into(),
        id: "b".into(),
    };
    call(&mut c, Action::Link(id, &link)).unwrap();
    let before: Vec<u8> = c
        .query_row(
            "SELECT original_bytes FROM rm_corporate_record_bytes",
            [],
            |r| r.get(0),
        )
        .unwrap();
    c.execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,actor) VALUES('u','b','revoke','user:u')").unwrap();
    assert!(call(&mut c, Action::Inspect(id)).is_err());
    assert!(call(&mut c, Action::Download(id)).is_err());
    // Creation normalizes IDs; the current full-scope fence must normalize identically.
    let alias = ingest("  record-normal  ");
    assert!(call(&mut c, Action::Ingest(&alias, b"synthetic original")).is_err());
    assert_eq!(
        call(
            &mut c,
            Action::List {
                cursor: 0,
                limit: 25
            }
        )
        .unwrap()["count"],
        0
    );
    let enrich = Enrich {
        record_id: None,
        assertion: "Must not mutate".into(),
    };
    assert!(call(&mut c, Action::Enrich(id, &enrich)).is_err());
    assert_eq!(
        c.query_row::<Vec<u8>, _, _>(
            "SELECT original_bytes FROM rm_corporate_record_bytes",
            [],
            |r| r.get(0)
        )
        .unwrap(),
        before
    );
    assert_eq!(
        c.query_row::<i64, _, _>(
            "SELECT count(*) FROM rm_corporate_record_events WHERE event_type='enriched'",
            [],
            |r| r.get(0)
        )
        .unwrap(),
        0
    );
}
#[test]
fn bad_permission_sensitive_read_and_current_session_revocation() {
    let (_d, mut c, cookies, p, cat) = fixture();
    let mut i = ingest("record-sensitive");
    i.sensitivity = Some("restricted".into());
    execute(
        &mut c,
        &cookies,
        &Fixed,
        &p,
        &cat,
        "a",
        Action::Ingest(&i, b"secret source"),
    )
    .unwrap();
    c.execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','grant','reader','user:u')").unwrap();
    assert_eq!(
        execute(
            &mut c,
            &cookies,
            &Fixed,
            &p,
            &cat,
            "a",
            Action::List {
                cursor: 0,
                limit: 25
            }
        )
        .unwrap()["count"],
        1
    );
    assert!(
        execute(
            &mut c,
            &cookies,
            &Fixed,
            &p,
            &cat,
            "a",
            Action::Download("record-sensitive")
        )
        .is_err()
    );
    assert!(
        execute(
            &mut c,
            &cookies,
            &Fixed,
            &p,
            &cat,
            "a",
            Action::Ingest(&ingest("other"), b"other")
        )
        .is_err()
    );
    c.execute_batch("DELETE FROM session WHERE id='s'").unwrap();
    assert!(
        execute(
            &mut c,
            &cookies,
            &Fixed,
            &p,
            &cat,
            "a",
            Action::List {
                cursor: 0,
                limit: 25
            }
        )
        .is_err()
    );
}
#[test]
fn unknown_scope_forged_path_replacement_missing_and_restart_originals() {
    let (d, mut c, cookies, p, cat) = fixture();
    let i = ingest("record-a");
    execute(
        &mut c,
        &cookies,
        &Fixed,
        &p,
        &cat,
        "a",
        Action::Ingest(&i, b"original A"),
    )
    .unwrap();
    let input = LinkInput {
        record_id: Some("forged".into()),
        r#type: "company".into(),
        id: "a".into(),
    };
    assert!(
        execute(
            &mut c,
            &cookies,
            &Fixed,
            &p,
            &cat,
            "a",
            Action::Link("record-a", &input)
        )
        .is_err()
    );
    let input = LinkInput {
        record_id: None,
        r#type: "company".into(),
        id: "missing".into(),
    };
    assert!(
        execute(
            &mut c,
            &cookies,
            &Fixed,
            &p,
            &cat,
            "a",
            Action::Link("record-a", &input)
        )
        .is_err()
    );
    let replace = Supersede {
        record_id: None,
        replacement_record_id: "missing".into(),
        reason: "Synthetic correction".into(),
    };
    assert!(
        execute(
            &mut c,
            &cookies,
            &Fixed,
            &p,
            &cat,
            "a",
            Action::Supersede("record-a", &replace)
        )
        .is_err()
    );
    let original = execute(
        &mut c,
        &cookies,
        &Fixed,
        &p,
        &cat,
        "a",
        Action::Download("record-a"),
    )
    .unwrap();
    c.close().unwrap();
    let mut c = Connection::open(d.path().join("control")).unwrap();
    assert_eq!(
        execute(
            &mut c,
            &cookies,
            &Fixed,
            &p,
            &cat,
            "a",
            Action::Download("record-a")
        )
        .unwrap(),
        original
    );
}

#[test]
fn corporate_knowledge_source_is_rechecked_inside_current_control_scope() {
    let (_d, mut c, cookies, p, cat) = fixture();
    let input = ingest("record-knowledge");
    execute(
        &mut c,
        &cookies,
        &Fixed,
        &p,
        &cat,
        "a",
        Action::Ingest(&input, b"knowledge source"),
    )
    .unwrap();
    let proposal:crate::company_knowledge::Proposal=serde_json::from_value(json!({"predicate":"business_description","value":"Synthetic governance context","source":{"kind":"corporate_record","ref":"record-knowledge"},"validFrom":"2026-01-01"})).unwrap();
    crate::company_knowledge::execute(
        &mut c,
        &cookies,
        &Fixed,
        &p,
        &cat,
        "a",
        crate::company_knowledge::Action::Propose(&proposal),
    )
    .unwrap();
    execute(
        &mut c,
        &cookies,
        &Fixed,
        &p,
        &cat,
        "a",
        Action::Link(
            "record-knowledge",
            &LinkInput {
                record_id: None,
                r#type: "company".into(),
                id: "b".into(),
            },
        ),
    )
    .unwrap();
    c.execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,actor)VALUES('u','b','revoke','user:u')").unwrap();
    assert!(
        crate::company_knowledge::execute(
            &mut c,
            &cookies,
            &Fixed,
            &p,
            &cat,
            "a",
            crate::company_knowledge::Action::Propose(&proposal)
        )
        .is_err()
    );
}
