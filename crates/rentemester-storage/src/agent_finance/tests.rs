use super::*;
use rusqlite::{Connection, TransactionBehavior};
fn policy(asset: bool) -> Policy {
    serde_json::from_value(json!({"supplierRules":[{"token":"synthetic","expenseAccount":"X","vatTreatment":"standard","label":"Supplies","assetLike":asset}]})).unwrap()
}
fn setup(tx: &Transaction<'_>, gross: i64, text: &str) {
    tx.execute_batch("UPDATE companies SET vat_period_type='quarter' WHERE id=1;INSERT INTO accounts(account_no,name,type,normal_balance) VALUES('A','Synthetic bank','asset','debit'),('X','Synthetic expense','expense','debit'),('V','Synthetic VAT','vat','debit');INSERT INTO account_role_mappings(role,account_no,status,version,confirmed_by)VALUES('input_vat','V','confirmed',1,'synthetic');INSERT INTO bank_accounts(slug,name,ledger_account_no)VALUES('synthetic','Synthetic bank','A');").unwrap();
    let metadata=serde_json::from_value(json!({"source":"synthetic","documentType":"purchase_sale","issueDate":"2026-10-18","deliveryDescription":"Synthetic supplies","invoiceNo":"SYN-1","amountIncVat":gross,"vatAmount":gross/5,"sender":{"name":"Synthetic supplier","address":"Synthetic1","vatOrCvr":"DK12345678"},"recipient":{"name":"Synthetic buyer","address":"Synthetic2","vatOrCvr":"DK87654321"}})).unwrap();
    crate::documents::register(
        tx,
        crate::documents::Original {
            filename: "synthetic.txt",
            hash: &"a".repeat(64),
            path: "synthetic-original",
            mime: "text/plain",
        },
        &metadata,
        "synthetic",
        "2026-10-18",
    )
    .unwrap();
    let csv = format!("date;text;amount\n2026-10-18;{text};-{gross}\n");
    let rows = rentemester_engine::bank::parse(&csv).unwrap();
    crate::bank::import(
        tx,
        &csv,
        &rows,
        Some("synthetic"),
        "synthetic",
        "2026-10-18T12:00:00Z",
        "synthetic-original",
    )
    .unwrap();
}
fn route(steps: Vec<ExpenseStep>) -> String {
    match steps.into_iter().next().unwrap() {
        ExpenseStep::Route(p) => p.kind,
        _ => panic!("expected route"),
    }
}
#[test]
fn agent_finance_policy_confidence_and_fixed_asset() {
    for (gross, text, expected) in [
        (125, "Synthetic supplier SYN-1", "AGENT_NO_ACCOUNT_RULE"),
        (
            5000,
            "Synthetic supplier SYN-1",
            "AGENT_POSSIBLE_FIXED_ASSET",
        ),
    ] {
        let mut c = Connection::open_in_memory().unwrap();
        let mut db = Connection::open_in_memory().unwrap();
        crate::fresh::initialize_empty_pair(&mut c, &mut db, "Synthetic buyer").unwrap();
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        setup(&tx, gross, text);
        let p = if expected == "AGENT_NO_ACCOUNT_RULE" {
            Policy::default()
        } else {
            policy(true)
        };
        assert_eq!(
            route(plan_expenses(&tx, &p, "2026-10-18").unwrap()),
            expected
        );
        assert_eq!(
            tx.query_row::<i64, _, _>("SELECT count(*) FROM journal_entries", [], |r| r.get(0))
                .unwrap(),
            0
        );
        let mut ambiguous = policy(false);
        ambiguous
            .supplier_rules
            .push(ambiguous.supplier_rules[0].clone());
        if expected != "AGENT_LOW_CONFIDENCE_MATCH" {
            assert_eq!(
                route(plan_expenses(&tx, &ambiguous, "2026-10-18").unwrap()),
                "AGENT_NO_ACCOUNT_RULE"
            );
        }
    }
}
#[test]
fn agent_finance_positive_shared_writer_reread_repeat_and_service_identity() {
    let mut c = Connection::open_in_memory().unwrap();
    let mut db = Connection::open_in_memory().unwrap();
    crate::fresh::initialize_empty_pair(&mut c, &mut db, "Synthetic buyer").unwrap();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    setup(&tx, 125, "Synthetic supplier SYN-1");
    let steps = plan_expenses(&tx, &policy(false), "2026-10-18").unwrap();
    let ExpenseStep::Book { candidate, summary } = steps.into_iter().next().unwrap() else {
        panic!("expected book")
    };
    assert_eq!(
        Money::parse(&summary.amount.to_string()).unwrap(),
        Money::parse("-125").unwrap()
    );
    let result = crate::expense_original::book_agent(
        &tx,
        &candidate,
        "synthetic-service",
        crate::idempotency::PrincipalKind::ServiceAccount,
        Date::parse("2026-12-01").unwrap(),
        1792368000000,
    )
    .unwrap();
    assert_eq!(result["ok"], true);
    assert_eq!(
        tx.query_row::<String, _, _>("SELECT created_by FROM journal_entries", [], |r| r.get(0))
            .unwrap(),
        "service_account:synthetic-service"
    );
    assert_eq!(
        tx.query_row::<String, _, _>("SELECT created_by_program FROM journal_entries", [], |r| r
            .get(0))
            .unwrap(),
        PROGRAM
    );
    assert!(
        plan_expenses(&tx, &policy(false), "2026-10-18")
            .unwrap()
            .is_empty()
    );
    assert!(
        crate::expense_original::book(
            &tx,
            &candidate,
            "synthetic-service",
            crate::idempotency::PrincipalKind::ServiceAccount,
            Date::parse("2026-12-01").unwrap(),
            1792368000000
        )
        .is_err()
    );
}
#[test]
fn agent_finance_corroboration_never_accepts_amount_or_company_form() {
    assert!(!corroborates("1250 DKK ApS", "Unrelated ApS", ""));
    assert!(corroborates("Payment SYN-42", "Unrelated ApS", "SYN-42"));
    assert!(corroborates("Synthetic payment", "Synthetic ApS", ""));
    let mut p = policy(false);
    p.supplier_rules[0].vat_treatment = "invented".into();
    assert!(validate_policy(&p).is_err());
    assert!(serde_json::from_value::<Policy>(json!({"supplierRules":[{"token":"x","expenseAccount":"X","vatTreatment":"standard","label":"x"}]})).is_err());
}

#[test]
fn agent_finance_equal_confidence_chooses_first_canonical_document() {
    let rows = vec![
        json!({"kind":"purchase_sale","documentId":1,"confidence":0.9}),
        json!({"kind":"purchase_sale","documentId":2,"confidence":0.9}),
        json!({"kind":"purchase_sale","documentId":3,"confidence":0.8}),
    ];
    assert_eq!(best_purchase(&rows).unwrap()["documentId"], 1);
    let reversed = rows.into_iter().rev().collect::<Vec<_>>();
    assert_eq!(best_purchase(&reversed).unwrap()["documentId"], 1);
}
