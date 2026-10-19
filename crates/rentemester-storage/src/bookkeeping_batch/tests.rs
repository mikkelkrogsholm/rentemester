use super::*;
use rentemester_engine::document;
use rusqlite::{Connection, TransactionBehavior};
use std::cell::Cell;
thread_local! {static FAULT:Cell<u8>=const{Cell::new(0)};}
pub(super) fn fault(point: u8) -> Result<(), Error> {
    FAULT.with(|f| {
        if f.get() == point {
            Err(fail("synthetic private atomic fault"))
        } else if f.get() == 3 && point == 1 {
            std::process::exit(57)
        } else {
            Ok(())
        }
    })
}
fn horizon() -> Date {
    Date::parse("2026-12-01").unwrap()
}
fn today(tx: &Transaction<'_>) -> String {
    tx.query_row("SELECT strftime('%Y-%m-%d','now')", [], |r| r.get(0))
        .unwrap()
}
fn fixture() -> Connection {
    let mut control = Connection::open_in_memory().unwrap();
    let mut company = Connection::open_in_memory().unwrap();
    crate::fresh::initialize_empty_pair(&mut control, &mut company, "Synthetic batch purchases")
        .unwrap();
    let tx = company
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    tx.execute_batch("UPDATE companies SET vat_period_type='quarter' WHERE id=1;
            INSERT INTO accounts(account_no,name,type,normal_balance) VALUES('A','Synthetic bank','asset','debit'),('X','Synthetic expense','expense','debit'),('V','Synthetic VAT','vat','debit');
            INSERT INTO account_role_mappings(role,account_no,status,version,confirmed_by) VALUES('input_vat','V','confirmed',1,'synthetic');
            INSERT INTO bank_accounts(slug,name,ledger_account_no) VALUES('synthetic','Synthetic bank','A');").unwrap();
    for (number, hash, amount, vat) in [(1, "a", 125, 25), (2, "b", 250, 50)] {
        let meta:document::Metadata=serde_json::from_value(json!({"source":"synthetic","documentType":"purchase_sale","issueDate":"2026-10-18","deliveryDescription":"Synthetic supplies","invoiceNo":format!("SYN-{number}"),"amountIncVat":amount,"vatAmount":vat,"sender":{"name":"Synthetic supplier","address":"Synthetic street 1","vatOrCvr":"DK12345678"},"recipient":{"name":"Synthetic buyer","address":"Synthetic street 2","vatOrCvr":"DK87654321"}})).unwrap();
        crate::documents::register(
            &tx,
            crate::documents::Original {
                filename: "synthetic.txt",
                hash: &hash.repeat(64),
                path: "synthetic-original",
                mime: "text/plain",
            },
            &meta,
            "synthetic",
            "2026-10-18",
        )
        .unwrap();
    }
    let csv = "date;text;amount\n2026-10-18;Synthetic purchase SYN-1;-125\n2026-10-18;Synthetic purchase SYN-2;-250\n";
    crate::bank::import(
        &tx,
        csv,
        &rentemester_engine::bank::parse(csv).unwrap(),
        Some("synthetic"),
        "synthetic",
        "2026-10-18T12:00:00Z",
        "synthetic-bank-original",
    )
    .unwrap();
    tx.commit().unwrap();
    company
}

fn scope() -> Scope {
    Scope {
        accounting_from: "2026-10-01".into(),
        accounting_to: "2026-10-31".into(),
        bank_from: "2026-10-01".into(),
        bank_to: "2026-10-31".into(),
        company_id: None,
    }
}
fn prepare(db: &mut Connection) -> Exact {
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let day = today(&tx);
    let proposal:crate::posting_rules::Input=serde_json::from_value(json!({"confirm":true,"proposal":{"ruleId":"synthetic-batch-purchase","effectiveFrom":"2000-01-01","conditions":{"currency":"DKK","supplierIdentity":"Synthetic supplier","supplierCountry":"DK","supplierVat":"DK12345678","documentType":"purchase_sale","vat":"positive"},"outcome":{"account":"X","vatTreatment":"standard"},"provenance":"synthetic batch test","rationale":"Synthetic explicit allocation"}})).unwrap();
    let c = crate::posting_rules::change(&tx, "propose", &proposal, "user:author", &day).unwrap();
    let review=serde_json::from_value(json!({"confirm":true,"ruleId":"synthetic-batch-purchase","version":1,"expectedPayloadHash":c["payloadHash"],"rationale":"Independent synthetic review","provenance":"synthetic batch test"})).unwrap();
    crate::posting_rules::change(&tx, "approve", &review, "user:reviewer", &day).unwrap();
    let p = plan(&tx, &scope(), horizon(), &day).unwrap();
    assert_eq!(
        p["items"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|i| i["partition"] == "ready")
            .count(),
        2,
        "{p}"
    );
    let input:Persist=serde_json::from_value(json!({"accountingFrom":scope().accounting_from,"accountingTo":scope().accounting_to,"bankFrom":scope().bank_from,"bankTo":scope().bank_to,"runKey":"synthetic-batch","expectedPlanHash":p["planHash"],"confirm":true})).unwrap();
    let created = persist(&tx, &input, "author", horizon(), &day).unwrap();
    let exact = Exact {
        run_id: created["runId"].as_i64().unwrap(),
        plan_hash: p["planHash"].as_str().unwrap().into(),
        confirm: true,
    };
    assert!(approve(&tx, &exact, "author").is_err());
    approve(&tx, &exact, "reviewer").unwrap();
    tx.commit().unwrap();
    exact
}
fn count(db: &Connection, table: &str) -> i64 {
    db.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}
fn no_effects(db: &Connection) {
    for table in [
        "journal_entries",
        "journal_lines",
        "bank_journal_reconciliations",
        "posting_rule_applications",
        "vat_validation_events",
        "purchase_posting_applications",
        "bookkeeping_batch_item_receipts",
        "bookkeeping_batch_applied_links",
        "bookkeeping_batch_item_attempts",
        "bookkeeping_batch_apply_attempts_v2",
        "bookkeeping_batch_final_checks_v2",
    ] {
        assert_eq!(count(db, table), 0, "{table}")
    }
    assert_eq!(count(db, "bookkeeping_batch_revision_approvals"), 1)
}
#[test]
fn real_two_item_apply_replay_full_proofs_and_file_snapshot_reopen() {
    let mut db = fixture();
    let exact = prepare(&mut db);
    let tx = db.transaction().unwrap();
    let day = today(&tx);
    let out = apply(&tx, &exact, "poster", horizon(), &day).unwrap();
    assert_eq!(out["results"].as_array().unwrap().len(), 2);
    assert!(
        out["checks"]
            .as_array()
            .unwrap()
            .iter()
            .all(|c| c["ok"] == true),
        "{out}"
    );
    tx.commit().unwrap();
    assert_eq!(count(&db, "journal_entries"), 2);
    validate_backup(&db).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let snapshot = dir.path().join("snapshot.sqlite");
    db.execute("VACUUM INTO ?1", [snapshot.to_str().unwrap()])
        .unwrap();
    drop(db);
    let mut db = Connection::open(snapshot).unwrap();
    validate_backup(&db).unwrap();
    let before = db.total_changes();
    let tx = db.transaction().unwrap();
    let replay = apply(&tx, &exact, "new-current-owner", horizon(), &today(&tx)).unwrap();
    assert_eq!(replay["duplicate"], true);
    assert_eq!(tx.total_changes(), before);
    assert_eq!(
        status_by_key(&tx, "synthetic-batch").unwrap(),
        status(&tx, exact.run_id).unwrap()
    );
    tx.commit().unwrap();
    assert_eq!(count(&db, "journal_entries"), 2);
    // Simulate an independently signed corrupt snapshot without changing its physical schema.
    let triggers=db.prepare("SELECT name,sql FROM sqlite_schema WHERE type='trigger' AND tbl_name='bookkeeping_batch_item_receipts'").unwrap().query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?))).unwrap().collect::<Result<Vec<_>,_>>().unwrap();
    for (name, _) in &triggers {
        db.execute_batch(&format!("DROP TRIGGER \"{}\"", name.replace('"', "\"\"")))
            .unwrap();
    }
    db.execute("UPDATE bookkeeping_batch_item_receipts SET receipt_json=json_set(receipt_json,'$.evidenceHash',?1) WHERE id=1",["0".repeat(64)]).unwrap();
    for (_, ddl) in &triggers {
        db.execute_batch(ddl).unwrap();
    }
    assert!(
        validate_backup(&db).is_err(),
        "corrupt receipt must refuse before fresh publication"
    );
}
#[test]
fn second_item_and_final_check_faults_roll_back_entire_prefix() {
    for fault in [1, 2, 4] {
        let mut db = fixture();
        if fault == 4 {
            db.execute("INSERT INTO bank_transactions(transaction_date,text,amount,currency)VALUES('2026-10-20','Synthetic unresolved candidate',-10,'DKK')",[]).unwrap();
        }
        let exact = prepare(&mut db);
        let tx = db.transaction().unwrap();
        FAULT.with(|f| f.set(fault));
        let result = apply(&tx, &exact, "poster", horizon(), &today(&tx));
        FAULT.with(|f| f.set(0));
        assert!(result.is_err());
        tx.rollback().unwrap();
        no_effects(&db);
        validate_backup(&db).unwrap();
    }
}
#[test]
fn stale_universe_cas_legacy_inspection_and_filtered_population() {
    let mut db = fixture();
    let exact = prepare(&mut db);
    let tx = db.transaction().unwrap();
    let day = today(&tx);
    let input: wire::WorkbenchInput = serde_json::from_value(
        json!({"from":"2026-10-01","to":"2026-10-31","limit":1,"status":"missingDocument"}),
    )
    .unwrap();
    let w = workbench(
        &tx,
        &input,
        horizon(),
        &day,
        json!({"status":"unavailable","blockers":null}),
    )
    .unwrap();
    assert_eq!(w["workbench"]["population"]["total"], 2);
    assert_eq!(w["workbench"]["page"]["total"], 0);
    tx.execute("INSERT INTO bank_transactions(transaction_date,text,amount,currency) VALUES('2026-10-20','Synthetic new evidence',-10,'DKK')",[]).unwrap();
    assert!(apply(&tx, &exact, "poster", horizon(), &day).is_err());
    let input:Persist=serde_json::from_value(json!({"accountingFrom":"2026-10-01","accountingTo":"2026-10-31","bankFrom":"2026-10-01","bankTo":"2026-10-31","runKey":"must-not-exist","expectedPlanHash":exact.plan_hash,"confirm":true})).unwrap();
    assert!(persist(&tx, &input, "author", horizon(), &day).is_err());
    assert_eq!(count(&tx, "bookkeeping_batch_runs"), 1);
    tx.rollback().unwrap();
    no_effects(&db);
    let tx = db.transaction().unwrap();
    let mut legacy = stored_plan(&tx, &exact).unwrap();
    legacy.as_object_mut().unwrap().remove("executionPolicy");
    let json = canonical(&legacy);
    tx.execute("INSERT INTO bookkeeping_batch_runs(run_key,company_id,accounting_from,accounting_to,bank_from,bank_to,plan_hash,plan_json,created_at) VALUES('legacy',1,'2026-10-01','2026-10-31','2026-10-01','2026-10-31',?1,?2,'2026-10-19')",params![exact.plan_hash,json]).unwrap();
    let id = tx.last_insert_rowid();
    tx.execute("INSERT INTO bookkeeping_batch_revisions(run_id,revision_hash,candidate_set_hash,plan_json,planner_kind,planner_subject_id,planner_actor,created_at) VALUES(?1,?2,?2,?3,'user','author','user:author','2026-10-19')",params![id,exact.plan_hash,json]).unwrap();
    assert_eq!(status(&tx, id).unwrap()["run"]["plan"], json);
    assert!(
        stored_plan(
            &tx,
            &Exact {
                run_id: id,
                plan_hash: exact.plan_hash.clone(),
                confirm: true
            }
        )
        .is_err()
    );
    tx.commit().unwrap();
    validate_backup(&db).unwrap();
}
#[test]
fn crash_child() {
    let Some(path) = std::env::var_os("RM_BATCH57_CRASH_DB") else {
        return;
    };
    let mut db = Connection::open(path).unwrap();
    let tx = db.transaction().unwrap();
    let state = status_by_key(&tx, "synthetic-batch").unwrap();
    let exact = Exact {
        run_id: state["run"]["runId"].as_i64().unwrap(),
        plan_hash: state["run"]["planHash"].as_str().unwrap().into(),
        confirm: true,
    };
    FAULT.with(|f| f.set(3));
    let _ = apply(&tx, &exact, "poster", horizon(), &today(&tx));
    panic!("child did not reach first item crash boundary")
}
#[test]
fn process_death_before_outer_commit_leaves_no_prefix() {
    let mut db = fixture();
    prepare(&mut db);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("crash.sqlite");
    db.execute("VACUUM INTO ?1", [path.to_str().unwrap()])
        .unwrap();
    drop(db);
    let out = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "bookkeeping_batch::tests::crash_child",
            "--nocapture",
        ])
        .env("RM_BATCH57_CRASH_DB", &path)
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(57),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    let db = Connection::open(path).unwrap();
    no_effects(&db);
    validate_backup(&db).unwrap();
}
#[test]
fn two_real_connections_concurrent_approval_and_apply_commit_one_outcome() {
    let mut db = fixture();
    let exact = prepare(&mut db);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("concurrent.sqlite");
    db.execute("VACUUM INTO ?1", [path.to_str().unwrap()])
        .unwrap();
    drop(db);
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let mut threads = vec![];
    for _ in 0..2 {
        let path = path.clone();
        let barrier = barrier.clone();
        let exact = Exact {
            run_id: exact.run_id,
            plan_hash: exact.plan_hash.clone(),
            confirm: true,
        };
        threads.push(std::thread::spawn(move || {
            let mut db = Connection::open(path).unwrap();
            db.busy_timeout(std::time::Duration::from_secs(20)).unwrap();
            barrier.wait();
            let tx = db
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .unwrap();
            approve(&tx, &exact, "reviewer").unwrap();
            let result = apply(&tx, &exact, "poster", horizon(), &today(&tx)).unwrap();
            tx.commit().unwrap();
            result["duplicate"].as_bool().unwrap()
        }));
    }
    let results = threads
        .into_iter()
        .map(|t| t.join().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(results.iter().filter(|v| !**v).count(), 1);
    let db = Connection::open(path).unwrap();
    assert_eq!(count(&db, "journal_entries"), 2);
    assert_eq!(count(&db, "bookkeeping_batch_revision_approvals"), 1);
    assert_eq!(count(&db, "bookkeeping_batch_apply_attempts_v2"), 1);
    assert_eq!(count(&db, "bookkeeping_batch_item_receipts"), 2);
    validate_backup(&db).unwrap();
}
