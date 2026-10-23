//! #696 canonical foreign workbench, admitted-clock and atomic effects proofs.
use crate::expense;
use rentemester_engine::{document, fiscal_year::Date, vies};
use rusqlite::{Connection, Transaction, TransactionBehavior};
use serde_json::json;

fn fixture() -> Connection {
    let mut control = Connection::open_in_memory().unwrap();
    let mut company = Connection::open_in_memory().unwrap();
    crate::fresh::initialize_empty_pair(&mut control, &mut company, "Synthetic foreign clock")
        .unwrap();
    let tx = company
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    tx.execute_batch("UPDATE companies SET vat_period_type='quarter';
        INSERT INTO accounts(account_no,name,type,normal_balance) VALUES
        ('A','Synthetic bank','asset','debit'),('X','Synthetic expense','expense','debit'),
        ('V','Synthetic input VAT','vat','debit'),('R','Synthetic output VAT','vat','credit');
        INSERT INTO account_role_mappings(role,account_no,status,version,confirmed_by) VALUES
        ('input_vat','V','confirmed',1,'synthetic'),('reverse_charge_vat','R','confirmed',1,'synthetic');
        INSERT INTO bank_accounts(slug,name,ledger_account_no) VALUES('synthetic','Synthetic bank','A');").unwrap();
    let meta: document::Metadata = serde_json::from_value(json!({
        "source":"synthetic", "documentType":"purchase_sale", "issueDate":"2026-10-18",
        "deliveryDescription":"Synthetic foreign services", "invoiceNo":"SYN-EU-696",
        "amountIncVat":100, "vatAmount":0, "currency":"EUR",
        "sender":{"name":"Synthetic EU supplier","address":"Synthetic street 1","vatOrCvr":"DE123456789","countryCode":"DE","identifierKind":"eu_vat"},
        "recipient":{"name":"Synthetic buyer","address":"Synthetic street 2","vatOrCvr":"DK87654321"}
    })).unwrap();
    crate::documents::register(
        &tx,
        crate::documents::Original {
            filename: "synthetic.txt",
            hash: &"e".repeat(64),
            path: "synthetic-original",
            mime: "text/plain",
        },
        &meta,
        "synthetic",
        "2026-10-18",
    )
    .unwrap();
    let csv = "date;text;amount\n2026-10-18;Synthetic EU supplier SYN-EU-696;-746.25\n";
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
fn input() -> expense::Input {
    serde_json::from_value(json!({"documentId":1,"bankTransactionId":1,"expenseAccountNo":"X","vatTreatment":"reverse_charge"})).unwrap()
}
fn cache(tx: &Transaction<'_>, at: &str, valid: bool) {
    let record = vies::record(&json!({"countryCode":"DE","vatNumber": "DE123456789".strip_prefix("DE").unwrap(),"normalized":"DE123456789","valid":valid,"name":null,"address":null,"rawResponse":"{\"reference\":\"synthetic696\"}"}), at).unwrap();
    crate::vies::store(tx, &record, "user:synthetic").unwrap();
}
fn no_effects(tx: &Transaction<'_>) {
    for table in [
        "journal_entries",
        "journal_lines",
        "bank_journal_reconciliations",
        "purchase_posting_applications",
        "posting_rule_applications",
        "vat_validation_events",
    ] {
        assert_eq!(
            tx.query_row::<i64, _, _>(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
                .unwrap(),
            0,
            "{table}"
        );
    }
}

use super::{apply_at, approve_at, persist_at, plan_at};
use rentemester_engine::bookkeeping_batch::{Exact, Persist, Scope};
use serde_json::Value;
const NOW: &str = "2090-01-02T12:00:00Z";
const DAY: &str = "2090-01-02";
fn horizon() -> Date {
    Date::parse("2090-02-16").unwrap()
}
fn scope() -> Scope {
    serde_json::from_value(json!({"accountingFrom":"2026-10-01","accountingTo":"2026-10-31","bankFrom":"2026-10-01","bankTo":"2026-10-31"})).unwrap()
}
fn reviewed(tx: &Transaction<'_>) -> (Exact, Value) {
    let currency: String = tx
        .query_row("SELECT currency FROM documents WHERE id=1", [], |r| {
            r.get(0)
        })
        .unwrap();
    let proposal = serde_json::from_value(json!({"confirm":true,"proposal":{"ruleId":"synthetic-696","effectiveFrom":"2020-01-01","conditions":{"currency":currency,"supplierCountry":"DE","supplierVat":"DE123456789","documentType":"purchase_sale","vat":"zero"},"outcome":{"account":"X","vatTreatment":"reverse_charge"},"provenance":"synthetic source","rationale":"Synthetic reviewed foreign purchase"}})).unwrap();
    let proposed =
        crate::posting_rules::change(tx, "propose", &proposal, "user:rule-author", DAY).unwrap();
    let review = serde_json::from_value(json!({"confirm":true,"ruleId":"synthetic-696","version":1,"expectedPayloadHash":proposed["payloadHash"],"rationale":"Independent source review","provenance":"synthetic source"})).unwrap();
    crate::posting_rules::change(tx, "approve", &review, "user:rule-reviewer", DAY).unwrap();
    let p = plan_at(tx, &scope(), horizon(), DAY, NOW).unwrap();
    assert_eq!(p["items"][0]["partition"], "ready", "{p}");
    assert_eq!(
        p,
        plan_at(tx, &scope(), horizon(), DAY, "2090-01-02T13:00:00Z").unwrap(),
        "moving current time must not enter stable hash"
    );
    let persist:Persist=serde_json::from_value(json!({"accountingFrom":"2026-10-01","accountingTo":"2026-10-31","bankFrom":"2026-10-01","bankTo":"2026-10-31","runKey":"synthetic-696","expectedPlanHash":p["planHash"],"confirm":true})).unwrap();
    let saved = persist_at(tx, &persist, "planner", horizon(), DAY, NOW).unwrap();
    let exact = Exact {
        run_id: saved["runId"].as_i64().unwrap(),
        plan_hash: p["planHash"].as_str().unwrap().into(),
        confirm: true,
    };
    assert!(approve_at(tx, &exact, "planner", horizon(), DAY, NOW).is_err());
    approve_at(tx, &exact, "independent-reviewer", horizon(), DAY, NOW).unwrap();
    (exact, p)
}
fn assert_applied(tx: &Transaction<'_>, exact: &Exact, expected_gross: &str, expected_vat: &str) {
    let result = apply_at(tx, exact, "poster", horizon(), DAY, NOW).unwrap();
    assert_eq!(result["ok"], true, "{result}");
    let receipt = &result["results"][0]["receipt"];
    for (key, table) in [
        ("journalEntryId", "journal_entries"),
        ("vatPreflightId", "vat_validation_events"),
        ("purchaseApplicationId", "purchase_posting_applications"),
        ("postingRuleApplicationId", "posting_rule_applications"),
    ] {
        assert_eq!(
            tx.query_row::<i64, _, _>(
                &format!("SELECT count(*) FROM {table} WHERE id=?1"),
                [receipt[key].as_i64().unwrap()],
                |r| r.get(0)
            )
            .unwrap(),
            1
        );
        assert_eq!(
            tx.query_row::<i64, _, _>(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
                .unwrap(),
            1
        );
    }
    let lines=super::rows(tx,"SELECT a.account_no,jl.debit_amount,jl.credit_amount,jl.currency FROM journal_lines jl JOIN accounts a ON a.id=jl.account_id ORDER BY a.account_no",[]).unwrap();
    assert_eq!(lines.len(), 4, "{lines:?}");
    for line in lines {
        assert_eq!(line["currency"], "DKK");
        let (debit, credit) = match line["account_no"].as_str().unwrap() {
            "A" => ("0", expected_gross),
            "X" => (expected_gross, "0"),
            "V" => (expected_vat, "0"),
            "R" => ("0", expected_vat),
            _ => panic!("unexpected account"),
        };
        assert_eq!(
            super::money(&line["debit_amount"]).unwrap(),
            rentemester_engine::money::Money::parse(debit).unwrap()
        );
        assert_eq!(
            super::money(&line["credit_amount"]).unwrap(),
            rentemester_engine::money::Money::parse(credit).unwrap()
        );
    }
    let vat:(String,String,String)=tx.query_row("SELECT supplier_country_code,classification,provider_status FROM vat_validation_events",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap();
    assert_eq!(vat, ("DE".into(), "EU".into(), "valid".into()));
    let replay = apply_at(tx, exact, "poster", horizon(), DAY, NOW).unwrap();
    assert_eq!(replay["duplicate"], true);
    assert_eq!(replay["results"][0]["receipt"], *receipt);
}
#[test]
fn workbench696_foreign_reviewed_atomic_apply() {
    let mut db = fixture();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    cache(&tx, "2090-01-01T12:00:00Z", true);
    let (exact, p) = reviewed(&tx);
    let vat = &p["items"][0]["detail"]["vat"];
    assert_eq!(vat["format"], "rentemester-native-purchase-vat/2");
    assert_eq!(vat["country"], "DE");
    assert_eq!(vat["providerReference"], "synthetic696");
    assert_eq!(vat["plan"]["grossAmount"], "100.00");
    assert_eq!(vat["plan"]["netAmount"], "746.25");
    assert_eq!(vat["plan"]["vatAmount"], "186.56");
    assert_eq!(vat["plan"]["rateSource"], "derived_dkk_settlement");
    assert_applied(&tx, &exact, "746.25", "186.56");
    tx.commit().unwrap();
}
#[test]
fn workbench696_same_foreign_bank() {
    let mut db = fixture();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    tx.execute("UPDATE bank_transactions SET currency='EUR',amount=-100,amount_dkk=-746,fx_rate_to_dkk=7.46",[]).unwrap();
    cache(&tx, "2090-01-01T12:00:00Z", true);
    let (exact, p) = reviewed(&tx);
    assert_eq!(
        p["items"][0]["detail"]["vat"]["plan"]["rateSource"],
        "imported_bank"
    );
    assert_applied(&tx, &exact, "746", "186.50");
}
#[test]
fn workbench696_foreign_dkk_supplier() {
    let mut db = fixture();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let raw: String = tx
        .query_row("SELECT payload_json FROM documents WHERE id=1", [], |r| {
            r.get(0)
        })
        .unwrap();
    let mut meta: Value = serde_json::from_str(&raw).unwrap();
    meta["currency"] = json!("DKK");
    tx.execute(
        "UPDATE documents SET currency='DKK',payload_json=?1",
        [meta.to_string()],
    )
    .unwrap();
    tx.execute("UPDATE bank_transactions SET amount=-100", [])
        .unwrap();
    cache(&tx, "2090-01-01T12:00:00Z", true);
    let (exact, p) = reviewed(&tx);
    assert_eq!(p["items"][0]["detail"]["vat"]["classification"], "EU");
    assert_applied(&tx, &exact, "100", "25");
}
#[test]
fn workbench696_current_source_invalidation() {
    let mut db = fixture();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    cache(&tx, "2090-01-01T12:00:00Z", true);
    let (exact, _) = reviewed(&tx);
    tx.execute(
        "UPDATE bank_transactions SET message='changed immutable source'",
        [],
    )
    .unwrap();
    assert!(apply_at(&tx, &exact, "poster", horizon(), DAY, NOW).is_err());
    no_effects(&tx);
}
#[test]
fn workbench696_missing_stale_negative_fx_and_rollback() {
    let mut db = fixture();
    {
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        assert!(expense::prepare_at(&tx, &input(), NOW).is_err());
        cache(&tx, "2090-01-01T12:00:00Z", false);
        assert!(expense::prepare_at(&tx, &input(), NOW).is_err());
        cache(&tx, "2089-01-01T12:00:00Z", true);
        assert!(expense::prepare_at(&tx, &input(), NOW).is_err());
        cache(&tx, "2090-01-01T12:00:00Z", true);
        tx.execute("UPDATE bank_transactions SET fx_rate_to_dkk=7", [])
            .unwrap();
        assert!(expense::prepare_at(&tx, &input(), NOW).is_err());
        no_effects(&tx);
        tx.rollback().unwrap();
    }
    {
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        cache(&tx, "2090-01-01T12:00:00Z", true);
        let (exact, _) = reviewed(&tx);
        tx.execute_batch("CREATE TEMP TRIGGER synthetic_rollback BEFORE INSERT ON bookkeeping_batch_final_checks_v2 BEGIN SELECT RAISE(ABORT,'synthetic final gate failure'); END;").unwrap();
        assert!(apply_at(&tx, &exact, "poster", horizon(), DAY, NOW).is_err());
        tx.rollback().unwrap();
    }
    let tx = db.transaction().unwrap();
    no_effects(&tx);
}
#[test]
fn workbench696_cache_expiry_rechecked_without_hashing_now() {
    let mut db = fixture();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    cache(&tx, "2090-01-01T12:00:00Z", true);
    let (exact, _) = reviewed(&tx);
    // Even an already-approved revision must revalidate current expiry.
    assert!(
        approve_at(
            &tx,
            &exact,
            "independent-reviewer",
            horizon(),
            DAY,
            "2090-05-01T12:00:00Z"
        )
        .is_err()
    );
    assert!(
        apply_at(
            &tx,
            &exact,
            "poster",
            horizon(),
            DAY,
            "2090-05-01T12:00:00Z"
        )
        .is_err()
    );
    no_effects(&tx);
    cache(&tx, "2090-01-02T11:00:00Z", true);
    assert!(apply_at(&tx, &exact, "poster", horizon(), DAY, NOW).is_err());
    no_effects(&tx);
}
#[test]
fn workbench696_wording_is_typed_exact_and_missing_fx_stays_visible() {
    let mut db = fixture();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    cache(&tx, "2090-01-01T12:00:00Z", true);
    let (_, p) = reviewed(&tx);
    // The optional condition must distinguish absent from explicitly false.
    let conditions: crate::posting_rules::Conditions =
        serde_json::from_value(json!({"currency":"EUR","reverseChargeWording":false})).unwrap();
    assert_eq!(conditions.reverse_charge_wording, Some(false));
    let proposal = serde_json::from_value(json!({"confirm":true,"proposal":{"ruleId":"synthetic-wording","effectiveFrom":"2020-01-01","conditions":{"currency":"EUR","reverseChargeWording":false},"outcome":{"account":"X","vatTreatment":"reverse_charge"},"provenance":"synthetic","rationale":"Exact immutable wording"}})).unwrap();
    let proposed =
        crate::posting_rules::change(&tx, "propose", &proposal, "user:author", DAY).unwrap();
    let review=serde_json::from_value(json!({"confirm":true,"ruleId":"synthetic-wording","version":1,"expectedPayloadHash":proposed["payloadHash"],"rationale":"independent","provenance":"synthetic"})).unwrap();
    crate::posting_rules::change(&tx, "approve", &review, "user:reviewer", DAY).unwrap();
    let rule = crate::purchase_rules::inspect(&tx, 1, 1, DAY).unwrap();
    assert_eq!(rule["decision"], "proposed"); // only original rule, not absent==false
    assert!(
        rule["explanation"]["rules"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["reasons"]
                .as_array()
                .unwrap()
                .iter()
                .any(|s| s.as_str().unwrap().contains("reverseChargeWording")))
    );
    tx.execute("UPDATE bank_transactions SET currency='EUR',amount=-100,amount_dkk=NULL,fx_rate_to_dkk=NULL",[]).unwrap();
    let current = plan_at(&tx, &scope(), horizon(), DAY, NOW).unwrap();
    assert_eq!(current["items"][0]["documentId"], 1);
    assert_eq!(current["items"][0]["detail"]["vatReady"], false);
    assert_ne!(current["planHash"], p["planHash"]);
    no_effects(&tx);
}
