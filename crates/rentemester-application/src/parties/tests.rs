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

fn create_input(id: &str, name: &str) -> Create {
    serde_json::from_value(json!({"partyId":id,"kind":"organization","name":name,"aliases":["Synthetic Trading"],"identifiers":[],"source":"synthetic-brief","observedAt":"2026-01-01","reviewAssertion":"Synthetic reviewed source","role":"supplier","defaults":{"currency":"DKK"}})).unwrap()
}
fn amendment() -> Assertion {
    Assertion {
        field: "name".into(),
        value: "Synthetic reviewed name".into(),
        source: "synthetic-registry-snapshot".into(),
        observed_at: "2026-02-01".into(),
        review_state: "approved".into(),
    }
}
#[test]
fn creates_associates_appends_provenance_and_restarts_without_accounting() {
    let (d, mut c, cookies, p, cat) = fixture();
    let i = create_input("party-synthetic", "Synthetic initial name");
    let first = execute(&mut c, &cookies, &Fixed, &p, &cat, "a", Action::Create(&i)).unwrap();
    assert_eq!(first["party"]["roles"][0]["companySlug"], "a");
    assert_eq!(first["party"]["aliases"][0]["alias"], "synthetic trading");
    assert_eq!(first["party"]["history"].as_array().unwrap().len(), 1);
    assert_eq!(
        execute(&mut c, &cookies, &Fixed, &p, &cat, "a", Action::Create(&i)).unwrap(),
        first
    );
    execute(
        &mut c,
        &cookies,
        &Fixed,
        &p,
        &cat,
        "b",
        Action::Role(
            "party-synthetic",
            &Role {
                role: "customer".into(),
                defaults: None,
            },
        ),
    )
    .unwrap();
    execute(
        &mut c,
        &cookies,
        &Fixed,
        &p,
        &cat,
        "a",
        Action::Assertion("party-synthetic", &amendment()),
    )
    .unwrap();
    execute(
        &mut c,
        &cookies,
        &Fixed,
        &p,
        &cat,
        "a",
        Action::Alias(
            "party-synthetic",
            &Alias {
                alias: "Synthetic Reviewed Alias".into(),
                source: "reviewed-brief".into(),
                observed_at: "2026-02-01".into(),
                review_state: "approved".into(),
            },
        ),
    )
    .unwrap();
    let read = execute(
        &mut c,
        &cookies,
        &Fixed,
        &p,
        &cat,
        "a",
        Action::Inspect("party-synthetic"),
    )
    .unwrap();
    assert_eq!(read["party"]["name"], "Synthetic initial name");
    assert_eq!(
        read["party"]["assertions"]
            .as_array()
            .unwrap()
            .last()
            .unwrap()["value"],
        "Synthetic reviewed name"
    );
    assert_eq!(read["party"]["roles"].as_array().unwrap().len(), 2);
    assert_eq!(read["party"]["history"].as_array().unwrap().len(), 1);
    let list = execute(
        &mut c,
        &cookies,
        &Fixed,
        &p,
        &cat,
        "a",
        Action::List {
            query: Some("reviewed alias"),
            cursor: 0,
            limit: 25,
        },
    )
    .unwrap();
    assert_eq!(list["count"], 1);
    assert!(
        c.execute("UPDATE rm_party_field_assertions SET value='rewritten'", [])
            .is_err()
    );
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
            Action::Inspect("party-synthetic")
        )
        .unwrap(),
        read
    );
    let ledger = Connection::open(d.path().join("ledger")).unwrap();
    assert_eq!(
        ledger
            .query_row("SELECT count(*) FROM journal_entries", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
}
#[test]
fn hidden_associations_block_global_updates_and_hidden_rows_never_affect_counts() {
    let (_d, mut c, cookies, p, cat) = fixture();
    execute(
        &mut c,
        &cookies,
        &Fixed,
        &p,
        &cat,
        "a",
        Action::Create(&create_input("party-shared", "Synthetic shared")),
    )
    .unwrap();
    execute(
        &mut c,
        &cookies,
        &Fixed,
        &p,
        &cat,
        "b",
        Action::Role(
            "party-shared",
            &Role {
                role: "customer".into(),
                defaults: Some(
                    serde_json::from_value(json!({"account":"SECRET-B-ACCOUNT"})).unwrap(),
                ),
            },
        ),
    )
    .unwrap();
    execute(
        &mut c,
        &cookies,
        &Fixed,
        &p,
        &cat,
        "b",
        Action::Create(&create_input("party-hidden", "SECRET-HIDDEN-NAME")),
    )
    .unwrap();
    c.execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,actor) VALUES('u','b','revoke','user:u')",[]).unwrap();
    let before = execute(
        &mut c,
        &cookies,
        &Fixed,
        &p,
        &cat,
        "a",
        Action::Inspect("party-shared"),
    )
    .unwrap();
    let wire = before.to_string();
    assert!(!wire.contains("SECRET-B"));
    assert_eq!(before["party"]["roles"].as_array().unwrap().len(), 1);
    let count = c.total_changes();
    assert!(
        execute(
            &mut c,
            &cookies,
            &Fixed,
            &p,
            &cat,
            "a",
            Action::Assertion("party-shared", &amendment())
        )
        .is_err()
    );
    assert_eq!(c.total_changes(), count);
    assert_eq!(
        execute(
            &mut c,
            &cookies,
            &Fixed,
            &p,
            &cat,
            "a",
            Action::Inspect("party-shared")
        )
        .unwrap(),
        before
    );
    let list = execute(
        &mut c,
        &cookies,
        &Fixed,
        &p,
        &cat,
        "a",
        Action::List {
            query: None,
            cursor: 0,
            limit: 1,
        },
    )
    .unwrap();
    assert_eq!(list["count"], 1);
    assert_eq!(list["nextCursor"], Value::Null);
    assert!(!list.to_string().contains("SECRET"));
    assert!(
        execute(
            &mut c,
            &cookies,
            &Fixed,
            &p,
            &cat,
            "a",
            Action::Inspect("party-hidden")
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
            Action::Create(&create_input("party-hidden", "SECRET-HIDDEN-NAME"))
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
            Action::Role(
                "party-hidden",
                &Role {
                    role: "supplier".into(),
                    defaults: None
                }
            )
        )
        .is_err()
    );
}
#[test]
fn current_permission_archive_defaults_identifier_and_source_scope_refusals() {
    let (_d, mut c, cookies, p, mut cat) = fixture();
    let mut i = create_input("party-typed", "Synthetic typed");
    i.identifiers = Some(vec![rentemester_engine::parties::Identifier {
        country: "DK".into(),
        identifier: Some("12345678".into()),
        identifier_kind: "dk_cvr".into(),
    }]);
    execute(&mut c, &cookies, &Fixed, &p, &cat, "a", Action::Create(&i)).unwrap();
    i.party_id = Some("party-conflict".into());
    assert!(execute(&mut c, &cookies, &Fixed, &p, &cat, "a", Action::Create(&i)).is_err());
    assert!(
        execute(
            &mut c,
            &cookies,
            &Fixed,
            &p,
            &cat,
            "a",
            Action::Role(
                "party-typed",
                &Role {
                    role: "supplier".into(),
                    defaults: Some(serde_json::from_value(json!({"currency":"EUR"})).unwrap())
                }
            )
        )
        .is_err()
    );
    execute(
        &mut c,
        &cookies,
        &Fixed,
        &p,
        &cat,
        "b",
        Action::Role(
            "party-typed",
            &Role {
                role: "vendor".into(),
                defaults: None,
            },
        ),
    )
    .unwrap();
    cat.companies[1].archived = true;
    assert!(
        execute(
            &mut c,
            &cookies,
            &Fixed,
            &p,
            &cat,
            "a",
            Action::Assertion("party-typed", &amendment())
        )
        .is_err()
    );
    cat.companies[1].archived = false;
    let knowledge = |id: &str| {
        serde_json::from_value::<crate::company_knowledge::Proposal>(json!({"predicate":"responsible_party","value":"Synthetic source","source":{"kind":"party","ref":id},"validFrom":"2026-01-01"})).unwrap()
    };
    crate::company_knowledge::execute(
        &mut c,
        &cookies,
        &Fixed,
        &p,
        &cat,
        "a",
        crate::company_knowledge::Action::Propose(&knowledge("party-typed")),
    )
    .unwrap();
    execute(
        &mut c,
        &cookies,
        &Fixed,
        &p,
        &cat,
        "b",
        Action::Create(&create_input(
            "party-secret-source",
            "Synthetic secret source",
        )),
    )
    .unwrap();
    c.execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,actor) VALUES('u','b','revoke','user:u')",[]).unwrap();
    assert!(
        crate::company_knowledge::execute(
            &mut c,
            &cookies,
            &Fixed,
            &p,
            &cat,
            "a",
            crate::company_knowledge::Action::Propose(&knowledge("party-secret-source"))
        )
        .is_err()
    );
    assert!(
        crate::company_knowledge::execute(
            &mut c,
            &cookies,
            &Fixed,
            &p,
            &cat,
            "a",
            crate::company_knowledge::Action::Propose(&knowledge("not-a-party"))
        )
        .is_err()
    );
    c.execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor) VALUES('u','a','grant','reader','user:u')",[]).unwrap();
    assert!(
        execute(
            &mut c,
            &cookies,
            &Fixed,
            &p,
            &cat,
            "a",
            Action::Create(&create_input("party-reader", "Synthetic reader"))
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
            Action::Inspect("party-typed")
        )
        .is_ok()
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
            Action::Inspect("party-typed")
        )
        .is_err()
    );
}
#[test]
fn revoked_source_scope_cannot_approve_pending_knowledge_and_unassociated_is_not_authority() {
    let (_d, mut c, cookies, p, cat) = fixture();
    execute(
        &mut c,
        &cookies,
        &Fixed,
        &p,
        &cat,
        "b",
        Action::Create(&create_input("party-source-b", "Synthetic source B")),
    )
    .unwrap();
    let input=serde_json::from_value::<crate::company_knowledge::Proposal>(json!({"predicate":"responsible_party","value":"Synthetic responsible source","source":{"kind":"party","ref":"party-source-b"},"validFrom":"2026-01-01"})).unwrap();
    let assertion = crate::company_knowledge::execute(
        &mut c,
        &cookies,
        &Fixed,
        &p,
        &cat,
        "a",
        crate::company_knowledge::Action::Propose(&input),
    )
    .unwrap();
    c.execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,actor) VALUES('u','b','revoke','user:u')",[]).unwrap();
    let mut review = crate::company_knowledge::Review {
        assertion_id: assertion["assertion"]["assertionId"]
            .as_str()
            .unwrap()
            .into(),
        decision: "approved".into(),
        reason: None,
    };
    assert!(
        crate::company_knowledge::execute(
            &mut c,
            &cookies,
            &Fixed,
            &p,
            &cat,
            "a",
            crate::company_knowledge::Action::Review(&review)
        )
        .is_err()
    );
    review.decision = "rejected".into();
    assert_eq!(
        crate::company_knowledge::execute(
            &mut c,
            &cookies,
            &Fixed,
            &p,
            &cat,
            "a",
            crate::company_knowledge::Action::Review(&review)
        )
        .unwrap()["assertion"]["reviewState"],
        "rejected"
    );
    let tx = c
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    storage::create(
        &tx,
        &create_input("party-unassociated", "Synthetic unassociated"),
        "unused",
        "user:u",
        "2026-01-01T00:00:00.000Z",
    )
    .unwrap();
    tx.commit().unwrap();
    assert!(
        execute(
            &mut c,
            &cookies,
            &Fixed,
            &p,
            &cat,
            "a",
            Action::Assertion("party-unassociated", &amendment())
        )
        .is_err()
    );
}

