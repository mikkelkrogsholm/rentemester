//! Narrow source-driven adapter proofs; no caller-supplied qualification seeds.
use super::*;
use crate::dinero_import;
use rentemester_engine::{dinero_import::containers, fiscal_year::Date};
use rusqlite::Connection;

const NOW: i64 = 1_792_497_600_000;
fn latest() -> Date {
    Date::parse("2026-10-21").unwrap()
}
fn db() -> (Connection, Connection) {
    let mut control = Connection::open_in_memory().unwrap();
    let mut company = Connection::open_in_memory().unwrap();
    crate::fresh::initialize_empty_pair(&mut control, &mut company, "Synthetic VAT import")
        .unwrap();
    company
        .execute(
            "UPDATE companies SET vat_period_type='quarter' WHERE id=1",
            [],
        )
        .unwrap();
    (control, company)
}
fn source() -> Vec<u8> {
    // Reuse the retained import fixture decoder/package helper, retaining its
    // synthetic company master but replacing accounting with one ordinary buy.
    let decoded = dinero_import::decode_source(include_bytes!(
        "../dinero_import/fixtures/source126-stored.zip"
    ))
    .unwrap();
    let mut files = decoded.files;
    files.retain(|f| {
        f.path == "Firmaoplysninger.csv" || f.path.ends_with(".csv") && f.path.starts_with("2025/")
    });
    for file in &mut files {
        let replacement = match file.path.as_str() {
            "2025/Kontoplan.csv" => {
                "Nummer;Navn;Type;Momstype;Total\n2000;Synthetic expense;Udgift;;100,00\n5510;Bankkonto;Aktiv;;-25,00\n60010;Overført resultat;Passiv;;-100,00\n64060;Købsmoms;Passiv;;25,00\n"
            }
            "2025/Posteringer.csv" => {
                "Konto;Kontonavn;Dato;Bilag;Bilagstype;Tekst;Momstype;Beløb;Saldo\n5510;Bank;2025-01-01;0;;Primobeholdning;;100,000000;100,000000\n60010;Equity;2025-01-01;0;;Primobeholdning;;-100,000000;-100,000000\n2000;Expense;2025-03-20;1;Køb;Synthetic purchase;I25;100,000000;100,000000\n64060;VAT;2025-03-20;1;Køb;Synthetic purchase;;25,000000;25,000000\n5510;Bank;2025-03-20;1;Køb;Synthetic purchase;;-125,000000;-25,000000\n"
            }
            "2025/SaldoBalance.csv" => "Konto;Kontonavn;Beløb\n5510;Bankkonto;-25,000000\n",
            _ => continue,
        };
        file.bytes = replacement.as_bytes().to_vec();
    }
    containers::package(&files).unwrap()
}
fn install(tx: &Transaction<'_>, bytes: &[u8]) {
    let decoded = dinero_import::decode_source(bytes).unwrap();
    let prepared = dinero_import::prepare(tx, &decoded, latest()).unwrap();
    assert_eq!(
        prepared.review()["status"],
        "ready",
        "{}",
        prepared.review()
    );
    let receipt = dinero_import::apply(
        tx,
        &prepared,
        &dinero_import::Apply {
            plan_hash: prepared.review()["planHash"].as_str().unwrap().into(),
            idempotency_key: "synthetic-historical-vat".into(),
            confirm: true,
        },
        latest(),
        "synthetic",
        NOW,
    )
    .unwrap();
    assert_eq!(receipt["journalCount"], 1);
    let originals = dinero_import::originals(tx).unwrap();
    assert_eq!(originals.len(), 1);
    dinero_import::verify_original(tx, &originals[0], bytes).unwrap();
}

