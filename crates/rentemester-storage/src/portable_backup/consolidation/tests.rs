use super::*;
use rentemester_engine::{eliminations::BalancePayload, intercompany::Mapping};
use serde_json::json;
const AT: &str = "2026-01-01T00:00:00.000Z";
fn db() -> Connection {
    let mut c = Connection::open_in_memory().unwrap();
    crate::control_migration::initialize_empty(&mut c).unwrap();
    c
}
fn profile(group: &str) -> Profile {
    let mut p: Profile = serde_json::from_str(include_str!(
        "../../../../rentemester-engine/src/consolidation/profile-source-canonical.json"
    ))
    .unwrap();
    p.group_id = group.into();
    p
}
fn payload(id: &str, m: &Mapping, version: u8, date: &str) -> BalancePayload {
    let side = |company: &str, accounts: &Vec<String>, receivable: bool| json!({"companySlug":company,"accountNos":accounts,"selectionHash":"b".repeat(64),"ledgerHeadHash":"c".repeat(64),"entryCount":1,"creditOre":if receivable{"1234"}else{"0"},"debitOre":if receivable{"0"}else{"1234"},"position":if receivable{"receivable"}else{"payable"}});
    let mut v = json!({"version":version,"id":id,"kind":"intercompany-balance","groupId":m.group_id,"mappingId":m.id,"mappingHash":crate::intercompany::sha(&m.canonical().unwrap()),"asOf":date,"currency":"DKK","amountOre":"1234","left":side(&m.left_company_slug,&m.left_account_nos,m.left_position==rentemester_engine::intercompany::Position::Receivable),"right":side(&m.right_company_slug,&m.right_account_nos,m.right_position==rentemester_engine::intercompany::Position::Receivable),"evidenceRefs":["synthetic-history"]});
    if version == 1 {
        v["left"].as_object_mut().unwrap().remove("position");
        v["left"].as_object_mut().unwrap().remove("debitOre");
        v["left"]["creditOre"] = json!("1234");
        v["right"].as_object_mut().unwrap().remove("position");
        v["right"].as_object_mut().unwrap().remove("creditOre");
        v["right"]["debitOre"] = json!("1234");
    }
    serde_json::from_value(v).unwrap()
}
fn fixture() -> Connection {
    let mut c = db();
    let tx = c
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    let reg = BTreeSet::from(["a".into(), "b".into()]);
    let group = |id: &str| json!({"id":id,"name":"Synthetic group","memberships":[{"id":"a-member","companySlug":"a","validFrom":"2000-01-01"},{"id":"b-member","companySlug":"b","validFrom":"2000-01-01"}],"ownership":[{"id":"owns-b","parentCompanySlug":"a","childCompanySlug":"b","basisPoints":10000,"evidenceRefs":["synthetic"],"validFrom":"2000-01-01"}]});
    let mut m: Manifest = serde_json::from_value(
        json!({"version":1,"groups":[group("synthetic"),group("historical")]}),
    )
    .unwrap();
    m = m.normalize(&reg).unwrap();
    crate::intercompany::apply_manifest(&tx, &m, None, "user:historical-owner", AT).unwrap();
    // Same profile ID moves to a new authenticated group after a reasoned lifecycle;
    // the old group's eventual removal must not rewrite historic canonical profiles.
    let old = profile("historical").normalize(&m).unwrap();
    let proposed = crate::consolidation::propose(&tx, &m, &old, "user:maker", AT).unwrap();
    let s = crate::consolidation::states(&tx, &m)
        .unwrap()
        .remove(&old.id)
        .unwrap();
    crate::consolidation::approve(
        &tx,
        &s,
        proposed["profileHash"].as_str().unwrap(),
        "user:reviewer",
        AT,
    )
    .unwrap();
    let s = crate::consolidation::states(&tx, &m)
        .unwrap()
        .remove(&old.id)
        .unwrap();
    crate::consolidation::revoke(&tx, &s, "user:maker", AT).unwrap();
    let p = profile("synthetic").normalize(&m).unwrap();
    let proposed = crate::consolidation::propose(&tx, &m, &p, "user:maker", AT).unwrap();
    let s = crate::consolidation::states(&tx, &m)
        .unwrap()
        .remove(&p.id)
        .unwrap();
    crate::consolidation::approve(
        &tx,
        &s,
        proposed["profileHash"].as_str().unwrap(),
        "user:reviewer",
        AT,
    )
    .unwrap();
    let hash = crate::intercompany::current(&tx, &reg).unwrap().unwrap().1;
    m.groups.retain(|g| g.id == "synthetic");
    crate::intercompany::apply_manifest(&tx, &m, Some(&hash), "user:historical-owner", AT).unwrap();
    let normal:Mapping=serde_json::from_value(json!({"id":"normal","groupId":"synthetic","leftCompanySlug":"a","rightCompanySlug":"b","leftAccountNos":["1000"],"rightAccountNos":["2000"],"leftPosition":"receivable","rightPosition":"payable","evidenceRefs":["synthetic"],"validFrom":"2000-01-01"})).unwrap();
    let normal = normal.normalize(&m).unwrap();
    for (kind, actor) in [("proposed", "user:maker"), ("approved", "user:reviewer")] {
        crate::intercompany::append(
            &tx,
            &crate::intercompany::states(&tx, &m).unwrap(),
            &normal,
            kind,
            actor,
            AT,
        )
        .unwrap();
    }
    for (id, version) in [("historical-v1", 1), ("historical-v2", 2)] {
        let p = payload(id, &normal, version, "2026-01-01");
        for (kind, actor) in [
            ("proposed", "user:maker"),
            ("approved", "user:reviewer"),
            ("applied", "user:reviewer"),
            ("reversed", "user:maker"),
        ] {
            crate::eliminations::append(&tx, &p, kind, actor, AT).unwrap();
        }
    }
    crate::intercompany::append(
        &tx,
        &crate::intercompany::states(&tx, &m).unwrap(),
        &normal,
        "revoked",
        "user:reviewer",
        AT,
    )
    .unwrap();
    let mut reverse = normal.clone();
    reverse.id = "reverse".into();
    std::mem::swap(
        &mut reverse.left_company_slug,
        &mut reverse.right_company_slug,
    );
    std::mem::swap(
        &mut reverse.left_account_nos,
        &mut reverse.right_account_nos,
    );
    std::mem::swap(&mut reverse.left_position, &mut reverse.right_position);
    for (kind, actor) in [("proposed", "user:maker"), ("approved", "user:reviewer")] {
        crate::intercompany::append(
            &tx,
            &crate::intercompany::states(&tx, &m).unwrap(),
            &reverse,
            kind,
            actor,
            AT,
        )
        .unwrap();
    }
    for (id, version, date) in [
        ("current-v2", 2, "2026-01-01"),
        ("unsafe-original-v1", 1, "2026-01-02"),
    ] {
        let p = payload(id, &reverse, version, date);
        for (kind, actor) in [
            ("proposed", "user:maker"),
            ("approved", "user:reviewer"),
            ("applied", "user:reviewer"),
        ] {
            crate::eliminations::append(&tx, &p, kind, actor, AT).unwrap();
        }
    }
    let mut pending = payload("pending", &reverse, 2, "2026-01-03");
    crate::eliminations::append(&tx, &pending, "proposed", "user:maker", AT).unwrap();
    pending.id = "rejected".into();
    crate::eliminations::append(&tx, &pending, "proposed", "user:maker", AT).unwrap();
    crate::eliminations::append(&tx, &pending, "rejected", "user:reviewer", AT).unwrap();
    tx.commit().unwrap();
    c
}
fn exported(c: &mut Connection) -> (Vec<u8>, Vec<u8>) {
    let tx = c
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    let b = export(&tx).unwrap();
    let source = super::super::intercompany::export(&tx).unwrap();
    (source, b)
}
fn target(source: &[u8]) -> Connection {
    let mut c = db();
    super::super::intercompany::initialize(&mut c, source, &["a".into(), "b".into()]).unwrap();
    c
}
#[test]
fn all_history_versions_statuses_dates_and_original_actors_roundtrip_exactly() {
    let mut c = fixture();
    let (source, bytes) = exported(&mut c);
    let mut restored = target(&source);
    initialize(&mut restored, &bytes, &["a".into(), "b".into()]).unwrap();
    assert_eq!(exported(&mut restored).1, bytes);
    let tx = restored.transaction().unwrap();
    let states = crate::eliminations::states(&tx).unwrap();
    assert_eq!(states["historical-v1"].status, "reversed");
    assert_eq!(states["historical-v1"].payload.version, 1);
    assert_eq!(states["current-v2"].actor, "user:reviewer");
    assert_eq!(states["pending"].status, "proposed");
    assert_eq!(states["rejected"].status, "rejected");
    assert_eq!(
        crate::eliminations::applied(&tx, "2026-01-01").unwrap()[0].elimination_id,
        "current-v2"
    );
    assert_eq!(
        crate::eliminations::applied(&tx, "2026-01-02").unwrap()[0]
            .payload
            .version,
        1
    );
    assert!(
        crate::eliminations::applied(&tx, "2026-01-03")
            .unwrap()
            .is_empty()
    );
    for t in [
        "user",
        "session",
        "twoFactor",
        "apikey",
        "rm_company_membership_events",
        "rm_workspace_user_access_events",
    ] {
        assert_eq!(
            tx.query_row(&format!("SELECT count(*) FROM {t}"), [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
}
#[test]
fn corrupt_shape_canonical_hash_chain_and_version_roll_back_both_tables() {
    let mut c = fixture();
    let (source, bytes) = exported(&mut c);
    let base: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    for case in 0..5 {
        let mut bad = base.clone();
        match case {
            0 => bad["format"] = json!("unknown"),
            1 => bad["tables"][TABLES[0]]["columns"][0] = json!("unknown"),
            2 => {
                bad["tables"][TABLES[1]]["rows"][0][7] =
                    json!({"type":"Text","value":"user:changed"})
            }
            3 => bad["tables"][TABLES[0]]["rows"][1][5] = json!({"type":"Null"}),
            _ => bad["tables"][TABLES[1]]["rows"][0][0] = json!({"type":"Text","value":"1"}),
        }
        let mut target = target(&source);
        assert!(
            initialize(
                &mut target,
                &serde_json::to_vec(&bad).unwrap(),
                &["a".into(), "b".into()]
            )
            .is_err()
        );
        for t in TABLES {
            assert_eq!(
                target
                    .query_row(&format!("SELECT count(*) FROM {t}"), [], |r| r
                        .get::<_, i64>(0))
                    .unwrap(),
                0
            );
        }
    }
}
#[test]
fn missing_history_company_or_approved_mapping_refuses_and_overwrite_never_occurs() {
    let mut c = fixture();
    let (source, bytes) = exported(&mut c);
    let mut t = target(&source);
    assert!(initialize(&mut t, &bytes, &["a".into()]).is_err());
    initialize(&mut t, &bytes, &["a".into(), "b".into()]).unwrap();
    assert!(initialize(&mut t, &bytes, &["a".into(), "b".into()]).is_err());
    let mut empty = db();
    assert!(initialize(&mut empty, &bytes, &["a".into(), "b".into()]).is_err());
    let tx = c
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    assert_eq!(
        export_control(&tx).unwrap().unsupported,
        vec![("intercompany", 0), ("consolidation", 0)]
    );
}
