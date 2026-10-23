use super::*;
use crate::purchase_rules::{
    self,
    tests::{approved_rule, fixture, replace_immutable},
};
const NOW: &str = "2026-10-23T12:00:00.000Z";
fn proposal() -> Value {
    json!({"confirm":true,"proposal":{"ruleId":"general","effectiveFrom":"2000-01-01","conditions":{"currency":"DKK","supplierIdentity":"Synthetic supplier","documentType":"purchase_sale","supplierCountry":"DK","supplierVat":"DK12345678","vat":"positive","amount":{"min":100,"max":150},"reverseChargeWording":false,"attributes":{"bankText":"Synthetic supplier debit","source.kind":"original"}},"outcome":{"account":"6000","vatTreatment":"standard","textTemplate":"Exact source allocation"},"provenance":"synthetic","rationale":"Independent general review"}})
}
fn context() -> Value {
    json!({"context":{"documentId":1,"currency":"DKK","supplierIdentity":"Synthetic supplier","documentType":"purchase_sale","supplierCountry":"DK","supplierVat":"DK12345678","vatAmount":25,"amount":125,"reverseChargeWording":false,"attributes":{"bankText":"Synthetic supplier debit","source.kind":"original"}}})
}
fn call(tx: &Transaction<'_>, op: Operation, v: &Value, actor: &str) -> Value {
    execute(tx, op, v, actor, NOW).unwrap()
}
fn transition(hash: &Value) -> Value {
    json!({"confirm":true,"ruleId":"general","version":1,"expectedPayloadHash":hash,"rationale":"Independent review","provenance":"synthetic"})
}
#[test]
fn general_journey_duplicate_independent_exact_lifecycle_dryrun_and_backup() {
    let mut db = fixture();
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    let p = proposal();
    let created = call(&tx, Operation::Propose, &p, "service_account:author");
    assert_eq!(created["ok"], true, "{created}");
    assert_eq!(created["version"], 1);
    let changes = tx.total_changes();
    assert_eq!(
        call(&tx, Operation::Propose, &p, "user:other")["duplicate"],
        true
    );
    assert_eq!(tx.total_changes(), changes);
    let t = transition(&created["payloadHash"]);
    assert_eq!(
        call(&tx, Operation::Approve, &t, "service_account:author")["ok"],
        false
    );
    let mut wrong = t.clone();
    wrong["expectedPayloadHash"] = json!("0".repeat(64));
    assert_eq!(
        call(&tx, Operation::Approve, &wrong, "user:reviewer")["ok"],
        false
    );
    assert_eq!(
        call(&tx, Operation::Approve, &t, "user:reviewer")["ok"],
        true
    );
    let before = tx.total_changes();
    let c = context();
    let result = call(&tx, Operation::Explain, &c, "user:reader");
    assert_eq!(result["decision"], "proposed", "{result}");
    assert!(result.get("ok").is_none());
    assert_eq!(result["payloadHash"], created["payloadHash"]);
    assert_eq!(call(&tx, Operation::Test, &c, "user:reader"), result);
    for flag in [
        "changedBuyer",
        "changedSupplier",
        "changedVat",
        "changedCurrency",
        "unusualAmount",
        "contradictoryEvidence",
    ] {
        let mut v = c.clone();
        v["context"][flag] = json!(true);
        let r = call(&tx, Operation::Test, &v, "user:reader");
        assert_eq!(r["decision"], "human_decision");
        assert!(r.get("outcome").is_none());
    }
    for amount in [99, 151] {
        let mut v = c.clone();
        v["context"]["amount"] = json!(amount);
        assert_eq!(
            call(&tx, Operation::Test, &v, "user:reader")["decision"],
            "human_decision"
        );
    }
    for amount in [100, 150] {
        let mut v = c.clone();
        v["context"]["amount"] = json!(amount);
        assert_eq!(
            call(&tx, Operation::Test, &v, "user:reader")["decision"],
            "proposed"
        );
    }
    let mut missing = c.clone();
    missing["context"].as_object_mut().unwrap().remove("amount");
    assert_eq!(
        call(&tx, Operation::Test, &missing, "user:reader")["decision"],
        "human_decision"
    );
    assert_eq!(before, tx.total_changes());
    // Even another eligible native rule cannot provide fallback around broad source rules.
    approved_rule(&tx, "native", true);
    let purchase = purchase_rules::inspect(&tx, 1, 1, "2099-01-01").unwrap();
    assert_eq!(purchase["decision"], "human_decision", "{purchase}");
    assert!(
        purchase_rules::apply(&tx, 1, 1, "2099-01-01", &purchase, "blocked", "user:poster")
            .is_err()
    );
    let mut early = t.clone();
    early["effectiveAt"] = json!("2000-01-01");
    assert_eq!(
        call(&tx, Operation::Disable, &early, "user:reviewer")["ok"],
        false
    );
    assert_eq!(
        call(&tx, Operation::Disable, &t, "user:reviewer")["ok"],
        true
    );
    assert_eq!(
        call(&tx, Operation::Disable, &t, "user:reviewer")["ok"],
        false
    );
    let retired = call(&tx, Operation::Explain, &c, "user:reader");
    assert!(
        retired["explanations"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["ruleId"] == "general" && e["state"] == "disabled" && e["matched"] == false)
    );
    let mut p2 = p.clone();
    p2["proposal"]["rationale"] = json!("Separate second version");
    let v2 = call(&tx, Operation::Propose, &p2, "user:author");
    assert_eq!(v2["version"], 2);
    let mut t2 = transition(&v2["payloadHash"]);
    t2["version"] = json!(2);
    assert_eq!(
        call(&tx, Operation::Approve, &t2, "user:reviewer")["ok"],
        true
    );
    assert_eq!(
        call(&tx, Operation::Supersede, &t2, "user:reviewer")["ok"],
        true
    );
    for table in ["journal_entries", "posting_rule_applications", "exceptions"] {
        assert_eq!(
            tx.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
    tx.commit().unwrap();
    purchase_rules::validate_backup(&db).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("rules.sqlite");
    db.execute("VACUUM INTO ?1", [path.to_str().unwrap()])
        .unwrap();
    drop(db);
    let db = rusqlite::Connection::open(path).unwrap();
    purchase_rules::validate_backup(&db).unwrap();
    replace_immutable(
        &db,
        "posting_rule_versions",
        "UPDATE posting_rule_versions SET payload_hash=printf('%064d',0) WHERE rule_id='general' AND version=1",
    );
    assert!(purchase_rules::validate_backup(&db).is_err());
}
#[test]
fn recursive_input_dates_money_and_audit_fields_refuse_without_effects() {
    let mut db = fixture();
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    for (key, value) in [
        ("createdAt", json!(NOW)),
        ("companyId", json!(1)),
        ("effectiveFrom", json!("2026-02-30")),
        ("effectiveTo", json!("1999-01-01")),
    ] {
        let mut p = proposal();
        p["proposal"][key] = value;
        assert_eq!(
            call(&tx, Operation::Propose, &p, "user:author")["ok"],
            false
        );
    }
    for value in [json!(0.001), json!(1.234), json!({"x":1}), json!("125")] {
        let mut p = proposal();
        p["proposal"]["conditions"]["amount"]["min"] = value;
        assert_eq!(
            call(&tx, Operation::Propose, &p, "user:author")["ok"],
            false
        );
    }
    for value in [
        json!({"company":1}),
        json!({"currency":"dkk"}),
        json!({"attributes":{"bad key":"value"}}),
    ] {
        let mut p = proposal();
        p["proposal"]["conditions"] = value;
        assert_eq!(
            call(&tx, Operation::Propose, &p, "user:author")["ok"],
            false
        );
    }
    assert_eq!(
        tx.query_row("SELECT count(*) FROM posting_rule_versions", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(cents(&json!(125)).unwrap().decimal(), "125.00");
    assert_eq!(cents(&json!(1.25)).unwrap().decimal(), "1.25");
}
#[test]
fn broad_lifecycle_corruption_refuses_read_and_backup() {
    let mut db = fixture();
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    let p = call(&tx, Operation::Propose, &proposal(), "user:author");
    assert_eq!(
        call(
            &tx,
            Operation::Approve,
            &transition(&p["payloadHash"]),
            "user:reviewer"
        )["ok"],
        true
    );
    tx.commit().unwrap();
    replace_immutable(
        &db,
        "posting_rule_lifecycle_events",
        "UPDATE posting_rule_lifecycle_events SET actor='user:author',approver='user:author' WHERE state='approved'",
    );
    assert!(purchase_rules::validate_backup(&db).is_err());
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    assert_eq!(
        call(&tx, Operation::Explain, &context(), "user:reader")["ok"],
        false
    );
}
