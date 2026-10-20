use super::*;
use rusqlite::{Connection, TransactionBehavior};
use serde_json::Value;
fn database() -> Connection {
    let mut control = Connection::open_in_memory().unwrap();
    let mut db = Connection::open_in_memory().unwrap();
    crate::fresh::initialize_empty_pair(&mut control, &mut db, "Synthetic Bank Tasks").unwrap();
    db.execute_batch("UPDATE companies SET vat_period_type='quarter' WHERE id=1; INSERT INTO accounts(account_no,name,type,normal_balance) VALUES('A','Synthetic bank','asset','debit'),('X','Synthetic expense','expense','debit'),('V','Synthetic input VAT','vat','debit'); INSERT INTO account_role_mappings(role,account_no,status,version,confirmed_by) VALUES('input_vat','V','confirmed',1,'synthetic'); INSERT INTO bank_accounts(slug,name,ledger_account_no) VALUES('synthetic','Synthetic bank','A');").unwrap();
    db
}
fn import(tx: &Transaction<'_>, csv: &str) -> Value {
    crate::bank::import(
        tx,
        csv,
        &rentemester_engine::bank::parse(csv).unwrap(),
        Some("synthetic"),
        "synthetic",
        "2026-10-18T12:00:00Z",
        "synthetic-original",
    )
    .unwrap()
}
fn tasks(tx: &Transaction<'_>) -> Vec<Value> {
    crate::exceptions::rows(tx, "all").unwrap()
}
fn document(tx: &Transaction<'_>, currency: &str, n: i64) {
    let metadata = serde_json::from_value(json!({"source":"synthetic","documentType":"purchase_sale","currency":currency,"issueDate":"2026-10-18","deliveryDescription":"Synthetic supplies","invoiceNo":format!("SYN-{n}"),"amountIncVat":125,"vatAmount":25,"sender":{"name":"Synthetic supplier","address":"Synthetic 1","vatOrCvr":"DK12345678"},"recipient":{"name":"Synthetic buyer","address":"Synthetic 2","vatOrCvr":"DK87654321"}})).unwrap();
    crate::documents::register(
        tx,
        crate::documents::Original {
            filename: "synthetic.txt",
            hash: &format!("{n:064x}"),
            path: &format!("synthetic-original-{n}"),
            mime: "text/plain",
        },
        &metadata,
        "synthetic",
        "2026-10-18",
    )
    .unwrap();
}
fn count(tx: &Transaction<'_>, table: &str) -> i64 {
    tx.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}
