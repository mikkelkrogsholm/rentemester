use super::*;
use serde_json::json;
const AT: &str = "2026-01-01T00:00:00.000Z";
fn db() -> Connection {
    let mut c = Connection::open_in_memory().unwrap();
    crate::control_migration::initialize_empty(&mut c).unwrap();
    c
}
fn fixture() -> Connection {
    let mut c = db();
    let tx = c
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    let mut manifest:Manifest=serde_json::from_value(json!({"version":1,"groups":[{"id":"synthetic","name":" Cafe\u{301} ","memberships":[{"id":"a-member","companySlug":"a","validFrom":"2020-01-01"},{"id":"b-member","companySlug":"b","validFrom":"2020-01-01"}],"ownership":[{"id":"owns-b","parentCompanySlug":"a","childCompanySlug":"b","basisPoints":10000,"evidenceRefs":["synthetic ownership"],"validFrom":"2020-01-01"}]}]})).unwrap();
    let registered = BTreeSet::from(["a".into(), "b".into()]);
    manifest = manifest.normalize(&registered).unwrap();
    source::apply_manifest(&tx, &manifest, None, "user:old-maker", AT).unwrap();
    let old = source::current(&tx, &registered).unwrap().unwrap().1;
    manifest.groups[0].name = "Synthetic renamed group".into();
    source::apply_manifest(&tx, &manifest, Some(&old), "user:old-owner", AT).unwrap();
    for (id, kinds) in [
        ("revoked", vec!["proposed", "approved", "revoked"]),
        ("active", vec!["proposed", "approved"]),
        ("pending", vec!["proposed"]),
    ] {
        let m:Mapping=serde_json::from_value(json!({"id":id,"groupId":"synthetic","leftCompanySlug":"a","rightCompanySlug":"b","leftAccountNos":[if id=="pending"{"1401"}else{"1400"}],"rightAccountNos":["2400"],"leftPosition":"receivable","rightPosition":"payable","evidenceRefs":["\u{e000}","😀"," Cafe\u{301} "],"validFrom":"2020-01-01"})).unwrap();
        let m = m.normalize(&manifest).unwrap();
        for kind in kinds {
            let actor = if kind == "proposed" {
                "user:old-maker"
            } else {
                "user:old-reviewer"
            };
            source::append(
                &tx,
                &source::states(&tx, &manifest).unwrap(),
                &m,
                kind,
                actor,
                AT,
            )
            .unwrap();
        }
    }
    tx.commit().unwrap();
    c
}
fn bytes(c: &mut Connection) -> Vec<u8> {
    let tx = c
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    let b = export(&tx).unwrap();
    tx.rollback().unwrap();
    b
}
#[test]
fn full_history_canonical_unicode_hashes_states_and_actors_roundtrip_without_authority() {
    let mut c = fixture();
    let b = bytes(&mut c);
    let mut restored = db();
    initialize(&mut restored, &b, &["a".into(), "b".into()]).unwrap();
    assert_eq!(bytes(&mut restored), b);
    let reg = BTreeSet::from(["a".into(), "b".into()]);
    let tx = restored
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    let (m, h) = source::current(&tx, &reg).unwrap().unwrap();
    assert_eq!(m.groups[0].name, "Synthetic renamed group");
    assert_eq!(
        source::states(&tx, &m).unwrap()["active"].actor,
        "user:old-reviewer"
    );
    assert_eq!(
        source::states(&tx, &m).unwrap()["revoked"].status,
        "revoked"
    );
    assert_eq!(
        source::states(&tx, &m).unwrap()["pending"].status,
        "proposed"
    );
    assert_eq!(h.len(), 64);
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
            tx.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
    assert_eq!(
        tx.query_row("SELECT count(*) FROM workspace_audit", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0,
        "codec must not fabricate historical audit"
    );
}
#[test]
fn signed_company_set_is_exact_for_all_historical_refs_and_failure_rolls_back_both_tables() {
    let mut c = fixture();
    let b = bytes(&mut c);
    let mut target = db();
    assert!(initialize(&mut target, &b, &["a".into()]).is_err());
    for table in TABLES {
        assert_eq!(
            target
                .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
    let tx = c
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    assert!(validate_companies(&tx, &["a".into()]).is_err());
}
#[test]
fn removed_historical_group_member_still_requires_signed_company_identity() {
    let mut c = db();
    let tx = c
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    let reg = BTreeSet::from(["a".into(), "b".into()]);
    let mut manifest:Manifest=serde_json::from_value(json!({"version":1,"groups":[{"id":"synthetic-history","name":"Synthetic historical members","memberships":[{"id":"a-member","companySlug":"a","validFrom":"2020-01-01"},{"id":"b-member","companySlug":"b","validFrom":"2020-01-01"}],"ownership":[]}]})).unwrap();
    manifest = manifest.normalize(&reg).unwrap();
    source::apply_manifest(&tx, &manifest, None, "user:historical", AT).unwrap();
    let old = source::current(&tx, &reg).unwrap().unwrap().1;
    manifest.groups[0]
        .memberships
        .retain(|m| m.company_slug == "a");
    source::apply_manifest(&tx, &manifest, Some(&old), "user:historical", AT).unwrap();
    let bytes = export(&tx).unwrap();
    tx.rollback().unwrap();
    let mut target = db();
    assert!(initialize(&mut target, &bytes, &["a".into()]).is_err());
    initialize(&mut target, &bytes, &["a".into(), "b".into()]).unwrap();
}
#[test]
fn unknown_shape_types_hashes_and_lifecycle_corruption_never_partially_restore() {
    let mut c = fixture();
    let b = bytes(&mut c);
    let base: serde_json::Value = serde_json::from_slice(&b).unwrap();
    for case in 0..6 {
        let mut bad = base.clone();
        match case {
            0 => bad["format"] = json!("unknown"),
            1 => {
                bad["tables"].as_object_mut().unwrap().remove(TABLES[0]);
            }
            2 => bad["tables"][TABLES[1]]["columns"][0] = json!("different"),
            3 => bad["tables"][TABLES[0]]["rows"][0][0] = json!({"type":"Text","value":"1"}),
            4 => {
                bad["tables"][TABLES[1]]["rows"][0][7] =
                    json!({"type":"Text","value":"user:changed"})
            }
            _ => bad["tables"][TABLES[0]]["rows"][1][2] = json!({"type":"Null"}),
        }
        let mut target = db();
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
                0
            );
        }
    }
}
#[test]
fn overwrite_and_nonempty_unencoded_sections_refuse() {
    let mut c = fixture();
    let b = bytes(&mut c);
    assert!(initialize(&mut c, &b, &["a".into(), "b".into()]).is_err());
    let mut c = fixture();
    c.execute(
        "INSERT INTO rm_intercompany_dispositions VALUES('synthetic','{}',?,'2026-01-01')",
        ["a".repeat(64)],
    )
    .unwrap();
    let tx = c
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    // A malformed retained disposition still refuses after the old blanket guard is removed.
    assert!(export_control(&tx).is_err());
    let mut c = fixture();
    c.execute("INSERT INTO rm_consolidation_profile_events(profile_id,event_type,profile_hash,canonical_profile,event_hash,actor,created_at) VALUES('synthetic','proposed',?,'{}',?,'user:old','2026-01-01T00:00:00Z')",["a".repeat(64),"b".repeat(64)]).unwrap();
    let tx = c
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    assert!(
        export_control(&tx)
            .err()
            .unwrap()
            .message
            .contains("consolidation")
    );
}
#[test]
fn valid_group_and_mapping_control_export_has_one_exact_new_section() {
    let mut c = fixture();
    let tx = c
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    let exported = export_control(&tx).unwrap();
    assert_eq!(exported.intercompany_bytes, export(&tx).unwrap());
    assert_eq!(
        exported.unsupported,
        vec![("intercompany", 0), ("consolidation", 0)]
    );
    assert!(
        tx.execute(
            "UPDATE rm_group_manifest_events SET actor='user:changed'",
            []
        )
        .is_err()
    );
    assert!(
        tx.execute("DELETE FROM rm_intercompany_mapping_events", [])
            .is_err()
    );
}

#[test]
fn v1_compatibility_decodes_empty_dispositions_without_rewriting_old_bytes() {
    let c = fixture();
    let v1 = dump_version(&c, true).unwrap();
    let mut restored = db();
    initialize(&mut restored, &v1, &["a".into(), "b".into()]).unwrap();
    assert_eq!(dump_version(&restored, true).unwrap(), v1);
    assert_eq!(
        restored
            .query_row(
                "SELECT count(*) FROM rm_intercompany_dispositions",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
    let v2 = bytes(&mut restored);
    let codec: Business = serde_json::from_slice(&v2).unwrap();
    assert_eq!(codec.format, FORMAT);
    assert_eq!(codec.tables.len(), 6);
}
#[test]
fn disposition_v2_history_service_principal_and_original_references_roundtrip_or_refuse() {
    use crate::intercompany_dispositions as d;
    let mut c = fixture();
    let tx = c
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    let party=serde_json::from_value(json!({"partyId":"synthetic-codec-party","kind":"organization","name":"Synthetic codec party","source":"synthetic","observedAt":AT,"reviewAssertion":"Explicit synthetic review","role":"related_company"})).unwrap();
    crate::parties::create(&tx, &party, "unused", "user:owner", AT).unwrap();
    let record=serde_json::from_value(json!({"recordId":"synthetic-codec-record","type":"intercompany_agreement","bytesBase64":"unused","filename":"synthetic.txt","source":"synthetic","receivedAt":AT,"uploader":"synthetic"})).unwrap();
    crate::corporate_records::ingest(
        &tx,
        &record,
        b"synthetic codec agreement",
        "unused",
        &[],
        "user:owner",
    )
    .unwrap();
    let disposition=serde_json::from_value(json!({"dispositionId":"synthetic-codec-disposition","type":"loan","economicDate":"2026-01-01","amount":12.34,"currency":"DKK","partyIds":["synthetic-codec-party"],"evidenceRecordIds":["synthetic-codec-record"],"left":{"companySlug":"a","role":"lender","expectedSide":"receivable"},"right":{"companySlug":"b","role":"borrower","expectedSide":"payable"}})).unwrap();
    let mut action = d::Action::Propose(d::Propose {
        disposition,
        confirm: true,
    });
    d::prepare(&tx, &mut action).unwrap();
    let proposed = d::execute(
        &tx,
        &action,
        &[],
        "service:historic",
        &d::Principal {
            kind: "service".into(),
            id: "historic-service".into(),
        },
        AT,
    )
    .unwrap();
    let approved = d::Action::Approve(d::Exact {
        disposition_id: "synthetic-codec-disposition".into(),
        payload_hash: proposed["payloadHash"].as_str().unwrap().into(),
        confirm: true,
    });
    d::execute(
        &tx,
        &approved,
        &[],
        "user:reviewer",
        &d::Principal {
            kind: "user".into(),
            id: "reviewer".into(),
        },
        AT,
    )
    .unwrap();
    let snapshots: Vec<_> = ["a", "b"]
        .into_iter()
        .enumerate()
        .map(|(n, company)| d::JournalSnapshot {
            company_slug: company.into(),
            ledger_uuid: format!("{n:032x}"),
            currency: "DKK".into(),
            ledger_head_hash: Some(format!("{:064x}", n + 1)),
            entries: vec![d::JournalEntry {
                id: 1,
                entry_no: "SYN-1".into(),
                transaction_date: "2026-01-01".into(),
                entry_hash: format!("{:064x}", n + 1),
                status: "posted".into(),
                reversal_of_entry_id: None,
                reversed: false,
                lines: vec![d::JournalLine {
                    debit: rentemester_engine::money::Money::parse(if n == 0 {
                        "12.34"
                    } else {
                        "0"
                    })
                    .unwrap(),
                    credit: rentemester_engine::money::Money::parse(if n == 0 {
                        "0"
                    } else {
                        "12.34"
                    })
                    .unwrap(),
                }],
            }],
        })
        .collect();
    for (n, side) in ["left", "right"].into_iter().enumerate() {
        let link = d::Action::Link(d::Link {
            disposition_id: "synthetic-codec-disposition".into(),
            payload_hash: proposed["payloadHash"].as_str().unwrap().into(),
            confirm: true,
            side: side.into(),
            journal_entry_id: 1,
            ledger_head_hash: snapshots[n].ledger_head_hash.clone(),
        });
        d::execute(
            &tx,
            &link,
            &snapshots,
            "user:reviewer",
            &d::Principal {
                kind: "user".into(),
                id: "reviewer".into(),
            },
            AT,
        )
        .unwrap();
    }
    let settle = d::Action::Settle(d::Settle {
        disposition_id: "synthetic-codec-disposition".into(),
        payload_hash: proposed["payloadHash"].as_str().unwrap().into(),
        confirm: true,
        settlement_evidence_record_ids: vec!["synthetic-codec-record".into()],
    });
    d::execute(
        &tx,
        &settle,
        &snapshots,
        "user:reviewer",
        &d::Principal {
            kind: "user".into(),
            id: "reviewer".into(),
        },
        AT,
    )
    .unwrap();
    assert_eq!(d::validate_history(&tx).unwrap().len(), 2);
    let parties = super::super::parties::export(&tx).unwrap();
    let corporate = super::super::corporate::export(&tx).unwrap();
    let signed = export(&tx).unwrap();
    tx.commit().unwrap();
    let mut restored = db();
    let registered = ["a".into(), "b".into()];
    super::super::parties::initialize(&mut restored, &parties, &registered).unwrap();
    super::super::corporate::initialize(&mut restored, &corporate, &registered).unwrap();
    initialize(&mut restored, &signed, &registered).unwrap();
    assert_eq!(bytes(&mut restored), signed);
    let principal: String = restored
        .query_row(
            "SELECT principal_kind FROM rm_intercompany_disposition_events ORDER BY id LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(principal, "service");
    let mut corrupt: Business = serde_json::from_slice(&signed).unwrap();
    let events = corrupt
        .tables
        .get_mut("rm_intercompany_disposition_events")
        .unwrap();
    events.rows[0][9] = Cell::Text("f".repeat(64));
    let corrupt = serde_json::to_vec(&corrupt).unwrap();
    let mut target = db();
    super::super::parties::initialize(&mut target, &parties, &registered).unwrap();
    super::super::corporate::initialize(&mut target, &corporate, &registered).unwrap();
    assert!(initialize(&mut target, &corrupt, &registered).is_err());
    assert_eq!(
        target
            .query_row(
                "SELECT count(*) FROM rm_intercompany_dispositions",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
}