#[test]
fn accepted_native_import_uses_same_qualified_vat_for_range_read_and_portfolio() {
    let (_control, mut db) = db();
    let bytes = source();
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    install(&tx, &bytes);
    let before = tx.total_changes();
    let projected = rows(&tx).unwrap();
    for row in &projected {
        let exact: (String, i64, String, String, Option<String>) = tx.query_row(
            "SELECT e.transaction_date,l.id,e.status,a.normal_balance,a.default_vat_code FROM journal_lines l JOIN journal_entries e ON e.id=l.journal_entry_id JOIN accounts a ON a.id=l.account_id WHERE l.id=?1",
            [row.line_id],
            |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?)),
        ).unwrap();
        assert_eq!(
            exact,
            (
                row.date.clone(),
                row.line_id,
                row.status.clone(),
                row.normal_balance.clone(),
                row.default_vat_code.clone()
            )
        );
    }
    assert_eq!(projected.len(), 5);
    assert_eq!(lines(&tx).unwrap().len(), projected.len());
    let report = range_report(&tx, "2025-01-01", "2025-03-31").unwrap();
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    assert_eq!(report.input_vat.decimal(), "25.00");
    assert_eq!(report.output_vat.decimal(), "0.00");
    assert_eq!(report.payable.decimal(), "-25.00");
    assert_eq!(report.canonical_bases["purchaseBase25"].decimal(), "100.00");
    let native = read(&tx, "synthetic", "2025-04-01", Some("2025")).unwrap();
    assert_eq!(native["inputVat"].as_f64(), Some(25.0));
    assert_eq!(native["payable"].as_f64(), Some(-25.0));
    assert_eq!(native["vatReportErrors"], json!([]));
    let portfolio = crate::portfolio::company(&tx, "synthetic", "2025-03-31").unwrap();
    assert_eq!(portfolio["netVatPayable"].as_f64(), Some(-25.0));
    assert_eq!(portfolio["vat"]["errors"], json!([]));
    assert_eq!(tx.total_changes(), before);
    assert!(crate::journal_hash::verify(&tx).unwrap().ok);
    tx.rollback().unwrap();
    assert_eq!(
        db.query_row("SELECT count(*) FROM journal_entries", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
}

// Deliberate corruption only: temporarily remove and restore this table's
// immutable triggers, as in the existing native import corruption fixtures.
fn corrupt(db: &Connection, table: &str, sql: &str) {
    let triggers: Vec<(String, String)> = db
        .prepare("SELECT name,sql FROM sqlite_master WHERE type='trigger' AND tbl_name=?1")
        .unwrap()
        .query_map([table], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    for (name, _) in &triggers {
        db.execute_batch(&format!("DROP TRIGGER \"{}\"", name.replace('"', "\"\"")))
            .unwrap();
    }
    db.execute_batch(sql).unwrap();
    for (_, ddl) in triggers {
        db.execute_batch(&ddl).unwrap();
    }
}

#[test]
fn corrupt_attestation_or_source_binding_refuses_all_vat_paths() {
    for (table, mutation) in [
        (
            "audit_log",
            "UPDATE audit_log SET message='{}' WHERE event_type='native_dinero_import'",
        ),
        (
            "dinero_import_sources",
            "UPDATE dinero_import_sources SET raw_size_bytes=raw_size_bytes+1",
        ),
    ] {
        let (_control, mut db) = db();
        let tx = db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        install(&tx, &source());
        corrupt(&tx, table, mutation);
        assert!(range_report(&tx, "2025-01-01", "2025-03-31").is_err());
        assert!(read(&tx, "synthetic", "2025-04-01", Some("2025")).is_err());
        assert!(crate::portfolio::company(&tx, "synthetic", "2025-03-31").is_err());
        tx.rollback().unwrap();
    }
}

#[test]
fn marker_only_manual_journal_never_qualifies_as_accepted_history() {
    let (_control, mut db) = db();
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    for (no, kind, normal) in [
        ("2000", "equity", "credit"),
        ("5510", "asset", "debit"),
        ("64060", "vat", "debit"),
    ] {
        crate::company_setup::create_account(
            &tx,
            &crate::company_setup::Account {
                account_no: no.into(),
                name: format!("Synthetic {no}"),
                r#type: kind.into(),
                normal_balance: normal.into(),
                default_vat_code: None,
            },
            "synthetic",
        )
        .unwrap();
    }
    let request = serde_json::from_value(json!({"transactionDate":"2025-03-20","text":"Synthetic marker spoof","currency":"DKK","lines":[{"accountNo":"5510","debitAmount":"25.00"},{"accountNo":"2000","creditAmount":"25.00"}]})).unwrap();
    let posted = crate::journal_post::post(
        &tx,
        &request,
        &crate::journal_preview::manual_journal_rule_version().unwrap(),
        latest(),
        crate::journal_post::Actor {
            subject: "synthetic",
            program: "rentemester-rust",
        },
    )
    .unwrap();
    assert_eq!(posted["ok"], true, "{posted}");
    corrupt(
        &tx,
        "journal_entries",
        "UPDATE journal_entries SET created_by_program='rentemester-import-postings'",
    );
    assert!(dinero_import::accepted_journal_ids(&tx).unwrap().is_empty());
    let report = range_report(&tx, "2025-01-01", "2025-03-31").unwrap();
    assert!(
        report
            .errors
            .iter()
            .any(|error| error.contains("historisk import mangler verificeret kildevalidering")),
        "{:?}",
        report.errors
    );
    tx.rollback().unwrap();
}
