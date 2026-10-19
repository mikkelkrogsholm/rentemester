use super::*;
use rentemester_engine::consolidation as engine;
use rentemester_storage::{consolidation as profiles, eliminations, intercompany};
use serde_json::json;
const DATE: &str = "2026-01-01";
const AT: &str = "2026-01-01T00:00:00.000Z";
fn report(control: &mut Connection, ledgers: &mut [Connection]) -> Value {
    let tx = control
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let reg = BTreeSet::from(["synthetic".into(), "synthetic-second".into()]);
    let (manifest, _) = intercompany::current(&tx, &reg).unwrap().unwrap();
    let s = profiles::states(&tx, &manifest)
        .unwrap()
        .remove("synthetic-report")
        .unwrap();
    let mapping = intercompany::states(&tx, &manifest)
        .unwrap()
        .remove("synthetic-reviewed")
        .unwrap();
    let mut models = vec![];
    let mut sides = vec![];
    for (index, slug) in ["synthetic", "synthetic-second"].iter().enumerate() {
        let lt = ledgers[index]
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        models.push(profiles::ledger(&lt, slug, DATE, DATE).unwrap());
        let (accounts, position) = if index == 0 {
            (
                &mapping.mapping.left_account_nos,
                &mapping.mapping.left_position,
            )
        } else {
            (
                &mapping.mapping.right_account_nos,
                &mapping.mapping.right_position,
            )
        };
        sides.push(intercompany::side(&lt, slug, accounts, position, DATE).unwrap());
    }
    let fresh = json!({"status":"matched","mappingId":mapping.mapping.id,"mappingHash":mapping.mapping_hash,"left":sides[0],"right":sides[1]});
    let applied = eliminations::applied(&tx, DATE)
        .unwrap()
        .into_iter()
        .map(|e| engine::Applied {
            elimination_id: e.elimination_id,
            payload_hash: e.payload_hash,
            payload: e.payload,
        })
        .collect::<Vec<_>>();
    engine::calculate(
        &s.profile,
        &s.profile_hash,
        DATE,
        DATE,
        &models,
        &applied,
        &[fresh],
        vec![],
        intercompany::sha,
    )
    .unwrap()
}
fn sign(
    context: artifact::Context,
    entries: Vec<artifact::Snapshot>,
) -> artifact::VerifiedArtifact {
    // Public RFC8032 fixture key, never operator material.
    let der = concat!(
        "3051020101300506032b657004220420",
        "9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60",
        "812100",
        "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a"
    );
    let der = (0..der.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&der[i..i + 2], 16).unwrap())
        .collect::<Vec<_>>();
    let key = artifact::SigningKey::from_pkcs8(&der).unwrap();
    let trust = artifact::TrustedPublicKey::from_bytes(&key.public_key_bytes()).unwrap();
    artifact::verify(&artifact::create(context, entries, &key).unwrap(), &trust).unwrap()
}
#[test]
fn signed_profile_and_actual_applied_elimination_report_survive_source_free_restore_reopen() {
    let old = intercompany_tests::signed_intercompany(false, false);
    let mut control = Connection::open_in_memory().unwrap();
    restore_business(&mut control, &old, &"a".repeat(64)).unwrap();
    let mut ledgers = vec![];
    for slug in ["synthetic", "synthetic-second"] {
        let entry = old
            .entries()
            .iter()
            .find(
                |e| matches!(e.kind(),artifact::EntryKind::CompanyLedger{company}if company==slug),
            )
            .unwrap();
        let mut db = Connection::open_in_memory().unwrap();
        db.deserialize_read_exact(MAIN_DB, entry.bytes(), entry.bytes().len(), false)
            .unwrap();
        db.execute_batch("INSERT INTO accounts(account_no,name,type,normal_balance) VALUES('1400','Synthetic asset','asset','debit'),('2400','Synthetic debt','liability','credit'),('8000','Synthetic equity','equity','credit')").unwrap();
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        let lines = if slug == "synthetic" {
            json!([{"accountNo":"1400","debitAmount":"12.34"},{"accountNo":"8000","creditAmount":"12.34"}])
        } else {
            json!([{"accountNo":"8000","debitAmount":"12.34"},{"accountNo":"2400","creditAmount":"12.34"}])
        };
        let input=serde_json::from_value(json!({"transactionDate":DATE,"text":"Synthetic consolidation source","currency":"DKK","lines":lines})).unwrap();
        let receipt = rentemester_storage::journal_post::post(
            &tx,
            &input,
            &rentemester_storage::journal_preview::manual_journal_rule_version().unwrap(),
            rentemester_engine::fiscal_year::Date::parse(DATE).unwrap(),
            rentemester_storage::journal_post::Actor {
                subject: "user:source",
                program: "synthetic-test",
            },
        )
        .unwrap();
        assert_eq!(receipt["ok"], true);
        tx.commit().unwrap();
        ledgers.push(db);
    }
    let tx = control
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let reg = BTreeSet::from(["synthetic".into(), "synthetic-second".into()]);
    let (manifest, _) = intercompany::current(&tx, &reg).unwrap().unwrap();
    let lines=[("assets","asset"),("debt","liability"),("equity","equity"),("result","equity"),("income","income"),("expense","expense")].iter().enumerate().map(|(n,(id,section))|json!({"id":id,"label":id,"section":section,"displayOrder":n,"role":if *id=="result"{Some("current-result")}else{None}})).collect::<Vec<_>>();
    let mappings=["synthetic","synthetic-second"].iter().enumerate().flat_map(|(n,company)|[("1400","assets"),("2400","debt"),("8000","equity")].into_iter().map(move|(account,line)|json!({"id":format!("m-{n}-{account}"),"companySlug":company,"accountNo":account,"reportingLineId":line,"validFrom":"2020-01-01"}))).collect::<Vec<_>>();
    let profile:engine::Profile=serde_json::from_value(json!({"version":1,"id":"synthetic-report","groupId":"synthetic-signed","currency":"DKK","reportingLines":lines,"accountMappings":mappings,"evidenceRefs":["synthetic-reviewed-chart"],"validFrom":"2020-01-01"})).unwrap();
    let profile = profile.normalize(&manifest).unwrap();
    let proposal = profiles::propose(&tx, &manifest, &profile, "user:old-maker", AT).unwrap();
    let state = profiles::states(&tx, &manifest)
        .unwrap()
        .remove(&profile.id)
        .unwrap();
    profiles::approve(
        &tx,
        &state,
        proposal["profileHash"].as_str().unwrap(),
        "user:old-reviewer",
        AT,
    )
    .unwrap();
    let mapping = intercompany::states(&tx, &manifest)
        .unwrap()
        .remove("synthetic-reviewed")
        .unwrap();
    let mut sides = vec![];
    for (index, slug) in ["synthetic", "synthetic-second"].iter().enumerate() {
        let lt = ledgers[index]
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        let (accounts, position) = if index == 0 {
            (
                &mapping.mapping.left_account_nos,
                &mapping.mapping.left_position,
            )
        } else {
            (
                &mapping.mapping.right_account_nos,
                &mapping.mapping.right_position,
            )
        };
        sides.push(intercompany::side(&lt, slug, accounts, position, DATE).unwrap());
    }
    let fresh = json!({"status":"matched","mappingId":mapping.mapping.id,"mappingHash":mapping.mapping_hash,"left":sides[0],"right":sides[1]});
    let input=serde_json::from_value(json!({"id":"synthetic-elimination","mappingId":mapping.mapping.id,"asOf":DATE,"evidenceRefs":["synthetic-source"]})).unwrap();
    let payload = rentemester_engine::eliminations::derive(
        &input,
        &mapping.mapping,
        &mapping.mapping_hash,
        &fresh,
    )
    .unwrap();
    for (kind, actor) in [
        ("proposed", "user:old-maker"),
        ("approved", "user:old-reviewer"),
        ("applied", "user:old-reviewer"),
    ] {
        eliminations::append(&tx, &payload, kind, actor, AT).unwrap();
    }
    tx.commit().unwrap();
    let before = report(&mut control, &mut ledgers);
    assert_eq!(before["status"], "ready");
    assert_eq!(before["consolidatedFigures"][0]["rawCompanySum"], "12.34");
    assert_eq!(
        before["consolidatedFigures"][0]["eliminationAdjustment"],
        "-12.34"
    );
    assert_eq!(
        before["consolidatedFigures"][0]["consolidatedAmount"],
        "0.00"
    );
    let tx = control
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let business = store::export_control(&tx).unwrap();
    tx.rollback().unwrap();
    let mut context = old.manifest().context.clone();
    context
        .coverage
        .unsupported
        .retain(|s| s.section != artifact::BusinessSection::Consolidation);
    context
        .coverage
        .included
        .push(artifact::BusinessSection::Consolidation);
    let mut entries = old
        .entries()
        .iter()
        .map(|e| artifact::Snapshot {
            kind: e.kind().clone(),
            path: e.path().into(),
            bytes: e.bytes().to_vec(),
        })
        .collect::<Vec<_>>();
    for (index, slug) in ["synthetic", "synthetic-second"].iter().enumerate() {
        entries
            .iter_mut()
            .find(|e| matches!(&e.kind,artifact::EntryKind::CompanyLedger{company}if company==slug))
            .unwrap()
            .bytes = snapshot(&ledgers[index]).unwrap();
    }
    entries
        .iter_mut()
        .find(|e| {
            matches!(
                e.kind,
                artifact::EntryKind::ControlBusiness {
                    section: artifact::BusinessSection::WorkspaceAudit
                }
            )
        })
        .unwrap()
        .bytes = business.bytes;
    entries.push(artifact::Snapshot {
        kind: artifact::EntryKind::ControlBusiness {
            section: artifact::BusinessSection::Consolidation,
        },
        path: "control/consolidation.json".into(),
        bytes: business.consolidation_bytes,
    });
    let archive = sign(
        context.clone(),
        entries
            .iter()
            .map(|e| artifact::Snapshot {
                kind: e.kind.clone(),
                path: e.path.clone(),
                bytes: e.bytes.clone(),
            })
            .collect(),
    );
    validate(&archive).unwrap();
    drop(control);
    drop(ledgers); // All subsequent report sources come from independently verified archive bytes.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("restored-control.sqlite");
    let mut restored = Connection::open(&path).unwrap();
    restore_business(&mut restored, &archive, &"b".repeat(64)).unwrap();
    restored.close().unwrap();
    let mut restored = Connection::open(path).unwrap();
    let mut restored_ledgers = vec![];
    for slug in ["synthetic", "synthetic-second"] {
        let e = archive
            .entries()
            .iter()
            .find(
                |e| matches!(e.kind(),artifact::EntryKind::CompanyLedger{company}if company==slug),
            )
            .unwrap();
        let mut db = Connection::open_in_memory().unwrap();
        db.deserialize_read_exact(MAIN_DB, e.bytes(), e.bytes().len(), false)
            .unwrap();
        restored_ledgers.push(db);
    }
    assert_eq!(report(&mut restored, &mut restored_ledgers), before);
    assert_eq!(
        restored
            .query_row("SELECT count(*) FROM user", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
    let section = entries
        .iter_mut()
        .find(|e| {
            matches!(
                e.kind,
                artifact::EntryKind::ControlBusiness {
                    section: artifact::BusinessSection::Consolidation
                }
            )
        })
        .unwrap();
    let mut bad: Value = serde_json::from_slice(&section.bytes).unwrap();
    bad["format"] = json!("unknown-consolidation");
    section.bytes = serde_json::to_vec(&bad).unwrap();
    assert!(
        validate(&sign(context, entries)).is_err(),
        "signature never excuses unknown native codec"
    );
}
