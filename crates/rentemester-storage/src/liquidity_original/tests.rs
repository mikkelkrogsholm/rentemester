use super::*;
use crate::{budget_original as budget, idempotency::PrincipalKind};
use rusqlite::Connection;
fn fixture() -> (Connection, Connection) {
    let mut control = Connection::open_in_memory().unwrap();
    let mut db = Connection::open_in_memory().unwrap();
    crate::fresh::initialize_empty_pair(&mut control, &mut db, "Synthetic source planning")
        .unwrap();
    db.execute_batch("INSERT INTO accounts(id,account_no,name,type,normal_balance) VALUES(1,'B','Synthetic cash','asset','debit'),(2,'I','Synthetic income','income','credit'),(3,'E','Synthetic expense','expense','debit');INSERT INTO account_role_mappings(role,account_no,version,confirmed_by) VALUES('bank','B',1,'synthetic');INSERT INTO journal_entries(id,entry_no,transaction_date,text,rule_version,entry_hash) VALUES(1,'S-1','2026-01-31','Synthetic opening','fixture','synthetic1'),(2,'S-2','2026-02-10','Synthetic cost','fixture','synthetic2');INSERT INTO journal_lines(journal_entry_id,account_id,debit_amount,credit_amount) VALUES(1,1,1000,0),(1,2,0,1000),(2,3,60,0),(2,1,0,60);").unwrap();
    (control, db)
}
fn set(tx: &Transaction<'_>, n: f64) -> Value {
    budget::set(
        tx,
        &serde_json::from_value(
            json!({"accountNo":" E ","period":" 2026-02 ","amount":n,"notes":"  "}),
        )
        .unwrap(),
        "synthetic",
        PrincipalKind::ServiceAccount,
    )
    .unwrap()
}
#[test]
fn source_latest_natural_signs_zero_refusals_typed_audit_and_no_read_effects() {
    let (mut c, mut db) = fixture();
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    let ct = c.transaction().unwrap();
    assert_eq!(set(&tx, 80.0)["amount"], 80.0);
    let latest = set(&tx, 90.0);
    assert_eq!(latest["ok"], true);
    let list = budget::list(&tx, None, None).unwrap();
    assert_eq!(list["count"], 1);
    assert_eq!(list["rows"][0]["id"], latest["budgetLineId"]);
    assert!(list["rows"][0]["notes"].is_null());
    assert!(list.get("appliedRules").is_none());
    let compare = budget::vs_actual(&tx, "2026-02", "2026-02").unwrap();
    assert_eq!(compare["lines"].as_array().unwrap().len(), 1);
    assert_eq!(compare["lines"][0]["actual"], 60.0);
    assert_eq!(compare["lines"][0]["variance"], 30.0);
    assert_eq!(compare["totalVariance"], -30.0);
    assert_eq!(
        budget::vs_actual(&tx, "2026-03", "2026-02").unwrap()["ok"],
        false
    );
    assert_eq!(
        budget::list(&tx, Some("2026-13"), None).unwrap()["ok"],
        false
    );
    let bad = serde_json::from_value(json!({"accountNo":"missing","period":"bad"})).unwrap();
    let failed = budget::set(&tx, &bad, "synthetic", PrincipalKind::User).unwrap();
    assert_eq!(failed["errors"].as_array().unwrap().len(), 3);
    let negative =
        serde_json::from_value(json!({"accountNo":"E","period":"2026-02","amount":-0.001}))
            .unwrap();
    assert_eq!(
        budget::set(&tx, &negative, "synthetic", PrincipalKind::User).unwrap()["ok"],
        false
    );
    for _ in 0..2 {
        let m = monthly(
            &tx,
            &Monthly {
                start_date: "2026-02-15".into(),
                months: "1e0".into(),
            },
        )
        .unwrap();
        assert_eq!(m["ok"], true);
        assert_eq!(m["startDate"], "2026-02-15");
        assert_eq!(m["openingBalance"], 1000.0);
        assert_eq!(m["closingBalance"], 910.0);
        let w = weekly(
            &ct,
            &tx,
            "synthetic",
            &Weekly {
                start_date: "2026-02-01".into(),
                weeks: Some("2.0".into()),
            },
        )
        .unwrap();
        assert_eq!(w["ok"], true);
        assert_eq!(w["periods"].as_array().unwrap().len(), 2);
        assert_eq!(w["periods"][0]["undatedBudgetAssumptions"], -90.0);
        assert_eq!(w["periods"][1]["undatedBudgetAssumptions"], 0.0);
        assert_eq!(w["periods"][0]["closingCash"], 1000.0);
        assert_eq!(w["periods"][0]["scenarioClosingCash"], 1000.0);
    }
    assert_eq!(tx.query_row("SELECT COUNT(*) FROM audit_log WHERE actor='service_account:synthetic' AND event_type='budget_set'",[],|r|r.get::<_,i64>(0)).unwrap(),2);
    assert_eq!(
        tx.query_row("SELECT COUNT(*) FROM budget_lines", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        2
    );
    assert_eq!(
        tx.query_row("SELECT COUNT(*) FROM journal_entries", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        2
    );
    assert_eq!(set(&tx, 0.0)["amount"], 0.0);
    assert_eq!(
        budget::list(&tx, None, None).unwrap()["rows"][0]["amount"],
        0.0
    );
}
#[test]
fn bounds_and_absent_opening_are_business_reports() {
    let (mut c, mut db) = fixture();
    db.execute("DELETE FROM account_role_mappings", []).unwrap();
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    let ct = c.transaction().unwrap();
    for raw in ["", "0", "-1", "1.5", "19", "NaN", "inf"] {
        let m = monthly(
            &tx,
            &Monthly {
                start_date: "2026-02-15".into(),
                months: raw.into(),
            },
        )
        .unwrap();
        assert_eq!(m["ok"], false);
        assert_eq!(m["appliedRules"][0], "liquidity-forecast");
    }
    for raw in ["", "0", "14", "1.5", "NaN"] {
        let w = weekly(
            &ct,
            &tx,
            "synthetic",
            &Weekly {
                start_date: "2026-02-01".into(),
                weeks: Some(raw.into()),
            },
        )
        .unwrap();
        assert_eq!(w["ok"], false);
        assert_eq!(w["appliedRules"][0], "liquidity-forecast-13-week-v1");
    }
    assert_eq!(
        monthly(
            &tx,
            &Monthly {
                start_date: "2026-02-15".into(),
                months: "1".into()
            }
        )
        .unwrap()["ok"],
        false
    );
    let w = weekly(
        &ct,
        &tx,
        "synthetic",
        &Weekly {
            start_date: "2026-02-01".into(),
            weeks: None,
        },
    )
    .unwrap();
    assert_eq!(w["ok"], false);
    assert_eq!(w["appliedRules"][0], "liquidity-forecast-13-week-v3");
}

#[test]
fn ungenerated_schedule_due_month_uses_gross_and_skips_materialized_periods() {
    let (_, mut db) = fixture();
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    // Immutable synthetic source specification. The ordinary original template
    // validator and writer establish the frozen gross and schedule.
    let payload = json!({"invoiceType":"full","invoiceNumber":"SYN-1","issueDate":"2026-01-31","dueDate":"2026-02-01","currency":"DKK","seller":{"name":"Synthetic Seller","address":"Synthetic Road 1","vatOrCvr":"12345678"},"buyer":{"name":"Synthetic Buyer","address":"Synthetic Road 2"},"lines":[{"description":"Synthetic service","quantity":1,"unitPriceExVat":40,"lineTotalExVat":40}],"totals":{"netAmount":40,"vatRate":0.25,"vatAmount":10,"grossAmount":50}});
    let input=serde_json::from_value(json!({"name":"Synthetic monthly schedule","interval":"monthly","firstIssueDate":"2026-01-31","paymentTermsDays":1,"invoice":payload,"confirm":true})).unwrap();
    let created =
        crate::recurring_invoices::original::create(&tx, &input, "synthetic", PrincipalKind::User)
            .unwrap();
    assert_eq!(created["ok"], true, "{created}");
    set(&tx, 90.0);
    let m = monthly(
        &tx,
        &Monthly {
            start_date: "2026-02-15".into(),
            months: "2".into(),
        },
    )
    .unwrap();
    assert_eq!(m["ok"], true, "{m}");
    assert_eq!(m["periods"][0]["recurringInflow"], 50.0);
    assert_eq!(m["periods"][0]["closingBalance"], 960.0);
    assert_eq!(m["periods"][1]["recurringInflow"], 50.0);
    assert_eq!(m["closingBalance"], 1010.0);
    assert_eq!(
        tx.query_row(
            "SELECT COUNT(*) FROM recurring_invoice_generations",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    assert_eq!(
        tx.query_row("SELECT COUNT(*) FROM documents", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
}

#[test]
fn monthly_foreign_recurring_inflow_is_not_added_to_dkk_cash() {
    let (_, mut db) = fixture();
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    let payload = json!({"invoiceType":"full","invoiceNumber":"SYN-EUR","issueDate":"2026-02-01","dueDate":"2026-02-01","currency":"EUR","seller":{"name":"Synthetic Seller","address":"Synthetic Road 1","vatOrCvr":"12345678"},"buyer":{"name":"Synthetic Buyer","address":"Synthetic Road 2"},"lines":[{"description":"Synthetic service","quantity":1,"unitPriceExVat":40,"lineTotalExVat":40}],"totals":{"netAmount":40,"vatRate":0.25,"vatAmount":10,"grossAmount":50}});
    // Historical imported schedule in its original currency; no future FX fact.
    tx.execute("INSERT INTO recurring_invoice_templates(name,interval,interval_count,first_issue_date,next_issue_date,payment_terms_days,delivery_channel,payload_json) VALUES('Synthetic EUR schedule','monthly',1,'2026-02-01','2026-02-01',0,'manual',?1)",[payload.to_string()]).unwrap();
    let before = tx.total_changes();
    let report = monthly(
        &tx,
        &Monthly {
            start_date: "2026-02-15".into(),
            months: "1".into(),
        },
    )
    .unwrap();
    assert_eq!(report["ok"], false);
    assert_eq!(report["periods"], json!([]));
    assert!(report["errors"][0].as_str().unwrap().contains("dated FX"));
    assert_eq!(tx.total_changes(), before);
}
