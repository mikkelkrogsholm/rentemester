//! Source-driven storage acceptance, independent from application receipt replay.
use super::*;
use rusqlite::TransactionBehavior;
const NOW: i64 = 1_792_497_600_000;
fn latest() -> Date {
    Date::parse("2026-10-21").unwrap()
}
fn db() -> (Connection, Connection) {
    let mut control = Connection::open_in_memory().unwrap();
    let mut company = Connection::open_in_memory().unwrap();
    crate::fresh::initialize_empty_pair(&mut control, &mut company, "Synthetic import").unwrap();
    (control, company)
}
fn source(control: &str, archive: bool) -> Vec<u8> {
    let schedule = json!({"contract":"rentemester-imported-receivables-v1","sourceDocumentHash":"a".repeat(64),"invoices":[
        {"id":"INV-opening","customerId":"C-1","customerName":"Synthetic opening customer","invoiceDate":"2024-12-20","grossAmount":30000,"controlAccountNo":"5520","recognitionRef":"opening-debtors","documentHash":"b".repeat(64),"payments":[{"id":"PAY-1","eventKind":"payment","paymentDate":"2025-03-05","amount":30000,"paymentRef":"dinero-payment-1","documentHash":"c".repeat(64)}]},
        {"id":"INV-current","customerId":"C-2","customerName":"Synthetic current customer","invoiceDate":"2025-03-20","grossAmount":25000,"controlAccountNo":"5520","recognitionRef":"dinero-invoice-2","documentHash":"d".repeat(64),"payments":[{"id":"CN-1","eventKind":"credit_note","paymentDate":"2025-03-21","amount":500,"paymentRef":"dinero-credit-1","documentHash":"e".repeat(64)}]}
    ]});
    let mut files=vec![
        ("Firmaoplysninger.csv","Firmanavn;Virksomhedstype;Adresse;Postnr;By;Land;Email;AttPerson;CvrNr;EanNr;Telefonnummer;Hjemmeside\nSynthetic Source Company;Anpartsselskab;Synthetic Street 124;1000;Synthetic City;Danmark;source@example.invalid;Synthetic Owner;87654321;;;\n".to_owned()),
        ("2025/Kontoplan.csv","Nummer;Navn;Type;Momstype;Total\n1000;Synthetic sales;Indtægt;;-24500,00\n5510;Bankkonto;Aktiv;;30000,00\n5520;Tilgodehavender fra salg;Aktiv;;24500,00\n60010;Overført resultat fra tidligere år;Passiv;;-30000,00\n".to_owned()),
        ("2025/Posteringer.csv","Konto;Kontonavn;Dato;Bilag;Bilagstype;Tekst;Momstype;Beløb;Saldo\n5520;Tilgodehavender fra salg;2025-01-01;0;;Primobeholdning;;30000,000000;30000,000000\n60010;Overført resultat fra tidligere år;2025-01-01;0;;Primobeholdning;;-30000,000000;-30000,000000\n5510;Bankkonto;2025-03-05;1;Registrering af betaling;Synthetic opening payment;;30000,000000;30000,000000\n5520;Tilgodehavender fra salg;2025-03-05;1;Registrering af betaling;Synthetic opening payment;;-30000,000000;0,000000\n5520;Tilgodehavender fra salg;2025-03-20;2;Finansbilag;Synthetic current invoice;;25000,000000;25000,000000\n1000;Synthetic sales;2025-03-20;2;Finansbilag;Synthetic current invoice;;-25000,000000;-25000,000000\n1000;Synthetic sales;2025-03-21;3;Finansbilag;Synthetic credit;;500,000000;-24500,000000\n5520;Tilgodehavender fra salg;2025-03-21;3;Finansbilag;Synthetic credit;;-500,000000;24500,000000\n".to_owned()),
        ("2025/SaldoBalance.csv",format!("Konto;Kontonavn;Beløb\n5520;Tilgodehavender fra salg;{control}\n")),
        ("Rentemester-modtagerposter-v1.json",serde_json::to_string(&schedule).unwrap()),
    ];
    if archive {
        files.push(("2024/Posteringer.csv","Konto;Kontonavn;Dato;Bilag;Bilagstype;Tekst;Momstype;Beløb;Saldo\n5510;Synthetic historical bank;2024-06-01;7;Finansbilag;Synthetic retained source;;100,000000;100,000000\n1000;Synthetic historical sales;2024-06-01;7;Finansbilag;Synthetic retained source;;-100,000000;-100,000000\n".into()));
    }
    serde_json::to_vec(&json!({"format":PACKAGE,"currency":"DKK","files":files.iter().map(|(path,bytes)|json!({"path":path,"contentBase64":STANDARD.encode(bytes)})).collect::<Vec<_>>()})).unwrap()
}
fn snapshot(db: &Connection) -> Value {
    let mut out = serde_json::Map::new();
    for table in [
        "journal_entries",
        "journal_lines",
        "accounts",
        "documents",
        "audit_log",
        "dinero_import_sources",
        "dinero_import_inventories",
        "dinero_import_inventory_entries",
        "dinero_import_attempts",
        "imported_receivable_headers",
        "imported_receivable_events",
    ] {
        let n: i64 = db
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
            .unwrap();
        out.insert(table.into(), json!(n));
    }
    Value::Object(out)
}
fn input(p: &Prepared) -> Apply {
    Apply {
        plan_hash: p.review()["planHash"].as_str().unwrap().into(),
        idempotency_key: "synthetic-native-source-key".into(),
        confirm: true,
    }
}
fn install(tx: &Transaction<'_>, bytes: &[u8]) -> Value {
    let decoded = decode_source(bytes).unwrap();
    let p = prepare(tx, &decoded, latest()).unwrap();
    assert_eq!(p.review()["status"], "ready", "{}", p.review());
    apply(tx, &p, &input(&p), latest(), "synthetic", NOW).unwrap()
}
fn verify_sources(db: &Connection, bytes: &[u8]) {
    validate_history(db).unwrap();
    let refs = originals(db).unwrap();
    assert!(!refs.is_empty());
    for original in refs {
        verify_original(db, &original, bytes).unwrap();
    }
}
fn assert_as_of(tx: &Transaction<'_>, date: &str, count: u64, total: i64) {
    let list = crate::imported_receivables::read(tx, date).unwrap();
    assert_eq!(list["ok"], true);
    assert_eq!(list["count"], count);
    assert_eq!(list["totalOpen"].as_f64(), Some(total as f64));
    assert_eq!(list["errors"], json!([]));
}
#[test]
fn plan_is_readonly_and_native_acceptance_couples_four_journals_control_schedule_and_original() {
    let (_control, mut db) = db();
    let bytes = source("24500,000000", false);
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let before = snapshot(&tx);
    let changes = tx.total_changes();
    let decoded = decode_source(&bytes).unwrap();
    let p = prepare(&tx, &decoded, latest()).unwrap();
    assert_eq!(p.review()["status"], "ready");
    assert_eq!(p.review()["counts"]["openingLines"], 2);
    assert_eq!(p.review()["counts"]["historicalVouchers"], 3);
    assert_eq!(p.review()["counts"]["importedInvoices"], 2);
    assert_eq!(p.review()["counts"]["importedEvents"], 2);
    assert_eq!(snapshot(&tx), before);
    assert_eq!(tx.total_changes(), changes);
    let receipt = apply(&tx, &p, &input(&p), latest(), "synthetic", NOW).unwrap();
    assert!(receipt["attemptId"].as_i64().is_some_and(|n| n > 0));
    let after = snapshot(&tx);
    assert_eq!(after["journal_entries"], 4);
    assert_eq!(after["dinero_import_attempts"], 1);
    assert_eq!(after["dinero_import_sources"], 1);
    assert_eq!(after["dinero_import_inventories"], 1);
    assert_eq!(after["dinero_import_inventory_entries"], 5);
    assert_eq!(after["imported_receivable_headers"], 2);
    assert_eq!(after["imported_receivable_events"], 2);
    assert_eq!(after["documents"], 0);
    let debtor:f64=tx.query_row("SELECT SUM(jl.debit_amount-jl.credit_amount) FROM journal_lines jl JOIN accounts a ON a.id=jl.account_id WHERE a.account_no='5520'",[],|r|r.get(0)).unwrap();
    assert_eq!(debtor, 24500.0);
    assert_as_of(&tx, "2024-12-19", 0, 0);
    assert_as_of(&tx, "2025-03-04", 1, 30000);
    assert_as_of(&tx, "2025-03-05", 1, 0);
    assert_as_of(&tx, "2025-03-20", 2, 25000);
    assert_as_of(&tx, "2025-03-21", 2, 24500);
    let list = crate::imported_receivables::read(&tx, "2025-03-21").unwrap();
    assert_eq!(list["rows"][0]["externalInvoiceId"], "INV-opening");
    assert_eq!(list["rows"][0]["openBalance"].as_f64(), Some(0.0));
    assert_eq!(list["rows"][0]["paidAmount"].as_f64(), Some(30000.0));
    assert_eq!(list["rows"][1]["paidAmount"].as_f64(), Some(500.0));
    assert!(
        list["rows"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["source"] == "imported")
    );
    verify_sources(&tx, &bytes);
    assert!(crate::periods::verify_sources(&tx).unwrap().is_empty());
    assert!(crate::journal_hash::verify(&tx).unwrap().ok);
    tx.commit().unwrap();
    // Storage-level duplicate acceptance is refused. Durable exact-key replay is
    // provided by the application, not a second domain write.
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let before = snapshot(&tx);
    let p = prepare(&tx, &decoded, latest()).unwrap();
    assert_eq!(p.review()["status"], "blocked");
    assert!(apply(&tx, &p, &input(&p), latest(), "synthetic", NOW).is_err());
    assert_eq!(snapshot(&tx), before);
    tx.rollback().unwrap();
}
#[test]
fn source_control_one_ore_difference_blocks_apply_without_partial_effects() {
    let (_control, mut db) = db();
    let bytes = source("24500,010000", false);
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let before = snapshot(&tx);
    let changes = tx.total_changes();
    let p = prepare(&tx, &decode_source(&bytes).unwrap(), latest()).unwrap();
    assert_eq!(p.review()["status"], "blocked");
    assert!(
        p.review()["controls"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["reconciles"] == false)
    );
    assert!(apply(&tx, &p, &input(&p), latest(), "synthetic", NOW).is_err());
    assert_eq!(snapshot(&tx), before);
    assert_eq!(tx.total_changes(), changes);
    tx.rollback().unwrap();
}
#[test]
fn accepted_import_transaction_rollback_discards_journals_provenance_and_schedule() {
    let (_control, mut db) = db();
    let bytes = source("24500,000000", false);
    let before = snapshot(&db);
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    install(&tx, &bytes);
    assert_eq!(snapshot(&tx)["journal_entries"], 4);
    tx.rollback().unwrap();
    assert_eq!(snapshot(&db), before);
}
#[test]
fn chart_or_company_change_invalidates_review_before_financial_writes() {
    for profile in [false, true] {
        let (_control, mut db) = db();
        let bytes = source("24500,000000", false);
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        let p = prepare(&tx, &decode_source(&bytes).unwrap(), latest()).unwrap();
        if profile {
            crate::company_setup::update_profile(
                &tx,
                json!({"cvr":"11223344"}).as_object().unwrap(),
                "synthetic",
            )
            .unwrap();
        } else {
            crate::company_setup::create_account(
                &tx,
                &crate::company_setup::Account {
                    account_no: "99001".into(),
                    name: "Synthetic later chart account".into(),
                    r#type: "expense".into(),
                    normal_balance: "debit".into(),
                    default_vat_code: None,
                },
                "synthetic",
            )
            .unwrap();
        }
        let before = snapshot(&tx);
        let changes = tx.total_changes();
        assert!(apply(&tx, &p, &input(&p), latest(), "synthetic", NOW).is_err());
        assert_eq!(snapshot(&tx), before);
        assert_eq!(tx.total_changes(), changes);
        tx.rollback().unwrap();
    }
}
#[test]
fn source_proof_and_paid_rows_survive_real_database_reopen() {
    let (_control, mut db) = db();
    let bytes = source("24500,000000", false);
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    install(&tx, &bytes);
    tx.commit().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("reopened.sqlite");
    db.execute("VACUUM INTO ?1", [path.to_str().unwrap()])
        .unwrap();
    drop(db);
    let mut reopened = Connection::open(path).unwrap();
    crate::validate(&reopened).unwrap();
    verify_sources(&reopened, &bytes);
    let tx = reopened
        .transaction_with_behavior(TransactionBehavior::Deferred)
        .unwrap();
    assert_as_of(&tx, "2025-12-31", 2, 24500);
    assert_eq!(snapshot(&tx)["documents"], 0);
    tx.rollback().unwrap();
}
fn corrupt(db: &Connection, table: &str, sql: &str) {
    let triggers = db
        .prepare(
            "SELECT name,sql FROM sqlite_schema WHERE type='trigger' AND tbl_name=?1 ORDER BY name",
        )
        .unwrap()
        .query_map([table], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
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
fn altered_schedule_attestation_or_original_never_validates_as_retained_source() {
    for kind in 0..3 {
        let (_control, mut db) = db();
        let bytes = source("24500,000000", false);
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        install(&tx, &bytes);
        tx.commit().unwrap();
        verify_sources(&db, &bytes);
        let original = originals(&db).unwrap().remove(0);
        match kind {
            0 => corrupt(
                &db,
                "imported_receivable_events",
                "UPDATE imported_receivable_events SET amount=499 WHERE event_kind='credit_note'",
            ),
            1 => corrupt(
                &db,
                "audit_log",
                "UPDATE audit_log SET message='{}' WHERE event_type='native_dinero_import'",
            ),
            _ => {
                let mut altered = bytes.clone();
                altered.push(b' ');
                assert!(verify_original(&db, &original, &altered).is_err());
                continue;
            }
        }
        assert!(validate_history(&db).is_err() || verify_original(&db, &original, &bytes).is_err());
    }
}
#[test]
fn normal_manual_post_cannot_spoof_historical_import_marker() {
    let (_control, mut db) = db();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    for (no, kind, balance) in [("5510", "asset", "debit"), ("1000", "equity", "credit")] {
        crate::company_setup::create_account(
            &tx,
            &crate::company_setup::Account {
                account_no: no.into(),
                name: format!("Synthetic {no}"),
                r#type: kind.into(),
                normal_balance: balance.into(),
                default_vat_code: None,
            },
            "synthetic",
        )
        .unwrap();
    }
    let request=serde_json::from_value(json!({"transactionDate":"2025-03-20","text":"Synthetic attempted bypass","currency":"DKK","createdByProgram":"rentemester-import-postings","lines":[{"accountNo":"5510","debitAmount":"10.00"},{"accountNo":"1000","creditAmount":"10.00"}]})).unwrap();
    let before = snapshot(&tx);
    let changes = tx.total_changes();
    let result = crate::journal_post::post(
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
    assert_eq!(result["ok"], false);
    assert!(result["errors"].as_array().unwrap().iter().any(|error| {
        error
            .as_str()
            .is_some_and(|message| message.contains("reserved"))
    }));
    assert_eq!(snapshot(&tx), before);
    assert_eq!(tx.total_changes(), changes);
    tx.rollback().unwrap();
}
#[test]
fn earlier_year_archive_is_source_bound_but_never_reposted_into_live_year() {
    let (_control, mut db) = db();
    let bytes = source("24500,000000", true);
    let decoded = decode_source(&bytes).unwrap();
    let archived = decoded.archive.as_ref().unwrap();
    assert_eq!(archived.parsed.years.len(), 1);
    assert_eq!(archived.parsed.years[0].fiscal_year, 2024);
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let p = prepare(&tx, &decoded, latest()).unwrap();
    assert_eq!(p.review()["counts"]["archiveYears"], 1);
    apply(&tx, &p, &input(&p), latest(), "synthetic", NOW).unwrap();
    assert_eq!(snapshot(&tx)["journal_entries"], 4);
    verify_sources(&tx, &bytes);
    let proofs = crate::archive_import::originals(&tx)
        .unwrap()
        .iter()
        .map(|r| crate::archive_import::verify_original(&tx, r, &archived.bytes).unwrap())
        .collect::<Vec<_>>();
    let archive = crate::archive_import::read(&tx, &proofs, Some(2024)).unwrap();
    assert_eq!(archive["years"][0]["postingCount"], 2);
    assert_as_of(&tx, "2025-03-21", 2, 24500);
    tx.commit().unwrap();
}
#[test]
fn chart_only_plan_discloses_missing_opening_and_never_invents_historical_postings() {
    let (_control, mut db) = db();
    let mut package: Value = serde_json::from_slice(&source("24500,000000", false)).unwrap();
    package["files"].as_array_mut().unwrap().retain(|file| {
        matches!(
            file["path"].as_str(),
            Some("Firmaoplysninger.csv" | "2025/Kontoplan.csv")
        )
    });
    let bytes = serde_json::to_vec(&package).unwrap();
    let decoded = decode_source(&bytes).unwrap();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let before = snapshot(&tx);
    let p = prepare(&tx, &decoded, latest()).unwrap();
    assert_eq!(p.review()["counts"]["openingLines"], 0);
    assert_eq!(p.review()["counts"]["historicalVouchers"], 0);
    assert_eq!(p.review()["counts"]["importedInvoices"], 0);
    assert!(
        p.review()["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|notice| notice
                .as_str()
                .is_some_and(|s| s.contains("chart-only") && s.contains("not posted")))
    );
    assert_eq!(snapshot(&tx), before);
    tx.rollback().unwrap();
}
fn change_file(package: &mut Value, path: &str, change: impl FnOnce(String) -> String) {
    let file = package["files"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|f| f["path"] == path)
        .unwrap();
    let old = String::from_utf8(
        STANDARD
            .decode(file["contentBase64"].as_str().unwrap())
            .unwrap(),
    )
    .unwrap();
    file["contentBase64"] = json!(STANDARD.encode(change(old)));
}
#[test]
fn explicit_source_vat_opening_is_auditable_without_relaxing_normal_manual_vat() {
    let (_control, mut db) = db();
    let mut package: Value = serde_json::from_slice(&source("24500,000000", false)).unwrap();
    change_file(&mut package, "2025/Kontoplan.csv", |text| {
        text.replace(
            "60010;Overført resultat fra tidligere år;Passiv;;-30000,00",
            "60010;Overført resultat fra tidligere år;Passiv;;-29500,00",
        ) + "64000;Synthetic source output VAT;Passiv;;-500,00\n"
    });
    change_file(&mut package, "2025/Posteringer.csv", |text| {
        text.replace("60010;Overført resultat fra tidligere år;2025-01-01;0;;Primobeholdning;;-30000,000000;-30000,000000","60010;Overført resultat fra tidligere år;2025-01-01;0;;Primobeholdning;;-29500,000000;-29500,000000")+"64000;Synthetic source output VAT;2025-01-01;0;;Primobeholdning;;-500,000000;-500,000000\n"
    });
    let bytes = serde_json::to_vec(&package).unwrap();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    install(&tx, &bytes);
    let vat:f64=tx.query_row("SELECT SUM(l.credit_amount-l.debit_amount) FROM journal_lines l JOIN accounts a ON a.id=l.account_id WHERE a.account_no='64000'",[],|r|r.get(0)).unwrap();
    assert_eq!(vat, 500.0);
    verify_sources(&tx, &bytes);
    assert!(crate::periods::verify_sources(&tx).unwrap().is_empty());
    let request=serde_json::from_value(json!({"transactionDate":"2025-03-22","text":"Synthetic uncoded manual VAT refusal","currency":"DKK","lines":[{"accountNo":"64000","debitAmount":"1.00"},{"accountNo":"5510","creditAmount":"1.00"}]})).unwrap();
    let before = snapshot(&tx);
    let result = crate::journal_post::post(
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
    assert_eq!(result["ok"], false);
    assert!(
        result["errors"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e.as_str().is_some_and(|s| s.to_lowercase().contains("vat")))
    );
    assert_eq!(snapshot(&tx), before);
    tx.commit().unwrap();
}
fn configured_debtor_balance(tx: &Transaction<'_>, date: &str) -> f64 {
    tx.query_row("SELECT COALESCE(SUM(l.debit_amount-l.credit_amount),0) FROM journal_lines l JOIN journal_entries j ON j.id=l.journal_entry_id JOIN accounts a ON a.id=l.account_id JOIN account_role_mappings m ON m.account_no=a.account_no AND m.role='debtors' AND m.status='confirmed' WHERE j.transaction_date<=?1",[date],|r|r.get(0)).unwrap()
}
#[test]
fn imported_and_native_invoices_reconcile_same_configured_debtor_at_each_cutoff() {
    let (_control, mut db) = db();
    let bytes = source("24500,000000", false);
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    install(&tx, &bytes);
    // Metadata only: every financial effect below uses the actual source,
    // statement-import and native invoice writers.
    tx.execute_batch("UPDATE companies SET address='Synthetic Street 124',cvr='87654321',vat_period_type='quarter';INSERT INTO accounts(account_no,name,type,normal_balance)VALUES('5400','Synthetic creditors','liability','credit'),('64000','Synthetic output VAT','vat','credit');").unwrap();
    for (role, account) in [
        ("bank", "5510"),
        ("debtors", "5520"),
        ("creditors", "5400"),
        ("output_vat", "64000"),
    ] {
        crate::company_setup::confirm_role(
            &tx,
            &crate::company_setup::Role {
                role: role.into(),
                account_no: account.into(),
                confirm: true,
            },
            "synthetic",
        )
        .unwrap();
    }
    crate::bank::create_account(&tx,&serde_json::from_value(json!({"name":"Synthetic independent source bank","slug":"synthetic-source-bank","currency":"DKK","ledgerAccountNo":"5510"})).unwrap(),"synthetic").unwrap();
    let csv = "date;text;amount;currency;reference;saldo\n2025-03-05;Synthetic independent source payment;30000;DKK;source-opening-payment;30000\n";
    crate::bank::import(
        &tx,
        csv,
        &rentemester_engine::bank::parse(csv).unwrap(),
        Some("synthetic-source-bank"),
        "synthetic",
        "2026-10-21T12:00:00Z",
        "synthetic",
    )
    .unwrap();
    let customer=crate::sales::create_customer(&tx,&serde_json::from_value(json!({"name":"Synthetic native buyer","address":"Synthetic Street 125","vatOrCvr":"DK11223344","paymentTermsDays":1})).unwrap(),"synthetic").unwrap();
    let input=serde_json::from_value(json!({"issueDate":"2025-03-22","dueDate":"2025-03-23","currency":"DKK","customerId":customer["id"],"lines":[{"description":"Synthetic native service","quantity":1,"unitPriceExVat":1000}]})).unwrap();
    let payload = crate::sales::prepare(&tx, &input, "2025-03-22T12:00:00Z", true).unwrap();
    let original_hash = crate::invoice_email::sha(&serde_json::to_vec_pretty(&payload).unwrap());
    let pdf_hash = crate::invoice_email::sha(b"Synthetic native combined-control PDF");
    let issued = crate::sales::register(
        &tx,
        &payload,
        crate::sales::Artifact {
            filename: "synthetic-combined.json",
            path: "synthetic-combined.json",
            hash: &original_hash,
            mime: "application/json",
        },
        crate::sales::Artifact {
            filename: "synthetic-combined.pdf",
            path: "synthetic-combined.pdf",
            hash: &pdf_hash,
            mime: "application/pdf",
        },
        "synthetic",
    )
    .unwrap();
    let id = issued["documentId"].as_i64().unwrap();
    crate::sales::post(
        &tx,
        &crate::sales::Post {
            invoice_document_id: id,
            revenue_account_no: "1000".into(),
            confirm: true,
        },
        "synthetic",
        latest(),
    )
    .unwrap();
    for (date, expected, native) in [
        ("2025-03-21", 24500.0, 0.0),
        ("2025-03-22", 25750.0, 1250.0),
    ] {
        let imported = crate::imported_receivables::read(&tx, date).unwrap();
        assert_eq!(imported["totalOpen"].as_f64(), Some(24500.0));
        assert_eq!(configured_debtor_balance(&tx, date), expected);
        assert_eq!(expected, 24500.0 + native);
        let actual_native:f64=tx.query_row("SELECT COALESCE(SUM(p.booked_gross_dkk),0) FROM issued_invoice_postings p JOIN documents d ON d.id=p.invoice_document_id JOIN accounts a ON a.id=p.receivable_account_id WHERE d.invoice_date<=?1 AND a.account_no='5520'",[date],|r|r.get(0)).unwrap();
        assert_eq!(actual_native, native);
        let packet = crate::periods::compute(&tx, "2025-01-01", date, Ok(vec![])).unwrap();
        let control = packet["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|i| i["code"] == "DKK_CONTROL_ACCOUNTS")
            .unwrap();
        assert_eq!(control["status"], "passed", "{packet}");
        assert_eq!(control["count"], 0);
        assert_eq!(control["evidence"], json!([]));
        // Passing DKK control is deliberately narrower than a green close
        // packet: unmatched statement rows and other blockers remain visible.
        assert!(
            packet["items"]
                .as_array()
                .unwrap()
                .iter()
                .any(|i| i["status"] == "blocked" || i["status"] == "warning")
        );
    }
    assert!(crate::periods::verify_sources(&tx).unwrap().is_empty());
    tx.commit().unwrap();
}
