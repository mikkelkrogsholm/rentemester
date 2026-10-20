use super::*;
use rusqlite::{Connection, TransactionBehavior};
fn database() -> (Connection, Connection) {
    let mut control = Connection::open_in_memory().unwrap();
    let mut company = Connection::open_in_memory().unwrap();
    crate::fresh::initialize_empty_pair(&mut control, &mut company, "Synthetic cashflow").unwrap();
    (control, company)
}
fn account(tx: &Transaction<'_>, name: &str, currency: &str) {
    let input = serde_json::from_value(json!({"name":name,"currency":currency})).unwrap();
    bank::create_account(tx, &input, "synthetic").unwrap();
}
fn import(tx: &Transaction<'_>, name: &str, csv: &str) {
    let rows = rentemester_engine::bank::parse(csv).unwrap();
    bank::import(
        tx,
        csv,
        &rows,
        Some(name),
        "synthetic",
        "2026-10-20T12:00:00Z",
        "synthetic-original",
    )
    .unwrap();
}
#[test]
fn bank_only_span_year_has_exact_flows_and_independent_multi_account_balances() {
    let (_control, mut db) = database();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    tx.execute("UPDATE companies SET fiscal_year_start_month=7,fiscal_year_label_strategy='span' WHERE id=1",[]).unwrap();
    account(&tx, "Alpha", "DKK");
    account(&tx, "Beta", "DKK");
    import(
        &tx,
        "alpha",
        "date;text;amount;currency;balance_after\n2025-06-30;Synthetic opening A;10;DKK;10\n2025-07-01;Synthetic cent in;0.01;DKK;10.01\n",
    );
    import(
        &tx,
        "beta",
        "date;text;amount;currency;balance_after\n2025-06-30;Synthetic opening B;20;DKK;20\n2025-07-01;Synthetic cents out;-0.02;DKK;19.98\n",
    );
    let before = tx.total_changes();
    let value = read(&tx, "synthetic", "2027-10-20", Some("2025/26")).unwrap();
    let report = &value["cashflow"];
    assert_eq!(report["periodStart"], "2025-07-01");
    assert_eq!(report["periodEnd"], "2026-06-30");
    assert_eq!(report["openingBalance"], 30.0);
    assert_eq!(report["closingBalance"], 29.99);
    assert_eq!(
        report["balanceSeries"],
        json!([{"date":"2025-07-01","balance":29.99}])
    );
    assert_eq!(report["months"][0]["netto"], -0.01);
    assert_eq!(report["months"][11]["label"], "2026-06");
    assert_eq!(report["status"], "complete");
    assert_eq!(report["coverage"]["transactionCount"], 2);
    assert_eq!(tx.total_changes(), before);
    assert!(read(&tx, "synthetic", "2027-10-20", Some("2025")).is_err());
    tx.commit().unwrap();
    let tx = db.transaction().unwrap();
    assert_eq!(
        read(&tx, "synthetic", "2027-10-20", Some("2025/26")).unwrap(),
        value
    );
    assert_eq!(
        tx.query_row::<i64, _, _>("SELECT count(*) FROM journal_entries", [], |r| r.get(0))
            .unwrap(),
        0
    );
}
#[test]
fn missing_statement_is_not_zero_but_explicit_zero_is_known() {
    let (_control, mut db) = database();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    account(&tx, "Missing", "DKK");
    import(
        &tx,
        "missing",
        "date;text;amount;currency\n2026-01-02;Synthetic known cash movement;10;DKK\n",
    );
    let unknown = read(&tx, "synthetic", "2026-10-20", None).unwrap();
    assert_eq!(unknown["cashflow"]["totalIn"], 10.0);
    assert!(unknown["cashflow"]["closingBalance"].is_null());
    assert_eq!(
        unknown["cashflow"]["balanceStatus"]["closing"],
        "no-balance-column"
    );
    assert_eq!(unknown["cashflow"]["status"], "partial");
    assert!(
        unknown["cashflow"]["balanceSeries"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    // A later independent statement ending at zero is actual evidence, not a default.
    import(
        &tx,
        "missing",
        "date;text;amount;currency;balance_after\n2026-02-01;Synthetic cash out;-10;DKK;0\n",
    );
    let zero = read(&tx, "synthetic", "2026-10-20", None).unwrap();
    assert_eq!(zero["cashflow"]["closingBalance"], 0.0);
    assert_eq!(zero["cashflow"]["balanceStatus"]["closing"], "known");
    assert!(zero["cashflow"]["openingBalance"].is_null());
}
#[test]
fn foreign_movement_uses_retained_rate_not_raw_foreign_amount() {
    let (_control, mut db) = database();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    account(&tx, "Foreign", "EUR");
    import(
        &tx,
        "foreign",
        "date;text;amount;currency;amount_dkk;fx_rate_to_dkk;balance_after\n2026-01-02;Synthetic foreign receipt;2;EUR;15;7.5;2\n",
    );
    let value = read(&tx, "synthetic", "2026-10-20", None).unwrap();
    assert_eq!(value["cashflow"]["totalIn"], 15.0);
    assert_eq!(value["cashflow"]["currency"], "DKK");
    assert!(value["cashflow"]["closingBalance"].is_null());
    assert_eq!(value["cashflow"]["balanceStatus"]["closing"], "unavailable");
    assert_eq!(value["cashflow"]["coverage"]["valuedTransactionCount"], 1);
}
#[test]
fn archived_journal_reference_is_not_a_fabricated_empty_bank_year() {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    let (_control, mut db) = database();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let bytes=serde_json::to_vec(&json!({"format":rentemester_engine::archive_import::PACKAGE,"currency":"DKK","files":[
        {"path":"2024/Posteringer.csv","contentBase64":STANDARD.encode("Konto;Kontonavn;Dato;Bilag;Bilagstype;Tekst;Moms;Beløb;Saldo\n6200;Synthetic archive;2024-06-01;1;Historical;Synthetic;;1;1\n")},
        {"path":"2025/Posteringer.csv","contentBase64":STANDARD.encode("Konto;Kontonavn;Dato;Bilag;Bilagstype;Tekst;Moms;Beløb;Saldo\n6200;Synthetic excluded;2025-01-01;1;Historical;Synthetic;;1;1\n")}
    ]})).unwrap();
    let (source, parsed) = crate::archive_import::decode_source(&bytes).unwrap();
    let prepared = crate::archive_import::prepare(&tx, &source, &parsed).unwrap();
    let input = crate::archive_import::Apply {
        plan_hash: prepared.review()["planHash"].as_str().unwrap().into(),
        idempotency_key: "synthetic-cashflow-archive".into(),
        confirm: true,
    };
    crate::archive_import::apply(&tx, &prepared, &input, "synthetic", 1_792_497_600_000).unwrap();
    let value = read(&tx, "synthetic", "2026-10-20", Some("2024")).unwrap();
    assert_eq!(value["cashflow"]["archived"], true);
    assert_eq!(value["cashflow"]["status"], "unavailable");
    assert!(value["cashflow"]["hasTransactions"].is_null());
    assert!(value["cashflow"]["periodStart"].is_null());
    assert!(value["cashflow"]["totalIn"].is_null());
    assert!(value["cashflow"]["months"].as_array().unwrap().is_empty());
}
