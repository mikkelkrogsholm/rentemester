use super::*;
use rusqlite::{Connection, TransactionBehavior};
fn fixture() -> Connection {
    let mut control = Connection::open_in_memory().unwrap();
    let mut db = Connection::open_in_memory().unwrap();
    crate::fresh::initialize_empty_pair(&mut control, &mut db, "Synthetic commitments").unwrap();
    db.execute_batch("INSERT INTO vendors(id,name) VALUES(1,'Synthetic Supplier');INSERT INTO accounts(account_no,name,type,normal_balance) VALUES('B','Synthetic bank','asset','debit'),('E','Synthetic expenses','expense','debit'),('I','Synthetic income','income','credit');INSERT INTO account_role_mappings(role,account_no,version,confirmed_by) VALUES('bank','B',1,'synthetic');").unwrap();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let meta=serde_json::from_value(json!({"source":"Synthetic contract","documentType":"cash_register_receipt","issueDate":"2026-01-31","amountIncVat":10.01,"currency":"DKK"})).unwrap();
    crate::documents::register(
        &tx,
        crate::documents::Original {
            filename: "synthetic.txt",
            hash: &"a".repeat(64),
            path: "documents/synthetic.txt",
            mime: "text/plain",
        },
        &meta,
        "synthetic",
        "2026-01-31",
    )
    .unwrap();
    tx.commit().unwrap();
    db
}
fn input() -> Input {
    serde_json::from_value(json!({"commitmentId":"synthetic-contract","vendorPartyId":"vendor:1","type":"subscription","description":"Synthetic service","businessPurpose":"Synthetic work","amount":10.01,"currency":"DKK","frequency":"monthly","nextDate":"2026-01-31","evidenceRefs":["document:1"]})).unwrap()
}
fn apply_input(i: &Input, p: &Value) -> Apply {
    serde_json::from_value(json!({"commitment":{"commitmentId":i.commitment_id,"vendorPartyId":i.vendor_party_id,"type":i.r#type,"description":i.description,"businessPurpose":i.business_purpose,"amount":i.amount,"currency":i.currency,"frequency":i.frequency,"nextDate":i.next_date,"endDate":i.end_date,"evidenceRefs":i.evidence_refs},"payloadHash":p["payloadHash"],"confirm":true,"idempotencyKey":"synthetic-approved-key"})).unwrap()
}
#[test]
fn monthly_dates_preserve_anchor() {
    assert_eq!(
        occurrence("2026-01-31", "monthly", 1).unwrap(),
        "2026-02-28"
    );
    assert_eq!(
        occurrence("2026-01-31", "monthly", 2).unwrap(),
        "2026-03-31"
    );
    assert_eq!(occurrence("2024-02-29", "yearly", 4).unwrap(), "2028-02-29");
}
#[test]
fn reviewed_plan_exact_schedule_idempotency_lifecycle_and_append_only_history() {
    let mut db = fixture();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let i = input();
    let p = plan(&tx, &i).unwrap();
    assert_eq!(p["schedule"][1]["date"], "2026-02-28");
    assert_eq!(p["schedule"][2]["date"], "2026-03-31");
    assert_eq!(p["vendorSnapshot"], "Synthetic Supplier");
    let mut apply_i = apply_input(&i, &p);
    apply_i.confirm = false;
    assert!(apply(&tx, &apply_i, "synthetic", "2026-01-01T00:00:00Z").is_err());
    apply_i.confirm = true;
    tx.execute("UPDATE vendors SET name='Changed supplier' WHERE id=1", [])
        .unwrap();
    assert!(apply(&tx, &apply_i, "synthetic", "2026-01-01T00:00:00Z").is_err());
    tx.execute(
        "UPDATE vendors SET name='Synthetic Supplier' WHERE id=1",
        [],
    )
    .unwrap();
    apply(&tx, &apply_i, "synthetic", "2026-01-01T00:00:00Z").unwrap();
    assert_eq!(
        apply(&tx, &apply_i, "synthetic", "2026-01-01T00:00:00Z").unwrap()["idempotent"],
        true
    );
    let mut conflicting = apply_input(&i, &p);
    conflicting.commitment.amount = Number::from(999);
    assert!(apply(&tx, &conflicting, "synthetic", "now").is_err());
    let result = list(&tx, "synthetic", "2026-01-31").unwrap();
    assert_eq!(result["forecast"]["periods"].as_array().unwrap().len(), 13);
    let total = result["forecast"]["periods"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| value_cents(&p["commitments"]))
        .sum::<i64>();
    assert_eq!(total, 4004);
    assert_eq!(result["forecast"]["periods"][0]["closingCash"], -10.01);
    let c = |action: &str, reason: &str, h: &str| Change {
        commitment_id: i.commitment_id.clone(),
        action: action.into(),
        reason: reason.into(),
        expected_payload_hash: h.into(),
        confirm: true,
    };
    assert!(
        change(
            &tx,
            &c("paused", " ", p["payloadHash"].as_str().unwrap()),
            "synthetic",
            "now"
        )
        .is_err()
    );
    change(
        &tx,
        &c(
            "paused",
            "Synthetic pause",
            p["payloadHash"].as_str().unwrap(),
        ),
        "synthetic",
        "now",
    )
    .unwrap();
    let paused = list(&tx, "synthetic", "2026-01-31").unwrap();
    assert_eq!(paused["commitments"][0]["status"], "paused");
    assert_eq!(paused["occurrences"], json!([]));
    assert_eq!(paused["history"].as_array().unwrap().len(), 2);
    change(
        &tx,
        &c(
            "ended",
            "Synthetic end",
            paused["commitments"][0]["payloadHash"].as_str().unwrap(),
        ),
        "synthetic",
        "now",
    )
    .unwrap();
    assert!(plan(&tx, &i).is_err());
    assert_eq!(
        tx.query_row("SELECT COUNT(*) FROM journal_entries", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        tx.query_row("SELECT COUNT(*) FROM payables", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert!(
        tx.execute("DELETE FROM supplier_commitment_events", [])
            .is_err()
    );
    tx.commit().unwrap();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    assert_eq!(
        list(&tx, "synthetic", "2026-01-31").unwrap()["commitments"][0]["status"],
        "ended"
    );
}
fn value_cents(v: &Value) -> i64 {
    Money::parse(&v.to_string())
        .unwrap()
        .decimal()
        .replace('.', "")
        .parse()
        .unwrap()
}
#[test]
fn match_canonical_values_once_excludes_expected_forecast_without_posting() {
    let mut db = fixture();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let i = input();
    let p = plan(&tx, &i).unwrap();
    apply(&tx, &apply_input(&i, &p), "synthetic", "now").unwrap();
    let m:Match=serde_json::from_value(json!({"commitmentId":i.commitment_id,"occurrenceDate":"2026-01-31","evidence":{"kind":"canonical_document","id":"1"},"confirm":true})).unwrap();
    let matched = match_occurrence(&tx, &m, "synthetic", "now").unwrap();
    assert_eq!(matched["variance"]["amount"], 0.0);
    assert_eq!(matched["variance"]["party"]["status"], "missing");
    assert_eq!(
        match_occurrence(&tx, &m, "synthetic", "now").unwrap()["idempotent"],
        true
    );
    let conflicting:Match=serde_json::from_value(json!({"commitmentId":i.commitment_id,"occurrenceDate":"2026-02-28","evidence":{"kind":"canonical_document","id":"1"},"confirm":true})).unwrap();
    assert!(match_occurrence(&tx, &conflicting, "synthetic", "now").is_err());
    let report = list(&tx, "synthetic", "2026-01-31").unwrap();
    assert_eq!(report["occurrences"].as_array().unwrap().len(), 3);
    assert_eq!(report["forecast"]["periods"][0]["commitments"], 0.0);
    assert_eq!(
        tx.query_row("SELECT COUNT(*) FROM journal_entries", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert!(serde_json::from_value::<Match>(json!({"commitmentId":"synthetic-contract","occurrenceDate":"2026-01-31","evidence":{"kind":"canonical_document","id":"1","amount":1},"confirm":true})).is_err());
}
#[test]
fn outgoing_bank_options_and_matches_use_positive_cost_without_posting() {
    let mut db = fixture();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    tx.execute_batch("INSERT INTO bank_transactions(id,transaction_date,text,amount,transaction_hash) VALUES(1,'2026-01-31','Synthetic outgoing',-10.01,'synthetic-outgoing'),(2,'2026-01-31','Synthetic incoming',10.01,'synthetic-incoming'),(3,'2026-01-31','Synthetic integer outgoing',-12,'synthetic-integer-outgoing');").unwrap();
    let report = list(&tx, "synthetic", "2026-01-31").unwrap();
    let options: Vec<_> = report["evidenceOptions"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| v["kind"] == "bank_transaction")
        .collect();
    assert_eq!(options.len(), 2);
    assert_eq!(
        options.iter().find(|v| v["id"] == "1").unwrap()["amount"],
        10.01
    );
    assert_eq!(
        options.iter().find(|v| v["id"] == "3").unwrap()["amount"],
        12.0
    );
    let i = input();
    let p = plan(&tx, &i).unwrap();
    apply(&tx, &apply_input(&i, &p), "synthetic", "now").unwrap();
    let mut m: Match = serde_json::from_value(json!({"commitmentId":i.commitment_id,"occurrenceDate":"2026-01-31","evidence":{"kind":"bank_transaction","id":"2"},"confirm":true})).unwrap();
    assert!(match_occurrence(&tx, &m, "synthetic", "now").is_err());
    m.evidence.id = "1".into();
    let matched = match_occurrence(&tx, &m, "synthetic", "now").unwrap();
    assert_eq!(matched["variance"]["amount"], 0.0);
    assert_eq!(
        list(&tx, "synthetic", "2026-01-31").unwrap()["forecast"]["periods"][0]["commitments"],
        0.0
    );
    assert_eq!(
        tx.query_row("SELECT COUNT(*) FROM journal_entries", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        tx.query_row("SELECT amount FROM bank_transactions WHERE id=1", [], |r| r
            .get::<_, f64>(0))
            .unwrap(),
        -10.01
    );
}
#[test]
fn no_opening_mapping_is_null_foreign_money_excluded_budget_is_informational() {
    let mut db = fixture();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    tx.execute("DELETE FROM account_role_mappings", []).unwrap();
    let mut i = input();
    i.currency = "EUR".into();
    let p = plan(&tx, &i).unwrap();
    apply(&tx, &apply_input(&i, &p), "synthetic", "now").unwrap();
    let b: crate::budget::Set =
        serde_json::from_value(json!({"accountNo":"E","period":"2026-01","amount":123.45}))
            .unwrap();
    crate::budget::set(&tx, &b, "synthetic").unwrap();
    let report = list(&tx, "synthetic", "2026-01-31").unwrap();
    assert_eq!(report["forecast"]["openingCash"], Value::Null);
    assert_eq!(report["forecast"]["periods"][0]["closingCash"], Value::Null);
    assert_eq!(report["forecast"]["periods"][0]["commitments"], 0.0);
    assert_eq!(
        report["forecast"]["periods"][0]["undatedBudgetAssumptions"],
        -123.45
    );
    assert_eq!(
        report["forecast"]["periods"][0]["excluded"][0]["currency"],
        "EUR"
    );
    assert_eq!(report["forecast"]["completeness"]["status"], "incomplete");
}
#[test]
fn vat_open_estimate_closed_obligation_and_unknown_settlement_are_separate() {
    let mut db = fixture();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    tx.execute_batch("UPDATE companies SET vat_period_type='quarter';INSERT INTO accounts(account_no,name,type,normal_balance) VALUES('OV','Synthetic output VAT','vat','credit');INSERT INTO journal_entries(id,entry_no,transaction_date,text,rule_version,entry_hash) VALUES(1,'SYN-VAT','2025-12-15','Synthetic VAT basis','fixture','synthetic-vat-hash');INSERT INTO journal_lines(journal_entry_id,account_id,debit_amount,credit_amount,vat_code) VALUES(1,(SELECT id FROM accounts WHERE account_no='B'),125,0,NULL),(1,(SELECT id FROM accounts WHERE account_no='I'),0,100,'DK_SALE_25'),(1,(SELECT id FROM accounts WHERE account_no='OV'),0,25,NULL);").unwrap();
    let flow = forecast::build(&tx, "synthetic", "2026-02-01", &[]).unwrap();
    assert_eq!(flow["openingCash"], 125.0);
    let source = flow["periods"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|p| p["sources"].as_array().unwrap())
        .find(|s| s["reference"] == "vat:2025-10-01:2025-12-31")
        .unwrap();
    assert_eq!(source["source"], "scenarios");
    assert_eq!(source["amount"], -25.0);
    assert_eq!(source["assumption"], true);
    tx.execute("INSERT INTO accounting_periods(period_start,period_end,kind,status,closed_at,closed_by) VALUES('2025-10-01','2025-12-31','vat_period','closed','2026-01-01','user:synthetic')",[]).unwrap();
    let flow = forecast::build(&tx, "synthetic", "2026-02-01", &[]).unwrap();
    let source = flow["periods"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|p| p["sources"].as_array().unwrap())
        .find(|s| s["reference"] == "vat:2025-10-01:2025-12-31")
        .unwrap();
    assert_eq!(source["source"], "obligations");
    assert_eq!(source["settlementStatus"], "unknown");
    assert_eq!(flow["periods"][12]["closingCash"], 100.0);
}
#[test]
fn legacy_party_snapshots_latest_revision_and_unstored_variance_remain_readable() {
    let mut db = fixture();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    for (amount, h) in [(10.01, "b".repeat(64)), (20.02, "c".repeat(64))] {
        let p = json!({"commitmentId":"legacy-contract","vendorPartyId":"party-existing-workspace","vendorSnapshot":"Legacy reviewed supplier","type":"service","description":"Legacy source","businessPurpose":"Legacy reviewed work","amount":amount,"currency":"DKK","frequency":"monthly","nextDate":"2026-01-31","endDate":null,"evidenceRefs":["legacy-reviewed-reference"],"status":"active"});
        tx.execute("INSERT INTO supplier_commitment_events(commitment_id,event_type,payload_json,payload_hash,actor,principal,created_at) VALUES('legacy-contract','approved',?1,?2,'user:synthetic','synthetic','2026-01-01')",params![p.to_string(),h]).unwrap();
    }
    let payload = json!({"commitmentId":"legacy-contract","occurrenceDate":"2026-01-31","expected":{"date":"2026-01-31","amount":10.01,"currency":"DKK","payloadHash":"b".repeat(64)},"evidence":{"kind":"canonical_document","id":"1","date":"2026-01-31","amount":10.01,"currency":"DKK","partyId":null,"documented":true,"sourceHash":"a".repeat(64)}});
    tx.execute("INSERT INTO supplier_commitment_occurrence_matches(commitment_id,occurrence_date,expected_payload_hash,evidence_kind,evidence_id,canonical_payload,payload_hash,actor,principal,created_at) VALUES('legacy-contract','2026-01-31',?1,'canonical_document','1',?2,?3,'user:synthetic','synthetic','2026-01-31')",params!["b".repeat(64),payload.to_string(),"d".repeat(64)]).unwrap();
    let before = tx.total_changes();
    let read = list(&tx, "synthetic", "2026-01-31").unwrap();
    assert_eq!(tx.total_changes(), before);
    assert_eq!(read["commitments"].as_array().unwrap().len(), 1);
    assert_eq!(read["commitments"][0]["amount"], 20.02);
    assert_eq!(read["matches"][0]["variance"]["amount"], 0.0);
    assert_eq!(
        read["matches"][0]["variance"]["party"]["expected"],
        "party-existing-workspace"
    );
    assert_eq!(read["matches"][0]["payloadHash"], "d".repeat(64));
    assert_eq!(read["occurrences"].as_array().unwrap().len(), 3);
}

#[test]
fn original_projection_and_typed_provenance_reuse_native_guarded_lifecycle() {
    use crate::{idempotency::PrincipalKind, supplier_commitment_original as original};
    let mut db = fixture();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let mut raw = serde_json::to_value(input()).unwrap();
    raw["vendorSnapshot"] = json!("Synthetic Supplier");
    raw["status"] = json!("active");
    raw["noticeDate"] = json!("2026-01-31");
    raw["renewalDate"] = json!("2026-03-02");
    let source: original::Input = serde_json::from_value(raw.clone()).unwrap();
    let native = source.native().unwrap();
    let p = source.proposal(&tx).unwrap();
    assert_eq!(p, plan(&tx, &native).unwrap());
    raw["vendorSnapshot"] = json!("forged");
    assert!(
        serde_json::from_value::<original::Input>(raw.clone())
            .unwrap()
            .proposal(&tx)
            .is_err()
    );
    raw["status"] = json!("paused");
    assert!(
        serde_json::from_value::<original::Input>(raw)
            .unwrap()
            .native()
            .is_err()
    );
    let a = Apply {
        commitment: native,
        payload_hash: p["payloadHash"].as_str().unwrap().into(),
        confirm: true,
        idempotency_key: None,
    };
    apply_principal(
        &tx,
        &a,
        "synthetic-service",
        PrincipalKind::ServiceAccount,
        "2026-01-31T00:00:00Z",
    )
    .unwrap();
    let identity: (String, String) = tx
        .query_row(
            "SELECT actor,principal FROM supplier_commitment_events",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(
        identity,
        (
            "service_account:synthetic-service".into(),
            "service_account:synthetic-service".into()
        )
    );
    let rows = original::list(&tx).unwrap();
    assert_eq!(rows.len(), 1);
    assert!(rows[0]["payload_json"].is_string());
    let alerts = original::alerts(&tx, "2026-01-31").unwrap();
    assert_eq!(alerts.len(), 2);
    assert_eq!(alerts[0]["severity"], "due");
    assert_eq!(alerts[1]["date"], "2026-03-02");
    assert!(original::alerts(&tx, "invalid").unwrap().is_empty());
    change_current_principal(
        &tx,
        &a.commitment.commitment_id,
        "paused",
        "Synthetic reason",
        "synthetic-human",
        PrincipalKind::User,
        "2026-02-01T00:00:00Z",
    )
    .unwrap();
    assert!(original::list(&tx).unwrap().is_empty());
    assert!(original::alerts(&tx, "2026-01-31").unwrap().is_empty());
    assert!(
        change_current_principal(
            &tx,
            &a.commitment.commitment_id,
            "ended",
            "Synthetic reason",
            "synthetic-human",
            PrincipalKind::User,
            "now"
        )
        .is_err()
    );
    let count: i64 = tx
        .query_row("SELECT COUNT(*) FROM supplier_commitment_events", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(count, 2);
}