#[test]
fn backup_fence_covers_shared_party_mutations_after_authorization() {
    struct Reject {
        seen: Vec<String>,
    }
    impl crate::backup_status::CompanyScopes for Reject {
        fn require(
            &mut self,
            _: &rusqlite::Transaction<'_>,
            _: &dyn Clock,
            slugs: &[String],
        ) -> Result<(), rentemester_engine::Error> {
            self.seen = slugs.to_vec();
            Err(rentemester_engine::Error::new(
                "backup_overdue",
                "synthetic refusal",
            ))
        }
    }
    let (_d, mut c, cookies, p, cat) = fixture();
    let mut reject = Reject { seen: vec![] };
    let input = create_input("party-synthetic", "Synthetic shared party");
    let error = execute_scoped(
        &mut c,
        &cookies,
        &Fixed,
        &p,
        &cat,
        "a",
        Action::Create(&input),
        &mut reject,
    )
    .unwrap_err();
    assert_eq!(error.error.code, "backup_overdue");
    assert_eq!(reject.seen, ["a"]);
    assert_eq!(
        c.query_row("SELECT count(*) FROM rm_party_events", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    execute(
        &mut c,
        &cookies,
        &Fixed,
        &p,
        &cat,
        "a",
        Action::Create(&input),
    )
    .unwrap();
    execute(
        &mut c,
        &cookies,
        &Fixed,
        &p,
        &cat,
        "b",
        Action::Role(
            "party-synthetic",
            &Role {
                role: "customer".into(),
                defaults: None,
            },
        ),
    )
    .unwrap();
    let before = execute(
        &mut c,
        &cookies,
        &Fixed,
        &p,
        &cat,
        "a",
        Action::Inspect("party-synthetic"),
    )
    .unwrap();
    let error = execute_scoped(
        &mut c,
        &cookies,
        &Fixed,
        &p,
        &cat,
        "b",
        Action::Assertion("party-synthetic", &amendment()),
        &mut reject,
    )
    .unwrap_err();
    assert_eq!(error.error.code, "backup_overdue");
    assert_eq!(reject.seen, ["a", "b"]);
    assert_eq!(
        execute(
            &mut c,
            &cookies,
            &Fixed,
            &p,
            &cat,
            "a",
            Action::Inspect("party-synthetic")
        )
        .unwrap(),
        before
    );
    reject.seen.clear();
    execute_scoped(
        &mut c,
        &cookies,
        &Fixed,
        &p,
        &cat,
        "a",
        Action::Inspect("party-synthetic"),
        &mut reject,
    )
    .unwrap();
    assert!(reject.seen.is_empty());
    c.execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,actor) VALUES('u','a','revoke','user:u')",[]).unwrap();
    let error = execute_scoped(
        &mut c,
        &cookies,
        &Fixed,
        &p,
        &cat,
        "b",
        Action::Assertion("party-synthetic", &amendment()),
        &mut reject,
    )
    .unwrap_err();
    assert_eq!(error.error.code, "access_denied");
    assert!(reject.seen.is_empty());
}

fn merge_input(target: &str) -> ProposeMerge {
    ProposeMerge {
        into_party_id: target.into(),
        review_assertion: "Synthetic explicit review\nBoth records checked".into(),
    }
}
fn merge_pair(c: &mut Connection, cookies: &CookieVerifier, p: &VerifiedCookie, cat: &Catalog) {
    for (id, slug) in [("from", "a"), ("into", "b")] {
        let mut i = create_input(id, &format!("Synthetic {id}"));
        i.identifiers = Some(vec![rentemester_engine::parties::Identifier {
            country: "DK".into(),
            identifier: Some(if id == "from" { "12345678" } else { "87654321" }.into()),
            identifier_kind: "dk_cvr".into(),
        }]);
        execute(c, cookies, &Fixed, p, cat, slug, Action::Create(&i)).unwrap();
    }
}
fn merge_hash(
    c: &mut Connection,
    cookies: &CookieVerifier,
    p: &VerifiedCookie,
    cat: &Catalog,
) -> ApproveMerge {
    let value = execute(
        c,
        cookies,
        &Fixed,
        p,
        cat,
        "a",
        Action::ProposeMerge("from", &merge_input("into")),
    )
    .unwrap();
    ApproveMerge {
        proposal_hash: value["proposalHash"].as_str().unwrap().into(),
    }
}
fn merge_event_count(c: &Connection) -> i64 {
    c.query_row("SELECT count(*) FROM rm_party_events WHERE event_type IN ('proposed_merge','approved_merge','superseded')",[],|r|r.get(0)).unwrap()
}
#[test]
fn party_merge_lifecycle_retains_exact_payload_actor_history_and_identity() {
    let (d, mut c, cookies, p, cat) = fixture();
    merge_pair(&mut c, &cookies, &p, &cat);
    let before = execute(
        &mut c,
        &cookies,
        &Fixed,
        &p,
        &cat,
        "a",
        Action::Inspect("from"),
    )
    .unwrap();
    let target = execute(
        &mut c,
        &cookies,
        &Fixed,
        &p,
        &cat,
        "a",
        Action::Inspect("into"),
    )
    .unwrap();
    let h = merge_hash(&mut c, &cookies, &p, &cat);
    assert_eq!(h.proposal_hash.len(), 64);
    assert_eq!(
        merge_hash(&mut c, &cookies, &p, &cat).proposal_hash,
        h.proposal_hash
    );
    assert_eq!(merge_event_count(&c), 1);
    let approved = execute(
        &mut c,
        &cookies,
        &Fixed,
        &p,
        &cat,
        "a",
        Action::ApproveMerge("from", &h),
    )
    .unwrap();
    for key in [
        "partyId",
        "name",
        "identifiers",
        "roles",
        "aliases",
        "assertions",
    ] {
        assert_eq!(before["party"][key], approved["party"][key]);
    }
    let history = approved["party"]["history"].as_array().unwrap();
    assert_eq!(history.len(), 4);
    for (entry, kind) in history[1..]
        .iter()
        .zip(["proposed_merge", "approved_merge", "superseded"])
    {
        assert_eq!(entry["event_type"], kind);
        assert_eq!(entry["payload_hash"], h.proposal_hash);
        assert_eq!(entry["actor"], "user:u");
        assert_eq!(entry["created_at"], "2026-01-01T00:01:00.000Z");
    }
    let payloads: Vec<String> = c.prepare("SELECT canonical_payload FROM rm_party_events WHERE party_id='from' AND event_type!='created' AND event_type!='linked' ORDER BY id").unwrap().query_map([],|r|r.get(0)).unwrap().map(Result::unwrap).collect();
    assert_eq!(payloads.len(), 3);
    assert!(payloads.iter().all(|v| v == &payloads[0]));
    assert_eq!(
        payloads[0],
        "{\"intoPartyId\":\"into\",\"reviewAssertion\":\"Synthetic explicit review\\nBoth records checked\"}"
    );
    assert_eq!(
        execute(
            &mut c,
            &cookies,
            &Fixed,
            &p,
            &cat,
            "a",
            Action::Inspect("into")
        )
        .unwrap(),
        target
    );
    assert_eq!(
        execute(
            &mut c,
            &cookies,
            &Fixed,
            &p,
            &cat,
            "a",
            Action::ApproveMerge("from", &h)
        )
        .unwrap(),
        approved
    );
    assert_eq!(merge_event_count(&c), 3);
    assert_eq!(
        c.query_row("SELECT count(*) FROM rm_party_identifiers", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        2
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
fn party_merge_refuses_self_missing_hidden_wrong_hash_source_and_unknown_authority() {
    let (_d, mut c, cookies, p, cat) = fixture();
    merge_pair(&mut c, &cookies, &p, &cat);
    for target in ["from", "missing"] {
        assert!(
            execute(
                &mut c,
                &cookies,
                &Fixed,
                &p,
                &cat,
                "a",
                Action::ProposeMerge("from", &merge_input(target))
            )
            .is_err()
        );
    }
    assert_eq!(merge_event_count(&c), 0);
    let h = merge_hash(&mut c, &cookies, &p, &cat);
    for (id, hash) in [
        ("into", h.proposal_hash.clone()),
        ("from", "a".repeat(64)),
        ("from", "not-a-hash".into()),
    ] {
        assert!(
            execute(
                &mut c,
                &cookies,
                &Fixed,
                &p,
                &cat,
                "a",
                Action::ApproveMerge(
                    id,
                    &ApproveMerge {
                        proposal_hash: hash
                    }
                )
            )
            .is_err()
        );
    }
    c.execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,actor) VALUES('u','b','revoke','user:u')",[]).unwrap();
    assert!(
        execute(
            &mut c,
            &cookies,
            &Fixed,
            &p,
            &cat,
            "a",
            Action::ProposeMerge("from", &merge_input("into"))
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
            Action::ApproveMerge("from", &h)
        )
        .is_err()
    );
    c.execute("DELETE FROM session", []).unwrap();
    assert!(
        execute(
            &mut c,
            &cookies,
            &Fixed,
            &p,
            &cat,
            "a",
            Action::ApproveMerge("from", &h)
        )
        .is_err()
    );
    assert_eq!(merge_event_count(&c), 1);
}
#[test]
fn party_merge_both_full_scopes_and_live_revocations_are_required() {
    for shared in ["from", "into"] {
        let (_d, mut c, cookies, p, mut cat) = fixture();
        merge_pair(&mut c, &cookies, &p, &cat);
        let h = merge_hash(&mut c, &cookies, &p, &cat);
        // Give the selected party both scopes; keep its other scope readable but not writable.
        let slug = if shared == "from" { "b" } else { "a" };
        execute(
            &mut c,
            &cookies,
            &Fixed,
            &p,
            &cat,
            slug,
            Action::Role(
                shared,
                &Role {
                    role: "customer".into(),
                    defaults: None,
                },
            ),
        )
        .unwrap();
        c.execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor) VALUES('u','b','grant','reader','user:u')",[]).unwrap();
        assert!(
            execute(
                &mut c,
                &cookies,
                &Fixed,
                &p,
                &cat,
                "a",
                Action::ProposeMerge("from", &merge_input("into"))
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
                Action::ApproveMerge("from", &h)
            )
            .is_err()
        );
        cat.companies[1].archived = true;
        assert!(
            execute(
                &mut c,
                &cookies,
                &Fixed,
                &p,
                &cat,
                "a",
                Action::ApproveMerge("from", &h)
            )
            .is_err()
        );
        assert_eq!(merge_event_count(&c), 1);
    }
}
#[test]
fn party_merge_backup_union_includes_anchor_for_both_operations() {
    struct Guard {
        seen: Vec<String>,
    }
    impl crate::backup_status::CompanyScopes for Guard {
        fn require(
            &mut self,
            _: &rusqlite::Transaction<'_>,
            _: &dyn Clock,
            slugs: &[String],
        ) -> Result<(), rentemester_engine::Error> {
            self.seen = slugs.to_vec();
            Err(error("backup_overdue"))
        }
    }
    let (_d, mut c, cookies, p, mut cat) = fixture();
    merge_pair(&mut c, &cookies, &p, &cat);
    cat.companies.push(RegisteredCompany {
        slug: "anchor".into(),
        name: "Synthetic anchor".into(),
        archived: false,
    });
    c.execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor) VALUES('u','anchor','grant','bookkeeper','user:u')",[]).unwrap();
    let mut guard = Guard { seen: vec![] };
    let e = execute_scoped(
        &mut c,
        &cookies,
        &Fixed,
        &p,
        &cat,
        "anchor",
        Action::ProposeMerge("from", &merge_input("into")),
        &mut guard,
    )
    .unwrap_err();
    assert_eq!(e.error.code, "backup_overdue");
    assert_eq!(guard.seen, ["a", "anchor", "b"]);
    assert_eq!(merge_event_count(&c), 0);
    let h = merge_hash(&mut c, &cookies, &p, &cat);
    guard.seen.clear();
    let e = execute_scoped(
        &mut c,
        &cookies,
        &Fixed,
        &p,
        &cat,
        "anchor",
        Action::ApproveMerge("from", &h),
        &mut guard,
    )
    .unwrap_err();
    assert_eq!(e.error.code, "backup_overdue");
    assert_eq!(guard.seen, ["a", "anchor", "b"]);
    assert_eq!(merge_event_count(&c), 1);
}
#[test]
fn party_merge_superseded_failure_rolls_back_approved_event() {
    let (_d, mut c, cookies, p, cat) = fixture();
    merge_pair(&mut c, &cookies, &p, &cat);
    let h = merge_hash(&mut c, &cookies, &p, &cat);
    c.execute_batch("CREATE TRIGGER synthetic_merge_failure BEFORE INSERT ON rm_party_events WHEN NEW.event_type='superseded' BEGIN SELECT RAISE(ABORT,'synthetic failure'); END;").unwrap();
    let e = execute(
        &mut c,
        &cookies,
        &Fixed,
        &p,
        &cat,
        "a",
        Action::ApproveMerge("from", &h),
    )
    .unwrap_err();
    assert_eq!(e.stage, FailureStage::NotCommitted);
    assert_eq!(merge_event_count(&c), 1);
    c.execute_batch("DROP TRIGGER synthetic_merge_failure")
        .unwrap();
    execute(
        &mut c,
        &cookies,
        &Fixed,
        &p,
        &cat,
        "a",
        Action::ApproveMerge("from", &h),
    )
    .unwrap();
    assert_eq!(merge_event_count(&c), 3);
}

#[test]
fn party_merge_transfer_fence_blocks_both_operations() {
    let (_d, mut c, cookies, p, cat) = fixture();
    merge_pair(&mut c, &cookies, &p, &cat);
    let h = merge_hash(&mut c, &cookies, &p, &cat);
    let tx = c
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    rentemester_storage::workspace_transfer::append(
        &tx,
        rentemester_storage::workspace_transfer::Fact::Frozen {
            transfer_id: "a".repeat(32),
            source_instance_id: "b".repeat(32),
            workspace_id: "w".into(),
            destination: "Synthetic destination".into(),
            reason: "Synthetic explicit review".into(),
            companies: vec![rentemester_storage::workspace_transfer::Company {
                slug: "a".into(),
                ledger_uuid: "c".repeat(32),
            }],
        },
        "user:u",
        "2026-01-01T00:00:00.000Z",
    )
    .unwrap();
    tx.commit().unwrap();
    for action in [
        Action::ProposeMerge("from", &merge_input("into")),
        Action::ApproveMerge("from", &h),
    ] {
        let e = execute(&mut c, &cookies, &Fixed, &p, &cat, "a", action).unwrap_err();
        assert_eq!(e.error.code, "workspace_transfer_fenced");
    }
    assert_eq!(merge_event_count(&c), 1);
}
