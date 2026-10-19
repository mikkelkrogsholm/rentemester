use super::*;
use crate::posting_rules::{self, Input};
use rusqlite::{Connection, TransactionBehavior};
fn fixture() -> Connection {
    fixture_with_bank(true)
}
fn fixture_with_bank(with_bank: bool) -> Connection {
    let mut db = Connection::open_in_memory().unwrap();
    let mut control = Connection::open_in_memory().unwrap();
    crate::fresh::initialize_empty_pair(&mut control, &mut db, "Synthetic purchase rules").unwrap();
    db.execute_batch("INSERT INTO accounts(account_no,name,type,normal_balance)VALUES('6000','Synthetic purchase expense','expense','debit'),('6001','Synthetic alternate expense','expense','debit');").unwrap();
    if with_bank {
        db.execute_batch("INSERT INTO bank_transactions(id,transaction_date,text,amount,currency)VALUES(1,'2026-10-18','Synthetic supplier debit',-125,'DKK'),(2,'2026-10-18','Synthetic supplier debit',-125,'DKK')").unwrap();
    }
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    for n in 1..=2 {
        let metadata:document::Metadata=serde_json::from_value(json!({"source":"synthetic-rule-test","documentType":"purchase_sale","issueDate":"2026-10-18","deliveryDescription":"Synthetic supplies","invoiceNo":format!("SYN-{n}"),"amountIncVat":125,"vatAmount":25,"sender":{"name":"Synthetic supplier","address":"Synthetic street 1","vatOrCvr":"DK12345678"},"recipient":{"name":"Synthetic buyer","address":"Synthetic street 2","vatOrCvr":"DK87654321"}})).unwrap();
        let h = format!("{n:064x}");
        crate::documents::register(
            &tx,
            crate::documents::Original {
                filename: "synthetic.pdf",
                hash: &h,
                path: &format!("documents/originals/{h}.pdf"),
                mime: "application/pdf",
            },
            &metadata,
            "user:author",
            "2026-10-19",
        )
        .unwrap();
    }
    tx.commit().unwrap();
    db
}
fn today(tx: &Transaction<'_>) -> String {
    tx.query_row("SELECT strftime('%Y-%m-%d','now')", [], |r| r.get(0))
        .unwrap()
}
fn proposal(id: &str, vat: bool) -> Input {
    let mut outcome = json!({"account":"6000"});
    if vat {
        outcome["vatTreatment"] = json!("standard");
    }
    serde_json::from_value(json!({"confirm":true,"proposal":{"ruleId":id,"effectiveFrom":"2000-01-01","conditions":{"currency":"DKK","attributes":{"bankText":"Synthetic supplier debit"},"supplierIdentity":"Synthetic supplier","supplierCountry":"DK","supplierVat":"DK12345678","documentType":"purchase_sale","vat":"positive"},"outcome":outcome,"provenance":"synthetic reviewed source","rationale":"Synthetic reviewed purchase allocation"}})).unwrap()
}
fn approved_rule(tx: &Transaction<'_>, id: &str, vat: bool) {
    let day = today(tx);
    let created =
        posting_rules::change(tx, "propose", &proposal(id, vat), "user:author", &day).unwrap();
    let review:Input=serde_json::from_value(json!({"confirm":true,"ruleId":id,"version":1,"expectedPayloadHash":created["payloadHash"],"rationale":"Independent synthetic review","provenance":"synthetic reviewed source"})).unwrap();
    assert!(posting_rules::change(tx, "approve", &review, "user:author", &day).is_err());
    posting_rules::change(tx, "approve", &review, "user:reviewer", &day).unwrap();
}
#[test]
fn two_purchase_proofs_real_application_ids_exact_duplicate_and_no_ledger_effects() {
    let mut db = fixture();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    approved_rule(&tx, "synthetic-purchases", true);
    let day = today(&tx);
    for n in 1..=2 {
        let proof = inspect(&tx, n, n, &day).unwrap();
        assert_eq!(proof["ok"], true, "{proof}");
        assert_eq!(inspect(&tx, n, n, &day).unwrap(), proof);
        assert_eq!(
            proof["outcome"],
            json!({"account":"6000","vatTreatment":"standard","dimensions":{}})
        );
        let key = format!("synthetic-purchase-{n}");
        let receipt = apply(&tx, n, n, &day, &proof, &key, "user:poster").unwrap();
        assert!(receipt["postingRuleApplicationId"].as_i64().unwrap() > 0);
        assert_eq!(receipt["duplicate"], false);
        let replay = apply(&tx, n, n, &day, &proof, &key, "user:poster").unwrap();
        assert_eq!(
            replay["postingRuleApplicationId"],
            receipt["postingRuleApplicationId"]
        );
        assert_eq!(replay["duplicate"], true);
    }
    assert_eq!(
        tx.query_row("SELECT count(*) FROM posting_rule_applications", [], |r| {
            r.get::<_, i64>(0)
        })
        .unwrap(),
        2
    );
    assert_eq!(
        tx.query_row("SELECT count(*) FROM journal_entries", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    tx.commit().unwrap();
}
#[test]
fn old_hints_unsupported_ambiguous_unapproved_and_stale_source_never_apply() {
    let mut db = fixture();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let day = today(&tx);
    let old:Input=serde_json::from_value(json!({"confirm":true,"proposal":{"ruleId":"old-hint","effectiveFrom":"2000-01-01","conditions":{"currency":"DKK","attributes":{"bankText":"Synthetic supplier debit"}},"outcome":{"account":"6000"},"provenance":"synthetic","rationale":"Synthetic old bank hint"}})).unwrap();
    let c = posting_rules::change(&tx, "propose", &old, "user:author", &day).unwrap();
    let r:Input=serde_json::from_value(json!({"confirm":true,"ruleId":"old-hint","version":1,"expectedPayloadHash":c["payloadHash"],"rationale":"Independent old hint review","provenance":"synthetic"})).unwrap();
    posting_rules::change(&tx, "approve", &r, "user:reviewer", &day).unwrap();
    assert_eq!(
        posting_rules::hint(&tx, "Synthetic supplier debit", "DKK", &day).unwrap(),
        Some("6000".into())
    );
    assert_eq!(inspect(&tx, 1, 1, &day).unwrap()["ok"], false);
    approved_rule(&tx, "real-purchase", true);
    let proof = inspect(&tx, 1, 1, &day).unwrap();
    assert_eq!(proof["ok"], true);
    tx.execute("UPDATE documents SET status='processed' WHERE id=1", [])
        .unwrap();
    assert!(apply(&tx, 1, 1, &day, &proof, "stale", "user:poster").is_err());
    approved_rule(&tx, "conflicting-purchase", true);
    let blocked = inspect(&tx, 2, 2, &day).unwrap();
    assert_eq!(blocked["decision"], "human_decision");
    assert!(apply(&tx, 2, 2, &day, &blocked, "ambiguous", "user:poster").is_err());
    assert_eq!(
        tx.query_row("SELECT count(*) FROM posting_rule_applications", [], |r| {
            r.get::<_, i64>(0)
        })
        .unwrap(),
        0
    );
    tx.rollback().unwrap();
}
#[test]
fn rule_disable_key_conflict_and_outer_rollback_preserve_append_only_proofs() {
    let mut db = fixture();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    approved_rule(&tx, "reviewed", true);
    let day = today(&tx);
    let proof = inspect(&tx, 1, 1, &day).unwrap();
    apply(&tx, 1, 1, &day, &proof, "one-key", "user:poster").unwrap();
    let other = inspect(&tx, 2, 2, &day).unwrap();
    assert!(apply(&tx, 2, 2, &day, &other, "one-key", "user:poster").is_err());
    tx.rollback().unwrap();
    assert_eq!(
        db.query_row("SELECT count(*) FROM posting_rule_applications", [], |r| {
            r.get::<_, i64>(0)
        })
        .unwrap(),
        0
    );
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    approved_rule(&tx, "reviewed", true);
    let proof = inspect(&tx, 1, 1, &day).unwrap();
    let disable:Input=serde_json::from_value(json!({"confirm":true,"ruleId":"reviewed","version":1,"expectedPayloadHash":proof["payloadHash"],"rationale":"Explicit synthetic rule retirement","provenance":"synthetic"})).unwrap();
    posting_rules::change(&tx, "disable", &disable, "user:reviewer", &day).unwrap();
    assert!(apply(&tx, 1, 1, &day, &proof, "disabled", "user:poster").is_err());
    assert_eq!(inspect(&tx, 1, 1, &day).unwrap()["ok"], false);
    tx.rollback().unwrap();
}

#[test]
fn approved_unsupported_legacy_family_blocks_fallback_to_otherwise_matching_native_rule() {
    let mut db = fixture();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    approved_rule(&tx, "native-reviewed", true);
    let day = today(&tx);
    assert_eq!(inspect(&tx, 1, 1, &day).unwrap()["ok"], true);
    // Model retained immutable legacy evidence. The amount-rule evaluator is outside this slice;
    // it must not be silently ignored in favour of another automatic rule.
    let h = "b".repeat(64);
    let at = format!("{day}T00:00:00.000Z");
    let id:i64=tx.query_row("INSERT INTO posting_rule_versions(rule_id,company_id,version,effective_from,conditions_json,outcome_json,provenance,rationale,created_by,payload_hash,created_at) VALUES('legacy-amount',1,1,'2000-01-01',?1,?2,'synthetic','Synthetic retained legacy rule','user:legacy-author',?3,?4) RETURNING id",params![json!({"company":1,"currency":"DKK","amount":{"min":1}}).to_string(),json!({"account":"6001","vatTreatment":"standard"}).to_string(),h,at],|r|r.get(0)).unwrap();
    for (state, actor, approver) in [
        ("proposed", "user:legacy-author", None),
        (
            "approved",
            "user:legacy-reviewer",
            Some("user:legacy-reviewer"),
        ),
    ] {
        tx.execute("INSERT INTO posting_rule_lifecycle_events(rule_version_id,state,effective_at,actor,approver,rationale,provenance,expected_payload_hash,created_at)VALUES(?1,?2,?3,?4,?5,'Synthetic retained legacy review','synthetic',?6,?3)",params![id,state,at,actor,approver,h]).unwrap();
    }
    let proof = inspect(&tx, 1, 1, &day).unwrap();
    assert_eq!(proof["ok"], false);
    assert!(
        proof["explanation"]["reasons"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s.as_str().unwrap().contains("no fallback"))
    );
    assert!(apply(&tx, 1, 1, &day, &proof, "unsupported", "user:poster").is_err());
    tx.rollback().unwrap();
}

fn replace_immutable(db: &Connection, table: &str, statement: &str) {
    let triggers = db
        .prepare("SELECT name,sql FROM sqlite_schema WHERE type='trigger' AND tbl_name=?1")
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
    db.execute_batch(statement).unwrap();
    for (_, ddl) in &triggers {
        db.execute_batch(ddl).unwrap();
    }
}
#[test]
fn backup_preserves_historical_disabled_rule_application_and_refuses_corrupt_proof() {
    let mut db = fixture();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    approved_rule(&tx, "historical", true);
    let day = today(&tx);
    let proof = inspect(&tx, 1, 1, &day).unwrap();
    apply(&tx, 1, 1, &day, &proof, "historical-proof", "user:poster").unwrap();
    let disable: Input = serde_json::from_value(json!({"confirm":true,"ruleId":"historical","version":1,"expectedPayloadHash":proof["payloadHash"],"rationale":"Later explicit disable","provenance":"synthetic"})).unwrap();
    posting_rules::change(&tx, "disable", &disable, "user:reviewer", &day).unwrap();
    tx.commit().unwrap();
    // Current account activity is not historical authority.
    db.execute("UPDATE accounts SET active=0 WHERE account_no='6000'", [])
        .unwrap();
    let before = db.total_changes();
    validate_backup(&db).unwrap();
    assert_eq!(db.total_changes(), before);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("rules.sqlite");
    db.execute("VACUUM INTO ?1", [path.to_str().unwrap()])
        .unwrap();
    drop(db);
    let db = Connection::open(path).unwrap();
    validate_backup(&db).unwrap();
    replace_immutable(
        &db,
        "posting_rule_applications",
        "UPDATE posting_rule_applications SET explanation_json=json_set(explanation_json,'$.outcome.account','6001')",
    );
    assert!(validate_backup(&db).is_err());
    assert_eq!(
        crate::portable_backup::company(&db).err().unwrap().code,
        "purchase_rule_refused"
    );
}
#[test]
fn backup_refuses_canonical_rule_and_lifecycle_corruption_even_without_applications() {
    for (table, sql) in [
        (
            "posting_rule_versions",
            "UPDATE posting_rule_versions SET outcome_json='{\"account\":\"6001\",\"vatTreatment\":\"standard\"}'",
        ),
        (
            "posting_rule_lifecycle_events",
            "UPDATE posting_rule_lifecycle_events SET actor='user:author',approver='user:author' WHERE state='approved'",
        ),
    ] {
        let mut db = fixture();
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        approved_rule(&tx, "reviewed", true);
        tx.commit().unwrap();
        validate_backup(&db).unwrap();
        replace_immutable(&db, table, sql);
        assert!(validate_backup(&db).is_err(), "{table}");
    }
}

#[test]
fn complete_atomic_two_purchase_inventory_passes_shared_backup_validator_after_reopen() {
    use crate::bookkeeping_batch as batch;
    use rentemester_engine::bookkeeping_batch::{Exact, Persist, Scope};
    let mut db = fixture_with_bank(false);
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    tx.execute_batch("UPDATE companies SET vat_period_type='quarter';
        INSERT INTO accounts(account_no,name,type,normal_balance)VALUES('A','Synthetic bank','asset','debit'),('V','Synthetic VAT','vat','debit');
        INSERT INTO account_role_mappings(role,account_no,status,version,confirmed_by)VALUES('input_vat','V','confirmed',1,'synthetic');
        INSERT INTO bank_accounts(slug,name,ledger_account_no)VALUES('synthetic','Synthetic bank','A');").unwrap();
    let csv = "date;text;amount\n2026-10-18;Synthetic supplier debit SYN-1;-125\n2026-10-18;Synthetic supplier debit SYN-2;-125\n";
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
    let mut rule = proposal("backup-batch", true);
    rule.proposal.as_mut().unwrap().conditions.attributes = None;
    let day = today(&tx);
    let created = posting_rules::change(&tx, "propose", &rule, "user:author", &day).unwrap();
    let review:Input=serde_json::from_value(json!({"confirm":true,"ruleId":"backup-batch","version":1,"expectedPayloadHash":created["payloadHash"],"rationale":"Independent synthetic backup review","provenance":"synthetic"})).unwrap();
    posting_rules::change(&tx, "approve", &review, "user:reviewer", &day).unwrap();
    let scope:Scope=serde_json::from_value(json!({"accountingFrom":"2026-10-01","accountingTo":"2026-10-31","bankFrom":"2026-10-01","bankTo":"2026-10-31"})).unwrap();
    let latest = Date::parse("2026-12-01").unwrap();
    let plan = batch::plan(&tx, &scope, latest, &day).unwrap();
    assert_eq!(
        plan["items"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|i| i["partition"] == "ready")
            .count(),
        2,
        "{plan}"
    );
    let persist:Persist=serde_json::from_value(json!({"accountingFrom":"2026-10-01","accountingTo":"2026-10-31","bankFrom":"2026-10-01","bankTo":"2026-10-31","runKey":"synthetic-backup-batch","expectedPlanHash":plan["planHash"],"confirm":true})).unwrap();
    let run = batch::persist(&tx, &persist, "author", latest, &day).unwrap();
    let exact = Exact {
        run_id: run["runId"].as_i64().unwrap(),
        plan_hash: plan["planHash"].as_str().unwrap().into(),
        confirm: true,
    };
    batch::approve(&tx, &exact, "reviewer", latest, &day).unwrap();
    batch::apply(&tx, &exact, "poster", latest, &day).unwrap();
    tx.commit().unwrap();
    let before = db.total_changes();
    crate::portable_backup::company(&db).unwrap();
    assert_eq!(before, db.total_changes());
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("complete-batch.sqlite");
    db.execute("VACUUM INTO ?1", [path.to_str().unwrap()])
        .unwrap();
    drop(db);
    let db = Connection::open(path).unwrap();
    crate::portable_backup::company(&db).unwrap();
    assert_eq!(
        db.query_row("SELECT count(*) FROM journal_entries", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        2
    );
    replace_immutable(
        &db,
        "posting_rule_applications",
        "UPDATE posting_rule_applications SET explanation_json=json_set(explanation_json,'$.explanation.reasons',json('[\"changed\"]')) WHERE id=1",
    );
    assert!(crate::portable_backup::company(&db).is_err());
}
