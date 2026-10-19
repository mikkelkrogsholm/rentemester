use super::*;
const AT: &str = "2026-01-01T00:00:00.000Z";
fn fresh() -> Connection {
    let mut c = Connection::open_in_memory().unwrap();
    let mut l = Connection::open_in_memory().unwrap();
    crate::fresh::initialize_empty_pair(&mut c, &mut l, "Synthetic ownership codec").unwrap();
    c
}
fn sets(v: &[&str]) -> BTreeSet<String> {
    v.iter().map(|s| s.to_string()).collect()
}
fn dependencies(c: &mut Connection) {
    let tx = c.transaction().unwrap();
    let party=serde_json::from_value(json!({"partyId":"party-owner","kind":"organization","name":"Synthetic owner party","aliases":[],"identifiers":[],"source":"Synthetic reviewed observation","observedAt":AT,"reviewAssertion":"Explicit synthetic review","role":"supplier"})).unwrap();
    crate::parties::create(&tx, &party, "unused", "user:historical", AT).unwrap();
    crate::parties::role(
        &tx,
        "party-owner",
        "a",
        &crate::parties::Role {
            role: "supplier".into(),
            defaults: None,
        },
        "user:historical",
        AT,
    )
    .unwrap();
    let input=serde_json::from_value(json!({"recordId":"record-ownership","type":"ownership_register","bytesBase64":"unused","filename":"synthetic-owner.txt","source":"Synthetic owner evidence","receivedAt":AT,"uploader":"Synthetic declared uploader","sensitivity":"restricted"})).unwrap();
    crate::corporate_records::ingest(
        &tx,
        &input,
        b"Synthetic immutable owner evidence",
        "unused",
        &[crate::corporate_records::Link {
            r#type: "company".into(),
            id: "a".into(),
        }],
        "user:historical",
    )
    .unwrap();
    tx.commit().unwrap();
}
fn input(id: &str, owner: Value, from: &str, bp: u16) -> source::Propose {
    serde_json::from_value(json!({"snapshotId":id,"source":"Synthetic ownership observation","observedAt":AT,"facts":[{"owner":owner,"ownedCompanySlug":"b","validFrom":from,"economicBasisPoints":bp,"votingBasisPoints":1000,"shareClass":"Synthetic A","controlType":"equity","jurisdiction":"DK","evidenceRefs":["opaque:unregistered-source","record-ownership"]}]})).unwrap()
}
fn propose(tx: &Transaction<'_>, i: &source::Propose) -> Value {
    source::propose(
        tx,
        i,
        &source::normalized(&i.facts).unwrap(),
        "unused",
        "user:maker",
        "maker",
        AT,
    )
    .unwrap()
}
fn review(tx: &Transaction<'_>, s: &Value, decision: &str) {
    source::review(
        tx,
        &source::Review {
            snapshot_id: s["snapshotId"].as_str().unwrap().into(),
            decision: decision.into(),
        },
        "user:reviewer",
        "reviewer",
        "2026-01-02T00:00:00.000Z",
    )
    .unwrap();
}
fn apply(tx: &Transaction<'_>, s: &Value) {
    source::apply(
        tx,
        &source::Apply {
            snapshot_id: s["snapshotId"].as_str().unwrap().into(),
            snapshot_hash: s["snapshotHash"].as_str().unwrap().into(),
            diff_hash: s["diffHash"].as_str().unwrap().into(),
        },
        "user:applier",
        "applier",
        "2026-01-03T00:00:00.000Z",
    )
    .unwrap();
}
fn fixture() -> Connection {
    let mut c = fresh();
    dependencies(&mut c);
    let tx = c.transaction().unwrap();
    let owner = json!({"kind":"company","companySlug":"a"});
    let first = propose(&tx, &input("initial", owner.clone(), "2000-01-01", 2000));
    review(&tx, &first, "approved");
    apply(&tx, &first);
    let later = propose(&tx, &input("later", owner.clone(), "2001-01-01", 3000));
    review(&tx, &later, "approved");
    apply(&tx, &later);
    let third = propose(&tx, &input("third", owner.clone(), "2002-01-01", 4000));
    review(&tx, &third, "approved");
    apply(&tx, &third);
    propose(&tx, &input("pending", owner.clone(), "2010-01-01", 5000));
    let approved = propose(
        &tx,
        &input("reviewed-not-applied", owner.clone(), "2011-01-01", 5000),
    );
    review(&tx, &approved, "approved");
    let rejected = propose(&tx, &input("rejected", owner, "2012-01-01", 5000));
    review(&tx, &rejected, "rejected");
    let mut party = input(
        "party-observation",
        json!({"kind":"party","partyId":"party-owner"}),
        "2000-01-01",
        1000,
    );
    party.facts[0].economic_basis_points = None;
    party.facts[0].economic_interval_basis_points = Some(rentemester_engine::ownership::Interval {
        min: 1000,
        max: 2000,
    });
    party.facts[0].valid_to_exclusive = Some("2030-01-01".into());
    let p = propose(&tx, &party);
    review(&tx, &p, "approved");
    apply(&tx, &p);
    tx.commit().unwrap();
    c.execute_batch("INSERT INTO user(id,name,email,emailVerified,twoFactorEnabled,createdAt,updatedAt)VALUES('excluded-old-owner','Synthetic excluded owner','excluded@example.invalid',1,1,'2026-01-01','2026-01-01');INSERT INTO session(id,expiresAt,token,createdAt,updatedAt,userId)VALUES('excluded-session','2026-01-02','SYNTHETIC-OWNERSHIP-EXCLUDED-SECRET','2026-01-01','2026-01-01','excluded-old-owner');INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('excluded-old-owner','a','grant','owner','system:synthetic');").unwrap();
    c
}
fn projection(c: &mut Connection) -> Value {
    let tx = c.transaction().unwrap();
    let mut out = vec![];
    for visible in [sets(&[]), sets(&["a"]), sets(&["b"]), sets(&["a", "b"])] {
        for writable in [sets(&[]), sets(&["a", "b"])] {
            for at in [
                "1999-01-01",
                "2000-06-01",
                "2001-06-01",
                "2002-06-01",
                "2031-01-01",
            ] {
                let approved: Vec<_> = source::approved(&tx)
                    .unwrap()
                    .into_iter()
                    .map(|(_, f)| f)
                    .filter(|f| f.active(at))
                    .collect();
                let allowed = |f: &Fact| {
                    f.companies().is_subset(&visible)
                        && match &f.owner {
                            Endpoint::Company { .. } => true,
                            Endpoint::Party { party_id } => {
                                crate::parties::visible(&tx, party_id, &visible).unwrap()
                            }
                        }
                        && f.evidence_refs.iter().all(|id| {
                            !crate::corporate_records::exists(&tx, id).unwrap()
                                || crate::corporate_records::detail_visible(
                                    &tx, id, &visible, &writable,
                                )
                                .unwrap()
                        })
                };
                let facts: Vec<_> = approved.iter().filter(|f| allowed(f)).cloned().collect();
                let graph = source::graph(at, &approved, &facts);
                let history: Vec<_> = source::ids(&tx)
                    .unwrap()
                    .into_iter()
                    .filter_map(|id| {
                        let s = source::load(&tx, &id).unwrap();
                        s.facts.iter().all(allowed).then_some(s.public)
                    })
                    .collect();
                out.push(json!({"graph":graph,"projection":source::projection(&graph),"history":history}));
            }
        }
    }
    json!(out)
}
#[test]
fn codec_roundtrip_preserves_ownership_all_states_endings_visibility_history_and_file_reopen() {
    let mut c = fixture();
    let expected = projection(&mut c);
    let tx = c
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    let bytes = export(&tx).unwrap();
    assert!(!String::from_utf8_lossy(&bytes).contains("SYNTHETIC-OWNERSHIP-EXCLUDED"));
    let deps = super::super::export_control(&tx).unwrap();
    assert_eq!(deps.unsupported.len(), 2);
    tx.rollback().unwrap();
    let path = std::env::temp_dir().join(format!(
        "rentemester-ownership-codec-{}-{}.sqlite",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let mut target = Connection::open(&path).unwrap();
    crate::control_migration::initialize_empty(&mut target).unwrap();
    super::super::initialize_parties(&mut target, &deps.parties_bytes, &["a".into(), "b".into()])
        .unwrap();
    super::super::initialize_corporate(
        &mut target,
        &deps.corporate_bytes,
        &["a".into(), "b".into()],
    )
    .unwrap();
    initialize(&mut target, &bytes, &["a".into(), "b".into()]).unwrap();
    assert_eq!(projection(&mut target), expected);
    assert_eq!(dump(&target).unwrap(), bytes);
    for name in [
        "user",
        "session",
        "account",
        "twoFactor",
        "rm_company_membership_events",
    ] {
        assert_eq!(
            target
                .query_row(&format!("SELECT COUNT(*) FROM {name}"), [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            0,
            "{name}"
        );
    }
    target.close().unwrap();
    let mut reopened = Connection::open(&path).unwrap();
    assert_eq!(projection(&mut reopened), expected);
    assert_eq!(dump(&reopened).unwrap(), bytes);
    reopened.close().unwrap();
    std::fs::remove_file(path).unwrap();
}
#[test]
fn malformed_ownership_sections_refuse_atomically_without_altering_dependencies() {
    let mut c = fixture();
    let tx = c.transaction().unwrap();
    let bytes = export(&tx).unwrap();
    tx.rollback().unwrap();
    let original: Value = serde_json::from_slice(&bytes).unwrap();
    for case in 0..18 {
        let mut v = original.clone();
        match case {
            0 => v["format"] = json!("unknown"),
            1 => {
                v["tables"]["user"] = json!({"columns":[],"rows":[]});
            }
            2 => v["tables"][TABLES[0]]["columns"][0] = json!("unknown"),
            3 => v["tables"][TABLES[0]]["rows"][0][0] = json!({"type":"Text","value":"1"}),
            4 => v["tables"][TABLES[0]]["rows"][0][4]["value"] = json!("0".repeat(64)),
            5 => v["tables"][TABLES[0]]["rows"][0][7]["value"] = json!("{}"),
            6 => v["tables"][TABLES[1]]["rows"][0][2]["value"] = json!("approved"),
            7 => v["tables"][TABLES[1]]["rows"][1][5]["value"] = json!("maker"),
            8 => v["tables"][TABLES[2]]["rows"][0][10]["value"] = json!(9999),
            9 => v["tables"][TABLES[3]]["rows"][0][3]["value"] = json!("2020-01-01"),
            10 => v["tables"][TABLES[3]]["rows"][0][4]["value"] = json!("f".repeat(64)),
            11 => v["tables"][TABLES[3]]["rows"][0][8]["value"] = json!("forged-applier"),
            12 => v["tables"][TABLES[2]]["rows"]
                .as_array_mut()
                .unwrap()
                .reverse(),
            13 => {
                v["tables"][TABLES[1]]["rows"].as_array_mut().unwrap().pop();
            }
            14..=16 => {
                let row = if case == 15 { 1 } else { 0 };
                let mut diff: Value = serde_json::from_str(
                    v["tables"][TABLES[0]]["rows"][row][7]["value"]
                        .as_str()
                        .unwrap(),
                )
                .unwrap();
                if case == 14 {
                    diff["add"][0]["economicBasisPoints"] = json!(9999);
                }
                if case == 15 {
                    diff["endings"][0]["effectiveToExclusive"] = json!("2025-01-01");
                }
                if case == 16 {
                    diff["invented"] = json!(true);
                }
                v["tables"][TABLES[0]]["rows"][row][5]["value"] = json!(hash(&diff));
                v["tables"][TABLES[0]]["rows"][row][7]["value"] = json!(canonical(&diff));
            }
            _ => v["tables"][TABLES[1]]["rows"][1][2]["value"] = json!("rejected"),
        }
        if (14..=16).contains(&case) {
            let mut corrupt = fresh();
            dependencies(&mut corrupt);
            let tx = corrupt.transaction().unwrap();
            let forged: Business = serde_json::from_value(v.clone()).unwrap();
            insert_tables(&tx, &forged, &TABLES).unwrap();
            assert!(export(&tx).is_err(), "corrupt source case{case}");
        }
        let mut target = fresh();
        dependencies(&mut target);
        let tx = target.transaction().unwrap();
        let party_before = super::super::parties::export(&tx).unwrap();
        tx.rollback().unwrap();
        assert!(
            initialize(
                &mut target,
                &serde_json::to_vec(&v).unwrap(),
                &["a".into(), "b".into()]
            )
            .is_err(),
            "case{case}"
        );
        for name in TABLES {
            assert_eq!(
                target
                    .query_row(&format!("SELECT COUNT(*) FROM {name}"), [], |r| r
                        .get::<_, i64>(0))
                    .unwrap(),
                0,
                "case{case}/{name}"
            );
        }
        let tx = target.transaction().unwrap();
        assert_eq!(super::super::parties::export(&tx).unwrap(), party_before);
    }
    let mut no_party = fresh();
    assert!(initialize(&mut no_party, &bytes, &["a".into(), "b".into()]).is_err());
    let mut missing_company = fresh();
    dependencies(&mut missing_company);
    assert!(initialize(&mut missing_company, &bytes, &["a".into()]).is_err());
}
fn legacy_proposal(tx: &Transaction<'_>, id: &str, facts: &[Fact]) -> Value {
    let diff = legacy_diff(tx, facts).unwrap();
    let sh = hash(&json!({"source":"Synthetic historical v1","observedAt":AT,"facts":facts}));
    tx.execute("INSERT INTO rm_ownership_source_snapshots(snapshot_id,source,observed_at,snapshot_hash,diff_hash,canonical_facts,diff_json,actor,principal_kind,principal_id,created_at)VALUES(?,'Synthetic historical v1',?,?,?,?,?,'system:legacy','local_operator','historical',?)",params![id,AT,sh,hash(&diff),canonical(&json!(facts)),canonical(&diff),AT]).unwrap();
    tx.execute("INSERT INTO rm_ownership_snapshot_events(snapshot_id,event_type,actor,principal_kind,principal_id,created_at)VALUES(?,'proposed','system:legacy','local_operator','historical','2026-01-01T00:00:00.001Z')",[id]).unwrap();
    source::load(tx, id).unwrap().public
}
fn legacy_review_apply(tx: &Transaction<'_>, id: &str) {
    // Pre-24 source explicitly admitted same-principal reviews. These labels are
    // historical provenance, and must not become new owner grants on restoration.
    tx.execute("INSERT INTO rm_ownership_snapshot_events(snapshot_id,event_type,actor,principal_kind,principal_id,created_at)VALUES(?,'approved','system:legacy','local_operator','historical',?)",params![id,AT]).unwrap();
    legacy_apply(tx, id, AT).unwrap();
    tx.execute("INSERT INTO rm_ownership_snapshot_events(snapshot_id,event_type,actor,principal_kind,principal_id,created_at)VALUES(?,'applied','service:historical','service_account','historical',?)",params![id,AT]).unwrap();
}
#[test]
fn historical_v1_profiles_exact_replays_principals_and_current_v2_endings_survive() {
    let mut c = fresh();
    dependencies(&mut c);
    let tx = c.transaction().unwrap();
    let f = input(
        "unused",
        json!({"kind":"company","companySlug":"a"}),
        "2000-01-01",
        2000,
    )
    .facts;
    let first = legacy_proposal(&tx, "legacy-initial", &f);
    legacy_proposal(&tx, "legacy-concurrent-same-fact", &f);
    legacy_review_apply(&tx, "legacy-initial");
    legacy_review_apply(&tx, "legacy-concurrent-same-fact");
    let unchanged = legacy_proposal(&tx, "legacy-unchanged", &f);
    assert_eq!(unchanged["diff"]["add"], json!([]));
    let v2 = propose(
        &tx,
        &input(
            "native-successor",
            json!({"kind":"company","companySlug":"a"}),
            "2001-01-01",
            3000,
        ),
    );
    review(&tx, &v2, "approved");
    apply(&tx, &v2);
    assert_eq!(first["diff"]["version"], 1);
    let bytes = export(&tx).unwrap();
    tx.commit().unwrap();
    let expected = projection(&mut c);
    let mut restored = fresh();
    dependencies(&mut restored);
    initialize(&mut restored, &bytes, &["a".into(), "b".into()]).unwrap();
    assert_eq!(dump(&restored).unwrap(), bytes);
    assert_eq!(projection(&mut restored), expected);
}
