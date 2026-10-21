use super::*;
use rusqlite::Connection;
fn fixture() -> Connection {
    let mut control = Connection::open_in_memory().unwrap();
    let mut db = Connection::open_in_memory().unwrap();
    crate::fresh::initialize_empty_pair(&mut control, &mut db, "Synthetic bank queries").unwrap();
    db.execute_batch("INSERT INTO accounts(id,account_no,name,type,normal_balance)VALUES(1,'B','Synthetic bank','asset','debit'),(2,'E','Synthetic equity','equity','credit');INSERT INTO bank_accounts(id,slug,name,ledger_account_no,currency)VALUES(1,'main','Synthetic bank','B','DKK'),(2,'foreign','Synthetic foreign bank',NULL,'EUR');").unwrap();
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    for (account, csv) in [
        (
            "main",
            "transaction_date,text,amount,currency,reference\n2026-04-01,Shared Receipt,12.34,DKK,secret-ref\n2026-04-02,Shared debit,-2.34,DKK,shared\n",
        ),
        (
            "foreign",
            "transaction_date,text,amount,currency,amount_dkk,fx_rate_to_dkk\n2025-12-31,Shared foreign,10.00,EUR,74.50,7.45\n",
        ),
    ] {
        bank::import(
            &tx,
            csv,
            &rentemester_engine::bank::parse(csv).unwrap(),
            Some(account),
            "synthetic",
            "2026-04-03T00:00:00Z",
            "bank/processed/synthetic.txt",
        )
        .unwrap();
    }
    tx.commit().unwrap();
    db.execute("UPDATE bank_accounts SET active=0 WHERE id=2", [])
        .unwrap();
    db
}
#[test]
fn bank_queries_original_units_all_year_filters_accounts_and_pages() {
    let mut db = fixture();
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    let all = list(&tx, &ListQuery::default()).unwrap();
    assert_eq!(all["data"]["count"], 3);
    assert!(all["data"].get("total").is_none());
    let rows = all["data"]["rows"].as_array().unwrap();
    assert_eq!(rows[0]["id"], 2);
    assert_eq!(rows[2]["amount"], 10);
    assert_eq!(rows[2]["currency"], "EUR");
    assert_eq!(rows[2].as_object().unwrap().len(), 13);
    assert!(rows[2]["bookingDate"].is_null());
    let mut q = ListQuery {
        limit: Some(1),
        offset: Some(1),
        ..ListQuery::default()
    };
    let page = list(&tx, &q).unwrap();
    assert_eq!(page["data"]["total"], 3);
    assert_eq!(page["data"]["rows"][0]["id"], 1);
    assert_eq!(page["data"]["nextOffset"], 2);
    q.offset = Some(9007199254740991);
    let end = list(&tx, &q).unwrap();
    assert_eq!(end["data"]["count"], 0);
    assert_eq!(end["data"]["hasMore"], false);
    assert!(end["data"].get("nextOffset").is_none());
    q = ListQuery {
        from: Some(" 2025-12-31 ".into()),
        to: Some("2025-12-31".into()),
        filters: Filters {
            account: Some(" foreign ".into()),
            text_match: Some(" SHARED ".into()),
            amount: Some("10.004".into()),
            status: Some("unmatched".into()),
        },
        ..ListQuery::default()
    };
    assert_eq!(list(&tx, &q).unwrap()["data"]["rows"][0]["id"], 3);
    q.filters.account = Some("2".into());
    assert_eq!(list(&tx, &q).unwrap()["data"]["count"], 1);
    q.filters.text_match = Some("secret-ref".into());
    assert_eq!(list(&tx, &q).unwrap()["data"]["count"], 0);
    let accounts = accounts(&tx, &AccountsQuery::default()).unwrap();
    assert_eq!(accounts["data"]["count"], 2);
    assert_eq!(accounts["data"]["accounts"][1]["active"], false);
    assert_eq!(
        accounts["data"]["accounts"][0].as_object().unwrap().len(),
        14
    );
    assert_eq!(
        super::accounts(
            &tx,
            &AccountsQuery {
                include_inactive: false
            }
        )
        .unwrap()["data"]["count"],
        1
    );
    let report = report(
        &tx,
        &ReportQuery {
            from: "2025-01-01".into(),
            to: "2026-12-31".into(),
            filters: Filters::default(),
        },
    )
    .unwrap();
    assert_eq!(report["data"]["unmatchedAmountTotal"], 20);
    assert_eq!(report["data"]["unmatched"][0]["bankTransactionId"], 3);
}
#[test]
fn bank_queries_business_refusals_inversion_and_unpaged_over_default_cap() {
    let mut db = fixture();
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    let mut q = ListQuery {
        filters: Filters {
            account: Some("absent".into()),
            ..Filters::default()
        },
        ..ListQuery::default()
    };
    let unknown = list(&tx, &q).unwrap();
    assert_eq!(unknown["code"], "NOT_FOUND");
    assert!(unknown.get("data").is_none());
    q.filters = Filters::default();
    q.from = Some("2026-02-30".into());
    assert_eq!(
        list(&tx, &q).unwrap()["errors"][0],
        "from must be YYYY-MM-DD when present"
    );
    q.from = Some("2026-05-01".into());
    q.to = Some("2026-04-01".into());
    assert_eq!(list(&tx, &q).unwrap()["data"]["count"], 0);
    let bad = report(
        &tx,
        &ReportQuery {
            from: q.from.unwrap(),
            to: q.to.unwrap(),
            filters: Filters::default(),
        },
    )
    .unwrap();
    assert_eq!(bad["ok"], false);
    assert_eq!(
        bad["errors"][0],
        "periodStart must be before or equal to periodEnd"
    );
    assert_eq!(bad["appliedRules"], json!([RULE]));
    let mut csv = "transaction_date,text,amount\n".to_owned();
    for i in 0..502 {
        csv.push_str(&format!("2024-01-01,Synthetic {i},0.01\n"));
    }
    bank::import(
        &tx,
        &csv,
        &rentemester_engine::bank::parse(&csv).unwrap(),
        None,
        "synthetic",
        "2026-04-03T00:00:00Z",
        "bank/processed/synthetic.txt",
    )
    .unwrap();
    assert_eq!(
        list(&tx, &ListQuery::default()).unwrap()["data"]["count"],
        505
    );
    let paged = list(
        &tx,
        &ListQuery {
            offset: Some(0),
            ..ListQuery::default()
        },
    )
    .unwrap();
    assert_eq!(paged["data"]["count"], 500);
    assert_eq!(paged["data"]["nextOffset"], 500);
    assert!(number(&Money::parse("9007199254740991.01").unwrap()).is_err());
}
#[test]
fn bank_queries_reversal_demotes_then_reviewed_correction_restores_match() {
    let mut db = fixture();
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    tx.execute_batch("INSERT INTO journal_entries(id,entry_no,transaction_date,text,source_bank_transaction_id,rule_version,entry_hash)VALUES(1,'SYN-1','2026-04-01','Original',1,'fixture','aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa');INSERT INTO journal_lines(journal_entry_id,account_id,debit_amount,credit_amount)VALUES(1,1,12.34,0),(1,2,0,12.34);").unwrap();
    let q = ListQuery {
        filters: Filters {
            status: Some("matched".into()),
            ..Filters::default()
        },
        ..ListQuery::default()
    };
    assert_eq!(list(&tx, &q).unwrap()["data"]["count"], 1);
    tx.execute_batch("INSERT INTO journal_entries(id,entry_no,transaction_date,text,status,reversal_of_entry_id,source_bank_transaction_id,rule_version,entry_hash)VALUES(2,'SYN-REV','2026-04-01','Reversal','reversed',1,1,'fixture','bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb');INSERT INTO journal_lines(journal_entry_id,account_id,debit_amount,credit_amount)VALUES(2,1,0,12.34),(2,2,12.34,0);INSERT INTO journal_entries(id,entry_no,transaction_date,text,rule_version,entry_hash)VALUES(3,'SYN-NEW','2026-04-01','Replacement','fixture','cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc');INSERT INTO journal_lines(journal_entry_id,account_id,debit_amount,credit_amount)VALUES(3,1,12.34,0),(3,2,0,12.34);").unwrap();
    assert_eq!(list(&tx, &q).unwrap()["data"]["count"], 0);
    let all = list(&tx, &ListQuery::default()).unwrap();
    assert!(all["data"]["rows"][1]["journalEntryId"].is_null());
    assert!(all["data"]["rows"][1]["journalEntryNo"].is_null());
    let plan = crate::bank_reconciliation::correction::plan(&tx, 1, 3).unwrap();
    let input = crate::bank_reconciliation::correction::Apply {
        bank_transaction_id: 1,
        replacement_journal_entry_id: 3,
        expected_reconciliation_id: plan["plan"]["reconciliationId"].as_str().unwrap().into(),
        plan_hash: plan["plan"]["planHash"].as_str().unwrap().into(),
        reason: "Synthetic reviewed replacement".into(),
        idempotency_key: "synthetic-bank-query-correction".into(),
        confirm: true,
    };
    crate::bank_reconciliation::correction::apply(&tx, &input, "synthetic", 1_780_000_000_000)
        .unwrap();
    let matched = list(&tx, &q).unwrap();
    assert_eq!(matched["data"]["rows"][0]["journalEntryId"], 3);
    let report = report(
        &tx,
        &ReportQuery {
            from: "2026-04-01".into(),
            to: "2026-04-02".into(),
            filters: Filters::default(),
        },
    )
    .unwrap();
    assert_eq!(report["data"]["matchedAmountTotal"], 12.34);
    assert_eq!(report["data"]["unmatchedAmountTotal"], -2.34);
}
