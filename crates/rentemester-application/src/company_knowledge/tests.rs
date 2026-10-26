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
                purpose: Default::default(), archived: false,
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

#[derive(Default)]
struct RejectingScopes {
    calls: Vec<Vec<String>>,
}
impl crate::backup_status::CompanyScopes for RejectingScopes {
    fn require(
        &mut self,
        _control: &rusqlite::Transaction<'_>,
        _clock: &dyn Clock,
        slugs: &[String],
    ) -> Result<(), rentemester_engine::Error> {
        self.calls.push(slugs.to_vec());
        Err(error("backup_locked"))
    }
}
#[test]
fn backup_scope_follows_authority_and_source_visibility_before_any_knowledge_effect() {
    let (_d, mut c, cookies, p, cat) = fixture();
    let input = proposal(json!("Synthetic guarded context"), "2026-01-01");
    let mut backup = RejectingScopes::default();
    let before = c.total_changes();
    let failure = execute_scoped(
        &mut c,
        &cookies,
        &Fixed,
        &p,
        &cat,
        "a",
        Action::Propose(&input),
        &mut backup,
    )
    .unwrap_err();
    assert_eq!(failure.error.code, "backup_locked");
    assert_eq!(backup.calls, vec![vec!["a".to_owned()]]);
    assert_eq!(c.total_changes(), before);
    let created = execute(
        &mut c,
        &cookies,
        &Fixed,
        &p,
        &cat,
        "a",
        Action::Propose(&input),
    )
    .unwrap();
    let id = created["assertion"]["assertionId"]
        .as_str()
        .unwrap()
        .to_owned();
    let before = c.total_changes();
    let review = Review {
        assertion_id: id.clone(),
        decision: "approved".into(),
        reason: None,
    };
    assert_eq!(
        execute_scoped(
            &mut c,
            &cookies,
            &Fixed,
            &p,
            &cat,
            "a",
            Action::Review(&review),
            &mut backup
        )
        .unwrap_err()
        .error
        .code,
        "backup_locked"
    );
    assert_eq!(c.total_changes(), before);
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
    let before = c.total_changes();
    let replacement = Supersede {
        assertion_id: id,
        replacement: proposal(json!("Synthetic replacement"), "2026-01-01"),
    };
    assert_eq!(
        execute_scoped(
            &mut c,
            &cookies,
            &Fixed,
            &p,
            &cat,
            "a",
            Action::Supersede(&replacement),
            &mut backup
        )
        .unwrap_err()
        .error
        .code,
        "backup_locked"
    );
    assert_eq!(c.total_changes(), before);
    let calls = backup.calls.len();
    execute_scoped(
        &mut c,
        &cookies,
        &Fixed,
        &p,
        &cat,
        "a",
        Action::Read {
            as_of: None,
            include_proposed: true,
        },
        &mut backup,
    )
    .unwrap();
    assert_eq!(backup.calls.len(), calls);
    let foreign = execute(
        &mut c,
        &cookies,
        &Fixed,
        &p,
        &cat,
        "b",
        Action::Propose(&input),
    )
    .unwrap();
    let foreign_id = foreign["assertion"]["assertionId"]
        .as_str()
        .unwrap()
        .to_owned();
    let rejected = Review {
        assertion_id: foreign_id.clone(),
        decision: "rejected".into(),
        reason: None,
    };
    assert_eq!(
        execute_scoped(
            &mut c,
            &cookies,
            &Fixed,
            &p,
            &cat,
            "a",
            Action::Review(&rejected),
            &mut backup
        )
        .unwrap_err()
        .error
        .code,
        "knowledge_not_found"
    );
    let foreign_replacement = Supersede {
        assertion_id: foreign_id,
        replacement: proposal(json!("Wrong company replacement"), "2026-01-01"),
    };
    assert_eq!(
        execute_scoped(
            &mut c,
            &cookies,
            &Fixed,
            &p,
            &cat,
            "a",
            Action::Supersede(&foreign_replacement),
            &mut backup
        )
        .unwrap_err()
        .error
        .code,
        "knowledge_not_found"
    );
    assert_eq!(backup.calls.len(), calls);
    let mut hidden = proposal(json!("Hidden source"), "2026-01-01");
    hidden.source.kind = "corporate_record".into();
    hidden.source.r#ref = "missing-record".into();
    assert_eq!(
        execute_scoped(
            &mut c,
            &cookies,
            &Fixed,
            &p,
            &cat,
            "a",
            Action::Propose(&hidden),
            &mut backup
        )
        .unwrap_err()
        .error
        .code,
        "knowledge_source_unavailable"
    );
    assert_eq!(backup.calls.len(), calls);
    c.execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor) VALUES('u','a','grant','reader','user:u')").unwrap();
    assert_eq!(
        execute_scoped(
            &mut c,
            &cookies,
            &Fixed,
            &p,
            &cat,
            "a",
            Action::Propose(&input),
            &mut backup
        )
        .unwrap_err()
        .error
        .code,
        "access_denied"
    );
    assert_eq!(backup.calls.len(), calls);
}

