use super::*;
use crate::corporate_records as corp;
const AT: &str = "2026-01-01T00:00:00.000Z";
fn set(v: &[&str]) -> BTreeSet<String> {
    v.iter().map(|s| s.to_string()).collect()
}
fn fresh() -> Connection {
    let mut c = Connection::open_in_memory().unwrap();
    let mut l = Connection::open_in_memory().unwrap();
    crate::fresh::initialize_empty_pair(&mut c, &mut l, "Synthetic corporate backup").unwrap();
    c
}
fn fixture() -> Connection {
    let mut c = fresh();
    let tx = c
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    let party=serde_json::from_value(json!({"partyId":"party-corporate-source","kind":"organization","name":"Synthetic corporate source party","aliases":[],"identifiers":[],"source":"Synthetic reviewed party source","observedAt":"2026-01-01","reviewAssertion":"Explicit synthetic source review","role":"supplier"})).unwrap();
    crate::parties::create(&tx, &party, "unused", "user:historical", AT).unwrap();
    crate::parties::role(
        &tx,
        "party-corporate-source",
        "a",
        &crate::parties::Role {
            role: "supplier".into(),
            defaults: None,
        },
        "user:historical",
        AT,
    )
    .unwrap();
    for (id, kind, sensitivity) in [
        ("record-a", "annual_report", "restricted"),
        ("record-b", "articles", "confidential"),
        ("record-legacy", "other", "normal"),
        ("record-unscoped", "other", "normal"),
    ] {
        let input=serde_json::from_value(json!({"recordId":id,"type":kind,"bytesBase64":"unused-in-storage","filename":format!("{id}.txt"),"source":"Synthetic source brief","receivedAt":AT,"recordDate":"2025-12-31","retentionUntil":"2030-01-01","uploader":"Historical factual uploader","sensitivity":sensitivity})).unwrap();
        let mut links = vec![
            corp::Link {
                r#type: "company".into(),
                id: "a".into(),
            },
            corp::Link {
                r#type: "company".into(),
                id: "b".into(),
            },
        ];
        if id == "record-a" {
            for (k, id) in [
                ("party", "party-corporate-source"),
                ("ownership", "opaque-ownership-evidence"),
                ("disposition", "opaque-disposition-evidence"),
                ("legacy_evidence_ref", "opaque-legacy-evidence"),
            ] {
                links.push(corp::Link {
                    r#type: k.into(),
                    id: id.into(),
                });
            }
        }
        corp::ingest(
            &tx,
            &input,
            &[b"\0\xffSynthetic original ", id.as_bytes()].concat(),
            "unused",
            &links,
            "user:historical-owner",
        )
        .unwrap();
    }
    corp::enrich(
        &tx,
        "record-a",
        &corp::Enrich {
            record_id: None,
            assertion: "Synthetic append-only enrichment".into(),
        },
        "user:historical-reviewer",
        "2026-02-01T00:00:00.000Z",
    )
    .unwrap();
    // Full source-allowed history, never invent one effective replacement or forbid cycles/self-links.
    for (old, new, reason) in [
        ("record-a", "record-b", "First reviewed replacement"),
        ("record-a", "record-b", "Second source assertion"),
        ("record-b", "record-a", "Source permits cyclic history"),
        ("record-a", "record-a", "Source permits self-linked history"),
    ] {
        corp::supersede(
            &tx,
            old,
            &corp::Supersede {
                record_id: None,
                replacement_record_id: new.into(),
                reason: reason.into(),
            },
            "user:historical-reviewer",
            "2026-02-02T00:00:00.000Z",
        )
        .unwrap();
    }
    // Migration20 had no backfill. Preserve unasserted legacy links without adopting visibility.
    let trigger:String=tx.query_row("SELECT sql FROM sqlite_master WHERE name='rm_corporate_record_scope_assertions_no_delete'",[],|r|r.get(0)).unwrap();
    tx.execute_batch("DROP TRIGGER rm_corporate_record_scope_assertions_no_delete;DELETE FROM rm_corporate_record_scope_assertions WHERE record_id IN ('record-legacy','record-unscoped');").unwrap();
    tx.execute_batch(&trigger).unwrap();
    // Relink may create new scope provenance while the old payload-hash event remains deduplicated.
    corp::link(
        &tx,
        "record-legacy",
        &corp::Link {
            r#type: "company".into(),
            id: "b".into(),
        },
        "user:new-assertion-reviewer",
        "2026-03-01T00:00:00.000Z",
    )
    .unwrap();
    tx.commit().unwrap();
    c.execute_batch("INSERT INTO user(id,name,email,emailVerified,twoFactorEnabled,createdAt,updatedAt)VALUES('old-auth-owner','Synthetic old owner','source@example.invalid',1,1,'2026-01-01','2026-01-01');INSERT INTO session(id,expiresAt,token,createdAt,updatedAt,userId)VALUES('old-session','2026-01-01T12:00:00.000Z','SYNTHETIC-EXCLUDED-SECRET','2026-01-01','2026-01-01','old-auth-owner');INSERT INTO twoFactor(id,secret,backupCodes,userId,verified)VALUES('old-factor','SYNTHETIC-EXCLUDED-MFA','SYNTHETIC-EXCLUDED-RECOVERY','old-auth-owner',1);INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('old-auth-owner','a','grant','owner','system:synthetic');").unwrap();
    c
}
fn export(c: &mut Connection) -> Control {
    let tx = c
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    let result = super::super::export_control(&tx).unwrap();
    validate_companies(&tx, &["a".into(), "b".into()]).unwrap();
    tx.rollback().unwrap();
    result
}
fn views(c: &mut Connection) -> Value {
    let tx = c.transaction().unwrap();
    let mut result = vec![];
    for read in [set(&[]), set(&["a"]), set(&["b"]), set(&["a", "b"])] {
        let mut rows = vec![];
        for id in ["record-a", "record-b", "record-legacy", "record-unscoped"] {
            rows.push(json!({"record":corp::inspect(&tx,id).unwrap(),"original":corp::download(&tx,id).unwrap(),"visible":corp::visible(&tx,id,&read).unwrap(),"readOnlyDetail":corp::detail_visible(&tx,id,&read,&set(&[])).unwrap(),"currentWritableDetail":corp::detail_visible(&tx,id,&read,&read).unwrap()}));
        }
        result.push(json!({"list":corp::list(&tx,&read,0,100).unwrap(),"rows":rows}));
    }
    json!(result)
}
#[test]
fn complete_originals_history_legacy_scope_and_native_visibility_survive_restore_restart() {
    let mut source = fixture();
    let expected = views(&mut source);
    let exported = export(&mut source);
    assert!(!String::from_utf8_lossy(&exported.corporate_bytes).contains("SYNTHETIC-EXCLUDED"));
    assert!(
        !exported
            .unsupported
            .iter()
            .any(|(section, _)| *section == "corporate-records")
    );
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("private-restored.sqlite");
    let mut target = Connection::open(&path).unwrap();
    super::super::initialize_control(&mut target, &exported.bytes, &"a".repeat(64)).unwrap();
    super::super::initialize_parties(
        &mut target,
        &exported.parties_bytes,
        &["a".into(), "b".into()],
    )
    .unwrap();
    initialize(
        &mut target,
        &exported.corporate_bytes,
        &["a".into(), "b".into()],
    )
    .unwrap();
    assert_eq!(views(&mut target), expected);
    let tx = target.transaction().unwrap();
    assert_eq!(dump(&tx).unwrap(), exported.corporate_bytes);
    tx.rollback().unwrap();
    for table in [
        "user",
        "account",
        "session",
        "twoFactor",
        "apikey",
        "rm_company_membership_events",
        "rm_workspace_user_access_events",
    ] {
        assert_eq!(
            target
                .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
    drop(target);
    let mut reopened = Connection::open(&path).unwrap();
    assert_eq!(views(&mut reopened), expected);
    assert!(
        initialize(
            &mut reopened,
            &exported.corporate_bytes,
            &["a".into(), "b".into()]
        )
        .is_err()
    );
}
fn rows<'a>(v: &'a mut Value, table: &str) -> &'a mut Vec<Value> {
    v["tables"][table]["rows"].as_array_mut().unwrap()
}
fn text(value: impl Into<String>) -> Value {
    json!({"type":"Text","value":value.into()})
}
fn event_payload(v: &mut Value, index: usize, change: impl FnOnce(&mut Value)) {
    let row = &mut rows(v, TABLES[1])[index];
    let mut p: Value = serde_json::from_str(row[5]["value"].as_str().unwrap()).unwrap();
    change(&mut p);
    let raw = p.to_string();
    row[4] = text(corp::hash(raw.as_bytes()));
    row[5] = text(raw);
}
#[test]
fn corrupt_unknown_crossrefs_and_typed_mirrors_refuse_with_whole_section_rollback() {
    let mut source = fixture();
    let exported = export(&mut source);
    let good: Value = serde_json::from_slice(&exported.corporate_bytes).unwrap();
    for case in 0..15 {
        let mut bad = good.clone();
        match case {
            0 => bad["format"] = json!("unsupported-corporate-v2"),
            1 => bad["tables"]["user"] = json!({"columns":["secret"],"rows":[]}),
            2 => bad["tables"][TABLES[0]]["columns"][1] = json!("unknown_bytes"),
            3 => rows(&mut bad, TABLES[0])[0][1] = json!({"type":"BlobBase64","value":"AA=="}),
            4 => rows(&mut bad, TABLES[0])[0][1] = text("not-a-blob"),
            5 => rows(&mut bad, TABLES[1])[0][4] = text("a".repeat(64)),
            6 => {
                event_payload(&mut bad, 0, |p| {
                    p["filename"] = json!("different-original.txt")
                });
            }
            7 => {
                rows(&mut bad, TABLES[2]).pop();
            }
            8 => {
                rows(&mut bad, TABLES[3])[0][3] = text("unknown-scope");
            }
            9 => {
                let i = rows(&mut bad, TABLES[1])
                    .iter()
                    .position(|r| r[2]["value"] == "superseded")
                    .unwrap();
                event_payload(&mut bad, i, |p| {
                    p["replacementRecordId"] = json!("missing-original")
                });
            }
            10 => {
                let mut row = rows(&mut bad, TABLES[1])[0].clone();
                row[0] = json!({"type":"Integer","value":99999});
                let mut payload: Value =
                    serde_json::from_str(row[5]["value"].as_str().unwrap()).unwrap();
                payload["type"] = json!("articles");
                let raw = payload.to_string();
                row[3] = text("articles");
                row[4] = text(corp::hash(raw.as_bytes()));
                row[5] = text(raw);
                rows(&mut bad, TABLES[1]).push(row);
            }
            11 => {
                let i = rows(&mut bad, TABLES[1])
                    .iter()
                    .position(|r| r[2]["value"] == "enriched")
                    .unwrap();
                event_payload(&mut bad, i, |p| p["unexpected"] = json!(true));
            }
            12 => rows(&mut bad, TABLES[3])[0][0] = text("1"),
            13 => rows(&mut bad, TABLES[0])[0][8] = json!({"type":"Integer","value":7}),
            14 => {
                let table = bad["tables"].as_object_mut().unwrap();
                table.remove(TABLES[3]);
            }
            _ => unreachable!(),
        }
        let mut target = fresh();
        super::super::initialize_parties(
            &mut target,
            &exported.parties_bytes,
            &["a".into(), "b".into()],
        )
        .unwrap();
        assert!(
            initialize(
                &mut target,
                &serde_json::to_vec(&bad).unwrap(),
                &["a".into(), "b".into()]
            )
            .is_err(),
            "case{case}"
        );
        for table in TABLES {
            assert_eq!(
                target
                    .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r
                        .get::<_, i64>(0))
                    .unwrap(),
                0,
                "case{case}"
            );
        }
        assert!(
            target
                .query_row("SELECT count(*) FROM rm_party_events", [], |r| r
                    .get::<_, i64>(0))
                .unwrap()
                > 0
        );
    }
    // Consistent signed company references still require the actual manifest company catalogue.
    let mut target = fresh();
    super::super::initialize_parties(
        &mut target,
        &exported.parties_bytes,
        &["a".into(), "b".into()],
    )
    .unwrap();
    assert!(initialize(&mut target, &exported.corporate_bytes, &["a".into()]).is_err());
    let mut target = fresh();
    assert!(
        initialize(
            &mut target,
            &exported.corporate_bytes,
            &["a".into(), "b".into()]
        )
        .is_err(),
        "party ref requires restored creation"
    );
}
#[test]
fn corrupt_source_and_unknown_schema_fail_export() {
    let mut source = fixture();
    source.execute_batch("DROP TRIGGER rm_corporate_record_bytes_no_update;UPDATE rm_corporate_record_bytes SET original_bytes=x'00' WHERE record_id='record-a';").unwrap();
    let tx = source.transaction().unwrap();
    assert!(super::export(&tx).is_err());
    tx.rollback().unwrap();
    let mut source = fixture();
    source
        .execute_batch("ALTER TABLE rm_corporate_record_bytes ADD COLUMN unknown_future_field TEXT")
        .unwrap();
    let tx = source.transaction().unwrap();
    assert!(super::export(&tx).is_err());
    tx.rollback().unwrap();
}
