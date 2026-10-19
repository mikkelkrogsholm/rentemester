use super::*;
use crate::posting_rules::{self, Input};
use rusqlite::{Connection, TransactionBehavior};
fn fixture() -> Connection {
    let mut db = Connection::open_in_memory().unwrap();
    let mut control = Connection::open_in_memory().unwrap();
    crate::fresh::initialize_empty_pair(&mut control, &mut db, "Synthetic purchase rules").unwrap();
    db.execute_batch("INSERT INTO accounts(account_no,name,type,normal_balance)VALUES('6000','Synthetic purchase expense','expense','debit'),('6001','Synthetic alternate expense','expense','debit');INSERT INTO bank_transactions(id,transaction_date,text,amount,currency)VALUES(1,'2026-10-18','Synthetic supplier debit',-125,'DKK'),(2,'2026-10-18','Synthetic supplier debit',-125,'DKK')").unwrap();
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