const CSV: &str = "date;text;amount\n2026-10-18;Synthetic outgoing;-125\n";
#[test]
fn import_creates_sign_currency_aware_tasks_and_duplicate_refresh_is_idempotent() {
    let mut db = database();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let csv = "date;text;amount;currency;amount_dkk;fx_rate_to_dkk\n2026-10-18;Synthetic outgoing;-1205;DKK;;\n2026-10-18;Synthetic incoming;125;DKK;;\n2026-10-18;Synthetic foreign;-100;EUR;-750;7.5\n";
    let result = import(&tx, csv);
    assert_eq!(result["exceptionsCreated"], 3);
    let initial = tasks(&tx);
    assert_eq!(initial.len(), 3);
    let outgoing = initial
        .iter()
        .find(|row| row["relatedBankTransactionId"] == 1)
        .unwrap();
    assert!(
        outgoing["message"]
            .as_str()
            .unwrap()
            .contains("1.205,00 kr.")
    );
    let incoming = initial
        .iter()
        .find(|row| row["relatedBankTransactionId"] == 2)
        .unwrap();
    assert!(
        incoming["message"]
            .as_str()
            .unwrap()
            .starts_with("Indbetalingen")
    );
    assert!(
        !incoming["requiredAction"]
            .as_str()
            .unwrap()
            .contains("udgiften")
    );
    let foreign = initial
        .iter()
        .find(|row| row["relatedBankTransactionId"] == 3)
        .unwrap();
    assert!(foreign["message"].as_str().unwrap().contains("100,00 EUR"));
    assert!(!foreign["message"].as_str().unwrap().contains("kr."));
    assert_eq!(foreign["sourceEvidence"]["amount"], -100.0);
    assert_eq!(foreign["sourceEvidence"]["currency"], "EUR");
    assert_eq!(import(&tx, csv)["exceptionsCreated"], 0);
    assert_eq!(tasks(&tx), initial);
    let changes = tx.total_changes();
    crate::exceptions::list(&tx, "synthetic", "open").unwrap();
    assert_eq!(
        tx.total_changes(),
        changes,
        "GET-style queue read must not synchronize"
    );
    assert_eq!(count(&tx, "bank_transactions"), 3);
    assert_eq!(count(&tx, "journal_entries"), 0);
    assert_eq!(count(&tx, "documents"), 0);
}
#[test]
fn late_voucher_refresh_appends_history_and_ambiguity_or_wrong_currency_never_selects() {
    let mut db = database();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    assert_eq!(import(&tx, CSV)["exceptionsCreated"], 1);
    let first = tasks(&tx)[0].clone();
    assert_eq!(first["relatedDocumentId"], Value::Null);
    document(&tx, "EUR", 1);
    assert_eq!(
        import(&tx, CSV)["exceptionsCreated"],
        0,
        "same amount in another currency is no candidate"
    );
    document(&tx, "DKK", 2);
    assert_eq!(import(&tx, CSV)["exceptionsCreated"], 1);
    let rows = tasks(&tx);
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["relatedDocumentId"], 2);
    assert!(
        rows[0]["message"]
            .as_str()
            .unwrap()
            .contains("Beløbet alene beviser ikke")
    );
    assert_eq!(rows[1]["id"], first["id"]);
    assert_eq!(rows[1]["message"], first["message"]);
    assert_eq!(rows[1]["status"], "resolved");
    assert_eq!(import(&tx, CSV)["exceptionsCreated"], 0);
    document(&tx, "DKK", 3);
    assert_eq!(import(&tx, CSV)["exceptionsCreated"], 1);
    let ambiguous = tasks(&tx);
    assert_eq!(ambiguous.len(), 3);
    assert_eq!(ambiguous[0]["relatedDocumentId"], Value::Null);
    assert_eq!(
        ambiguous[0]["sourceEvidence"]["candidateBilagId"],
        Value::Null
    );
    assert_eq!(ambiguous[1]["status"], "resolved");
    assert_eq!(import(&tx, CSV)["exceptionsCreated"], 0);
    let incoming = "date;text;amount\n2026-10-18;Synthetic incoming;125\n";
    import(&tx, incoming);
    let income = tasks(&tx)
        .into_iter()
        .find(|row| row["relatedBankTransactionId"] == 2)
        .unwrap();
    assert_eq!(income["relatedDocumentId"], Value::Null);
    assert_eq!(count(&tx, "bank_transactions"), 2);
    assert_eq!(count(&tx, "documents"), 3);
    assert_eq!(count(&tx, "journal_entries"), 0);
}
#[test]
fn reviewed_expense_post_resolves_task_and_duplicate_import_never_reopens_it() {
    let mut db = database();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    document(&tx, "DKK", 1);
    assert_eq!(import(&tx, CSV)["exceptionsCreated"], 1);
    assert_eq!(tasks(&tx)[0]["relatedDocumentId"], 1);
    let input=serde_json::from_value(json!({"documentId":1,"bankTransactionId":1,"expenseAccountNo":"X","vatTreatment":"standard"})).unwrap();
    crate::expense::post(
        &tx,
        &input,
        "synthetic",
        rentemester_engine::fiscal_year::Date::parse("2026-10-18").unwrap(),
    )
    .unwrap();
    assert_eq!(tasks(&tx)[0]["status"], "resolved");
    assert_eq!(import(&tx, CSV)["exceptionsCreated"], 0);
    assert_eq!(tasks(&tx).len(), 1);
    assert_eq!(count(&tx, "documents"), 1);
    assert_eq!(count(&tx, "journal_entries"), 1);
    assert_eq!(count(&tx, "bank_transactions"), 1);
}
