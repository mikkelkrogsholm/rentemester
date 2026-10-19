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
    c.execute_batch("INSERT INTO user(id,name,email,emailVerified,twoFactorEnabled,createdAt,updatedAt) VALUES('u','Synthetic','u@example.invalid',1,1,'2026-01-01','2026-01-01');INSERT INTO twoFactor(id,secret,backupCodes,userId,verified) VALUES('f','synthetic','synthetic','u',1);INSERT INTO session(id,expiresAt,token,createdAt,updatedAt,userId) VALUES('s','2026-01-01T12:00:00.000Z','synthetic-token','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z','u');INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor) VALUES('u','activate','member','user:u');INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor) VALUES('u','a','grant','bookkeeper','user:u');INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor) VALUES('u','b','grant','bookkeeper','user:u');").unwrap();
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
fn proposal(value: Value, from: &str) -> Proposal {
    serde_json::from_value(json!({"predicate":"business_description","value":value,"source":{"kind":"user","ref":"synthetic-brief"},"validFrom":from})).unwrap()
}
#[test]
fn lifecycle_history_future_context_conflicts_and_restart() {
    let (d, mut c, cookies, p, cat) = fixture();
    let i = proposal(
        json!({"text":"Synthetic service","nested":{"z":1.0,"a":true}}),
        "2026-01-01",
    );
    let a =
        execute(&mut c, &cookies, &Fixed, &p, &cat, "a", Action::Propose(&i)).unwrap()["assertion"]
            .clone();
    let id = a["assertionId"].as_str().unwrap();
    assert_eq!(
        execute(&mut c, &cookies, &Fixed, &p, &cat, "a", Action::Propose(&i)).unwrap()["assertion"]
            ["assertionId"],
        id
    );
    let review = Review {
        assertion_id: id.into(),
        decision: "approved".into(),
        reason: Some("Synthetic review".into()),
    };
    execute(
        &mut c,
        &cookies,
        &Fixed,
        &p,
        &cat,
        "a",
        Action::Review(&review),
    )
    .unwrap();
    assert!(
        execute(
            &mut c,
            &cookies,
            &Fixed,
            &p,
            &cat,
            "b",
            Action::Review(&review)
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
            Action::Review(&review)
        )
        .unwrap()["assertion"]["history"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let replacement = Supersede {
        assertion_id: id.into(),
        replacement: proposal(json!("Future service"), "2027-01-01"),
    };
    let changed = execute(
        &mut c,
        &cookies,
        &Fixed,
        &p,
        &cat,
        "a",
        Action::Supersede(&replacement),
    )
    .unwrap();
    assert_eq!(changed["replacement"]["reviewState"], "proposed");
    assert_eq!(changed["superseded"]["reviewState"], "superseded");
    assert!(
        execute(
            &mut c,
            &cookies,
            &Fixed,
            &p,
            &cat,
            "a",
            Action::Supersede(&replacement)
        )
        .is_err()
    );
    let r = Review {
        assertion_id: changed["replacement"]["assertionId"]
            .as_str()
            .unwrap()
            .into(),
        decision: "approved".into(),
        reason: None,
    };
    execute(&mut c, &cookies, &Fixed, &p, &cat, "a", Action::Review(&r)).unwrap();
    let read = |c: &mut Connection, at| {
        execute(
            c,
            &cookies,
            &Fixed,
            &p,
            &cat,
            "a",
            Action::Read {
                as_of: Some(at),
                include_proposed: true,
            },
        )
        .unwrap()
    };
    assert!(
        read(&mut c, "2026-01-01")["context"]["assertions"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        read(&mut c, "2027-01-01")["context"]["assertions"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let conflict = proposal(json!("Conflicting service"), "2027-01-01");
    execute(
        &mut c,
        &cookies,
        &Fixed,
        &p,
        &cat,
        "a",
        Action::Propose(&conflict),
    )
    .unwrap();
    let before = read(&mut c, "2027-01-01");
    assert_eq!(
        before["context"]["conflicts"],
        json!(["business_description"])
    );
    assert_eq!(before["context"]["safeForProductBehavior"], false);
    assert_eq!(
        before["history"][0]["principal"],
        json!({"kind":"user","id":"u"})
    );
    assert!(
        c.execute("DELETE FROM rm_company_knowledge_events", [])
            .is_err()
    );
    drop(c);
    let mut c = Connection::open(d.path().join("control")).unwrap();
    assert_eq!(read(&mut c, "2027-01-01"), before);
    let l = Connection::open(d.path().join("ledger")).unwrap();
    assert_eq!(
        l.query_row("SELECT count(*) FROM journal_entries", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
}
#[test]
fn current_role_revocation_archive_and_source_refusals_leave_control_unchanged() {
    let (_d, mut c, cookies, p, mut cat) = fixture();
    let i = proposal(json!("Synthetic"), "2026-01-01");
    c.execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor) VALUES('u','a','grant','reader','user:u')",[]).unwrap();
    assert!(execute(&mut c, &cookies, &Fixed, &p, &cat, "a", Action::Propose(&i)).is_err());
    assert!(
        execute(
            &mut c,
            &cookies,
            &Fixed,
            &p,
            &cat,
            "a",
            Action::Read {
                as_of: None,
                include_proposed: false
            }
        )
        .is_ok()
    );
    c.execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor) VALUES('u','a','grant','reviewer','user:u')",[]).unwrap();
    cat.companies[0].archived = true;
    assert!(execute(&mut c, &cookies, &Fixed, &p, &cat, "a", Action::Propose(&i)).is_err());
    cat.companies[0].archived = false;
    let mut bad = i.clone();
    bad.source.kind = "party".into();
    assert!(
        execute(
            &mut c,
            &cookies,
            &Fixed,
            &p,
            &cat,
            "a",
            Action::Propose(&bad)
        )
        .is_err()
    );
    bad = i.clone();
    bad.valid_to_exclusive = Some("2025-01-01".into());
    assert!(
        execute(
            &mut c,
            &cookies,
            &Fixed,
            &p,
            &cat,
            "a",
            Action::Propose(&bad)
        )
        .is_err()
    );
    bad = i.clone();
    bad.predicate = "tax_instruction".into();
    assert!(
        execute(
            &mut c,
            &cookies,
            &Fixed,
            &p,
            &cat,
            "a",
            Action::Propose(&bad)
        )
        .is_err()
    );
    c.execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,actor) VALUES('u','a','revoke','user:u')",[]).unwrap();
    assert!(
        execute(
            &mut c,
            &cookies,
            &Fixed,
            &p,
            &cat,
            "a",
            Action::Read {
                as_of: None,
                include_proposed: false
            }
        )
        .is_err()
    );
    assert_eq!(
        c.query_row(
            "SELECT count(*) FROM rm_company_knowledge_assertions",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
}
#[test]
fn rejected_state_and_bounded_values_are_exact() {
    let (_d, mut c, cookies, p, cat) = fixture();
    let i = proposal(json!(["DK"]), "2026-01-01");
    let a = execute(&mut c, &cookies, &Fixed, &p, &cat, "a", Action::Propose(&i)).unwrap();
    let mut r = Review {
        assertion_id: a["assertion"]["assertionId"].as_str().unwrap().into(),
        decision: "rejected".into(),
        reason: None,
    };
    execute(&mut c, &cookies, &Fixed, &p, &cat, "a", Action::Review(&r)).unwrap();
    r.decision = "approved".into();
    assert!(execute(&mut c, &cookies, &Fixed, &p, &cat, "a", Action::Review(&r)).is_err());
    let bad = proposal(json!("x".repeat(32768)), "2026-01-01");
    assert!(
        execute(
            &mut c,
            &cookies,
            &Fixed,
            &p,
            &cat,
            "a",
            Action::Propose(&bad)
        )
        .is_err()
    );
}

#[test]
fn self_supersession_is_refused_without_changing_approved_context_or_events() {
    let (_d, mut c, cookies, p, cat) = fixture();
    let i = proposal(json!("Synthetic same content"), "2026-01-01");
    let a = execute(&mut c, &cookies, &Fixed, &p, &cat, "a", Action::Propose(&i)).unwrap();
    let id = a["assertion"]["assertionId"].as_str().unwrap();
    execute(
        &mut c,
        &cookies,
        &Fixed,
        &p,
        &cat,
        "a",
        Action::Review(&Review {
            assertion_id: id.into(),
            decision: "approved".into(),
            reason: None,
        }),
    )
    .unwrap();
    let before = execute(
        &mut c,
        &cookies,
        &Fixed,
        &p,
        &cat,
        "a",
        Action::Read {
            as_of: Some("2026-01-01"),
            include_proposed: true,
        },
    )
    .unwrap();
    let changes = c.total_changes();
    assert!(
        execute(
            &mut c,
            &cookies,
            &Fixed,
            &p,
            &cat,
            "a",
            Action::Supersede(&Supersede {
                assertion_id: id.into(),
                replacement: i
            })
        )
        .is_err()
    );
    assert_eq!(c.total_changes(), changes);
    assert_eq!(
        execute(
            &mut c,
            &cookies,
            &Fixed,
            &p,
            &cat,
            "a",
            Action::Read {
                as_of: Some("2026-01-01"),
                include_proposed: true
            }
        )
        .unwrap(),
        before
    );
}