fn service_fixture() -> (
    tempfile::TempDir,
    Connection,
    Catalog,
    rentemester_runtime::service_key::ServiceKey,
    String,
    String,
) {
    let (dir, mut c, cookies, proof, cat) = fixture();
    c.execute("INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor) VALUES('u','activate','workspace_owner','user:u')", []).unwrap();
    let receipt = crate::service_lifecycle::execute(
        &mut c,
        &cookies,
        &Fixed,
        &proof,
        &crate::service_lifecycle::Operation::Create {
            display_name: "Synthetic knowledge service".into(),
            operation_id: None,
            expires_in_seconds: None,
        },
    )
    .unwrap();
    let issued: Value = serde_json::from_str(receipt.as_json()).unwrap();
    let subject = issued["serviceAccountId"].as_str().unwrap().to_owned();
    let credential = issued["credentialId"].as_str().unwrap().to_owned();
    let key =
        rentemester_runtime::service_key::ServiceKey::new("w", issued["secret"].as_str().unwrap())
            .unwrap();
    c.execute("INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor) VALUES(?,'activate','member','user:u')", [&subject]).unwrap();
    c.execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor) VALUES(?,'a','grant','bookkeeper','user:u')", [&subject]).unwrap();
    c.execute("UPDATE apikey SET remaining=100 WHERE id=?", [&credential])
        .unwrap();
    (dir, c, cat, key, subject, credential)
}
fn service(
    c: &mut Connection,
    cat: &Catalog,
    key: &rentemester_runtime::service_key::ServiceKey,
    slug: &str,
    action: Action<'_>,
) -> Result<Value, Failure> {
    execute_service_scoped(
        c,
        &Fixed,
        key,
        cat,
        slug,
        action,
        &mut crate::backup_status::UnlockedTestScopes,
    )
}
#[test]
fn service_canonical_append_only_attribution_dedup_review_supersession_and_journals() {
    let (dir, mut c, cat, key, subject, credential) = service_fixture();
    let ledger = Connection::open(dir.path().join("ledger")).unwrap();
    let before: i64 = ledger
        .query_row("SELECT count(*) FROM journal_entries", [], |r| r.get(0))
        .unwrap();
    let p = proposal(json!("Synthetic service scope"), "2026-01-01");
    let a = service(&mut c, &cat, &key, "a", Action::Propose(&p)).unwrap()["assertion"].clone();
    assert_eq!(a["companySlug"], "a");
    assert_eq!(
        a["principal"],
        json!({"kind":"service_account","id":subject})
    );
    assert_eq!(a["actor"], format!("service_account:{subject}"));
    assert_eq!(a["history"][0]["principal_kind"], "service_account");
    let repeated = service(&mut c, &cat, &key, "a", Action::Propose(&p)).unwrap();
    assert_eq!(repeated["assertion"], a);
    let review = Review {
        assertion_id: a["assertionId"].as_str().unwrap().into(),
        decision: "approved".into(),
        reason: Some("Synthetic independent policy review".into()),
    };
    let approved = service(&mut c, &cat, &key, "a", Action::Review(&review)).unwrap();
    assert_eq!(approved["assertion"]["history"][0], a["history"][0]);
    assert_eq!(approved["assertion"]["history"][1]["principal_id"], subject);
    let supersede = Supersede {
        assertion_id: review.assertion_id,
        replacement: proposal(json!("Synthetic replacement"), "2026-01-01"),
    };
    let changed = service(&mut c, &cat, &key, "a", Action::Supersede(&supersede)).unwrap();
    assert_eq!(changed["superseded"]["reviewState"], "superseded");
    assert_eq!(changed["superseded"]["history"][0], a["history"][0]);
    assert_eq!(changed["replacement"]["principal"], a["principal"]);
    let remaining: i64 = c
        .query_row(
            "SELECT remaining FROM apikey WHERE id=?",
            [&credential],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        remaining, 96,
        "each operation consumes exactly one request, including dedup"
    );
    assert_eq!(
        ledger
            .query_row("SELECT count(*) FROM journal_entries", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        before
    );
}
#[test]
fn service_company_segregation_current_revocation_and_forged_principal_refusal() {
    let (_dir, mut c, cat, key, subject, credential) = service_fixture();
    let p = proposal(json!("Synthetic A only"), "2026-01-01");
    service(&mut c, &cat, &key, "a", Action::Propose(&p)).unwrap();
    let before = c.total_changes();
    assert_eq!(
        service(
            &mut c,
            &cat,
            &key,
            "b",
            Action::Read {
                as_of: None,
                include_proposed: true
            }
        )
        .unwrap_err()
        .error
        .code,
        "access_denied"
    );
    assert_eq!(c.total_changes(), before);
    for field in ["actor", "principal", "companySlug"] {
        let mut forged = serde_json::to_value(json!({"predicate":"business_description","value":"forged","source":{"kind":"user","ref":"synthetic"},"validFrom":"2026-01-01"})).unwrap();
        forged[field] = json!("user:u");
        assert!(serde_json::from_value::<Proposal>(forged).is_err());
    }
    c.execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor) VALUES(?,'a','revoke',NULL,'user:u')", [&subject]).unwrap();
    assert_eq!(
        service(&mut c, &cat, &key, "a", Action::Propose(&p))
            .unwrap_err()
            .error
            .code,
        "access_denied"
    );
    c.execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor) VALUES(?,'a','grant','reader','user:u')", [&subject]).unwrap();
    assert_eq!(
        service(&mut c, &cat, &key, "a", Action::Propose(&p))
            .unwrap_err()
            .error
            .code,
        "access_denied"
    );
    assert_eq!(
        service(
            &mut c,
            &cat,
            &key,
            "a",
            Action::Read {
                as_of: None,
                include_proposed: true
            }
        )
        .unwrap()["history"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    c.execute("UPDATE apikey SET enabled=0 WHERE id=?", [&credential])
        .unwrap();
    assert_eq!(
        service(
            &mut c,
            &cat,
            &key,
            "a",
            Action::Read {
                as_of: None,
                include_proposed: true
            }
        )
        .unwrap_err()
        .error
        .code,
        "invalid_credentials"
    );
    assert_eq!(
        c.query_row(
            "SELECT remaining FROM apikey WHERE id=?",
            [&credential],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        98
    );
}
#[test]
fn historical_path_rows_are_diagnosed_without_read_migration() {
    let (_dir, mut c, _, _, _) = fixture();
    let p = proposal(json!("Historical synthetic"), "2026-01-01");
    let tx = c.transaction().unwrap();
    // Explicitly seed historical state without weakening append-only triggers.
    tx.execute("INSERT INTO rm_company_knowledge_assertions(assertion_id,company_slug,predicate,value_json,value_hash,source_kind,source_ref,valid_from,certainty,actor,principal_kind,principal_id,created_at) VALUES('historical','/synthetic/companies/a','business_description','\"historical\"',?,'user','synthetic','2026-01-01T00:00:00.000Z','confirmed','user:u','user','u','2026-01-01T00:00:00.000Z')", ["a".repeat(64)]).unwrap();
    let before = tx.total_changes();
    let e = storage::read(&tx, "/synthetic/companies/a", "2026-01-01", true).unwrap_err();
    assert_eq!(e.code, "knowledge_legacy_company_identity");
    assert!(e.message.contains("reconciliation"));
    assert_eq!(tx.total_changes(), before);
    assert!(
        storage::propose(
            &tx,
            "/synthetic/companies/a",
            &p,
            "no-path",
            "u",
            "2026-01-01T00:00:00.000Z"
        )
        .is_err()
    );
    assert_eq!(tx.query_row("SELECT company_slug FROM rm_company_knowledge_assertions WHERE assertion_id='historical'", [], |r| r.get::<_, String>(0)).unwrap(), "/synthetic/companies/a");
}

#[test]
fn service_sources_require_selected_company_not_grantor_or_workspace_visibility() {
    let (_dir, mut c, cat, key, _subject, credential) = service_fixture();
    let tx = c.transaction().unwrap();
    let party: rentemester_storage::parties::Create = serde_json::from_value(json!({"partyId":"synthetic-party","kind":"organization","name":"Synthetic party","source":"synthetic","observedAt":"2026-01-01","reviewAssertion":"Synthetic proof","role":"supplier"})).unwrap();
    let id = rentemester_storage::parties::create(
        &tx,
        &party,
        "unused",
        "user:u",
        "2026-01-01T00:00:00.000Z",
    )
    .unwrap();
    let role = rentemester_storage::parties::Role {
        role: "supplier".into(),
        defaults: None,
    };
    rentemester_storage::parties::role(&tx, &id, "b", &role, "user:u", "2026-01-01T00:00:00.000Z")
        .unwrap();
    let record: rentemester_storage::corporate_records::Ingest = serde_json::from_value(json!({"recordId":"synthetic-record","type":"articles","bytesBase64":"unused","filename":"synthetic.txt","source":"synthetic","receivedAt":"2026-01-01","uploader":"user:u"})).unwrap();
    let links = [rentemester_storage::corporate_records::Link {
        r#type: "company".into(),
        id: "b".into(),
    }];
    rentemester_storage::corporate_records::ingest(
        &tx,
        &record,
        b"synthetic governance proof",
        "unused",
        &links,
        "user:u",
    )
    .unwrap();
    tx.commit().unwrap();
    for (kind, reference) in [
        ("party", "synthetic-party"),
        ("corporate_record", "synthetic-record"),
    ] {
        let mut p = proposal(json!(format!("Synthetic {kind}")), "2026-01-01");
        p.source.kind = kind.into();
        p.source.r#ref = reference.into();
        let mut backup = RejectingScopes::default();
        let f = execute_service_scoped(
            &mut c,
            &Fixed,
            &key,
            &cat,
            "a",
            Action::Propose(&p),
            &mut backup,
        )
        .unwrap_err();
        assert_eq!(f.error.code, "knowledge_source_unavailable");
        assert!(backup.calls.is_empty());
    }
    assert_eq!(
        c.query_row(
            "SELECT remaining FROM apikey WHERE id=?",
            [&credential],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        100
    );
    let tx = c.transaction().unwrap();
    rentemester_storage::parties::role(&tx, &id, "a", &role, "user:u", "2026-01-01T00:00:00.000Z")
        .unwrap();
    rentemester_storage::corporate_records::link(
        &tx,
        "synthetic-record",
        &rentemester_storage::corporate_records::Link {
            r#type: "company".into(),
            id: "a".into(),
        },
        "user:u",
        "2026-01-01T00:00:00.000Z",
    )
    .unwrap();
    let record_a: rentemester_storage::corporate_records::Ingest = serde_json::from_value(json!({"recordId":"synthetic-record-a","type":"articles","bytesBase64":"unused","filename":"synthetic-a.txt","source":"synthetic","receivedAt":"2026-01-01","uploader":"user:u"})).unwrap();
    rentemester_storage::corporate_records::ingest(
        &tx,
        &record_a,
        b"synthetic A-only proof",
        "unused",
        &[rentemester_storage::corporate_records::Link {
            r#type: "company".into(),
            id: "a".into(),
        }],
        "user:u",
    )
    .unwrap();
    tx.commit().unwrap();
    let mut shared = proposal(json!("Shared corporate evidence"), "2026-01-01");
    shared.source.kind = "corporate_record".into();
    shared.source.r#ref = "synthetic-record".into();
    assert_eq!(
        service(&mut c, &cat, &key, "a", Action::Propose(&shared))
            .unwrap_err()
            .error
            .code,
        "knowledge_source_unavailable"
    );
    for (kind, reference) in [
        ("party", "synthetic-party"),
        ("corporate_record", "synthetic-record-a"),
    ] {
        let mut p = proposal(json!(format!("Synthetic {kind}")), "2026-01-01");
        p.source.kind = kind.into();
        p.source.r#ref = reference.into();
        service(&mut c, &cat, &key, "a", Action::Propose(&p)).unwrap();
    }
}

#[test]
fn service_backup_and_expiry_fences_leave_history_and_usage_uncommitted() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    struct Expiring(AtomicUsize);
    impl Clock for Expiring {
        fn now_unix_ms(&self) -> Result<i64, AuthError> {
            Ok(1767225660000
                + if self.0.fetch_add(1, Ordering::SeqCst) == 0 {
                    0
                } else {
                    2
                })
        }
    }
    let (_dir, mut c, cat, key, _subject, credential) = service_fixture();
    let p = proposal(json!("Synthetic backup fence"), "2026-01-01");
    let mut backup = RejectingScopes::default();
    assert_eq!(
        execute_service_scoped(
            &mut c,
            &Fixed,
            &key,
            &cat,
            "a",
            Action::Propose(&p),
            &mut backup
        )
        .unwrap_err()
        .error
        .code,
        "backup_locked"
    );
    assert_eq!(backup.calls, vec![vec!["a".to_owned()]]);
    c.execute(
        "UPDATE apikey SET expiresAt='2026-01-01T00:01:00.001Z' WHERE id=?",
        [&credential],
    )
    .unwrap();
    assert_eq!(
        execute_service_scoped(
            &mut c,
            &Expiring(AtomicUsize::new(0)),
            &key,
            &cat,
            "a",
            Action::Propose(&p),
            &mut crate::backup_status::UnlockedTestScopes
        )
        .unwrap_err()
        .error
        .code,
        "invalid_credentials"
    );
    assert_eq!(
        c.query_row(
            "SELECT remaining FROM apikey WHERE id=?",
            [&credential],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        100
    );
    for table in [
        "rm_company_knowledge_assertions",
        "rm_company_knowledge_events",
    ] {
        assert_eq!(
            c.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
}

#[test]
fn canonical_owner_read_reports_legacy_scope_without_exposing_it_to_member_or_service() {
    fn seed(c: &Connection) {
        c.execute("INSERT INTO rm_company_knowledge_assertions(assertion_id,company_slug,predicate,value_json,value_hash,source_kind,source_ref,valid_from,certainty,actor,principal_kind,principal_id,created_at) VALUES('historical','/synthetic/companies/a','business_description','\"historical\"',?,'user','synthetic','2026-01-01T00:00:00.000Z','confirmed','user:u','user','u','2026-01-01T00:00:00.000Z')", ["a".repeat(64)]).unwrap();
    }
    fn stored(c: &Connection) -> (String, i64) {
        (c.query_row("SELECT company_slug || value_json || value_hash || source_ref || actor || principal_kind || principal_id FROM rm_company_knowledge_assertions WHERE assertion_id='historical'", [], |r| r.get(0)).unwrap(),
         c.query_row("SELECT COUNT(*) FROM rm_company_knowledge_events", [], |r| r.get(0)).unwrap())
    }
    let (_d, mut c, cookies, proof, cat) = fixture();
    seed(&c);
    let before = stored(&c);
    let member = execute(
        &mut c,
        &cookies,
        &Fixed,
        &proof,
        &cat,
        "a",
        Action::Read {
            as_of: None,
            include_proposed: true,
        },
    )
    .unwrap();
    assert!(member.get("scopeDiagnostics").is_none());
    c.execute("INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor) VALUES('u','activate','workspace_owner','system:synthetic-test')", []).unwrap();
    let changes = c.total_changes();
    let owner = execute(
        &mut c,
        &cookies,
        &Fixed,
        &proof,
        &cat,
        "a",
        Action::Read {
            as_of: None,
            include_proposed: true,
        },
    )
    .unwrap();
    assert_eq!(owner["scopeDiagnostics"][0]["assertionCount"], 1);
    assert_eq!(
        owner["scopeDiagnostics"][0]["code"],
        "knowledge_legacy_company_identity"
    );
    assert_eq!(owner["scopeDiagnostics"][0]["automaticRepair"], false);
    assert!(!owner.to_string().contains("/synthetic/companies/a"));
    assert!(owner["history"].as_array().unwrap().is_empty());
    assert_eq!(c.total_changes(), changes);
    assert_eq!(stored(&c), before);
    let (_d, mut c, cat, key, _, _) = service_fixture();
    seed(&c);
    let before = stored(&c);
    let value = execute_service_scoped(
        &mut c,
        &Fixed,
        &key,
        &cat,
        "a",
        Action::Read {
            as_of: None,
            include_proposed: true,
        },
        &mut crate::backup_status::UnlockedTestScopes,
    )
    .unwrap();
    assert!(value.get("scopeDiagnostics").is_none());
    assert_eq!(stored(&c), before);
}
