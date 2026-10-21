use super::*;
use crate::journal_post::{self, Actor};
use rentemester_engine::{fiscal_year::Date, identifiers::JournalEntryId};
use rusqlite::{Connection, TransactionBehavior};
fn fixture() -> Connection {
    let mut control = Connection::open_in_memory().unwrap();
    let mut db = Connection::open_in_memory().unwrap();
    crate::fresh::initialize_empty_pair(&mut control, &mut db, "Synthetic dated reports").unwrap();
    db.execute_batch("UPDATE companies SET fiscal_year_start_month=7,fiscal_year_label_strategy='span';INSERT INTO accounts(account_no,name,type,normal_balance)VALUES('A','Synthetic cash','asset','debit'),('E','Synthetic equity','equity','credit'),('I','Synthetic income','income','credit'),('X','Synthetic expense','expense','debit');").unwrap();
    db
}
fn actor() -> Actor<'static> {
    Actor {
        subject: "user:synthetic",
        program: "rentemester-rust",
    }
}
fn latest() -> Date {
    Date::parse("2026-02-15").unwrap()
}
fn post(tx: &Transaction<'_>, date: &str, debit: &str, credit: &str, amount: &str) -> i64 {
    // The real posting gate requires a retained document for income/expense.
    // Physical originals belong to the host barrier, exercised separately.
    let needs_source: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM accounts WHERE account_no IN (?1,?2) AND type IN ('income','expense'))",[debit,credit],|r|r.get(0)).unwrap();
    let document: Option<i64> = if needs_source {
        let hash = crate::invoice_email::sha(
            format!("Synthetic dated source {date} {debit} {credit} {amount}").as_bytes(),
        );
        Some(tx.query_row("INSERT INTO documents(source,document_type,sha256_hash,invoice_date,payload_json)VALUES('synthetic','purchase_sale',?1,?2,'{}') RETURNING id",rusqlite::params![hash,date],|r|r.get(0)).unwrap())
    } else {
        None
    };
    let request=serde_json::from_value(json!({"transactionDate":date,"text":"Synthetic dated transfer","currency":"DKK","documentId":document.map(|id|id.to_string()),"lines":[{"accountNo":debit,"debitAmount":amount},{"accountNo":credit,"creditAmount":amount}]})).unwrap();
    let result = journal_post::post(
        tx,
        &request,
        &crate::manual_journal_rule_version().unwrap(),
        latest(),
        actor(),
    )
    .unwrap();
    assert_eq!(result["ok"], true, "{result}");
    result["entryId"].as_i64().unwrap()
}
fn range(kind: Kind, from: &str, to: &str) -> Input {
    Input {
        kind,
        from: Some(from.into()),
        to: Some(to.into()),
        as_of: None,
    }
}
fn balance(date: &str) -> Input {
    Input {
        kind: Kind::Balance,
        from: None,
        to: None,
        as_of: Some(date.into()),
    }
}
#[test]
fn exact_cross_year_period_reversal_and_cumulative_equity_match_original_wire() {
    let mut db = fixture();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    post(&tx, "2025-06-30", "A", "E", "100.00");
    let sale = post(&tx, "2025-07-01", "A", "I", "80.00");
    post(&tx, "2026-01-01", "X", "A", "20.00");
    assert_eq!(
        crate::journal_reversal::reverse(
            &tx,
            &crate::journal_reversal::Input {
                entry_id: JournalEntryId::from_sqlite(sale),
                transaction_date: Date::parse("2026-01-02").unwrap(),
                reason: "Synthetic correction".into()
            },
            &crate::manual_journal_rule_version().unwrap(),
            latest(),
            actor()
        )
        .unwrap()["ok"],
        true
    );
    let changes = tx.total_changes();
    let tb = read(&tx, &range(Kind::TrialBalance, "2025-07-01", "2026-01-01")).unwrap();
    assert_eq!(
        tb["appliedRules"],
        json!(["financial-statements:trial-balance"])
    );
    assert_eq!(tb["totalDebit"], 100);
    assert_eq!(tb["totalCredit"], 100);
    assert_eq!(tb["linesConsidered"], 4);
    assert_eq!(tb["journalEntryCount"], 2);
    assert_eq!(tb["accounts"][0]["accountNo"], "A");
    assert_eq!(tb["accounts"][0]["normalBalance"], "debit");
    assert_eq!(tb["accounts"][0]["balance"], 60);
    let pnl = read(&tx, &range(Kind::ProfitLoss, "2025-07-01", "2026-01-01")).unwrap();
    assert_eq!(pnl["result"], 60);
    assert_eq!(pnl["income"][0]["type"], "income");
    assert_eq!(pnl["expense"][0]["type"], "expense");
    assert_eq!(
        read(&tx, &range(Kind::ProfitLoss, "2026-01-02", "2026-01-02")).unwrap()["result"],
        -80
    );
    let before = read(&tx, &balance("2026-01-01")).unwrap();
    assert_eq!(before["totalAssets"], 160);
    assert_eq!(before["equity"]["total"], 100);
    assert_eq!(before["equity"]["lines"].as_array().unwrap().len(), 1);
    assert_eq!(before["periodResult"], 60);
    assert_eq!(before["totalLiabilitiesAndEquity"], 160);
    assert_eq!(before["balanced"], true);
    let after = read(&tx, &balance("2026-01-02")).unwrap();
    assert_eq!(after["totalAssets"], 80);
    assert_eq!(after["equity"]["total"], 100);
    assert_eq!(after["periodResult"], -20);
    assert_eq!(after["balanced"], true);
    let touched = read(&tx, &range(Kind::TrialBalance, "2025-07-01", "2026-01-02")).unwrap();
    assert_eq!(
        touched["accounts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["accountNo"] == "I")
            .unwrap()["balance"],
        0
    );
    assert!(!serde_json::to_string(&before).unwrap().contains("100.0"));
    assert_eq!(tx.total_changes(), changes);
}
#[test]
fn opening_cutover_admits_cumulative_balance_but_refuses_undocumented_range() {
    let mut db = fixture();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let input:crate::opening_balance::Input=serde_json::from_value(json!({"cutOverDate":"2025-07-01","lines":[{"accountNo":"A","debitAmount":"100.00"},{"accountNo":"E","creditAmount":"100.00"}],"note":"Synthetic reviewed opening"})).unwrap();
    let prepared = crate::opening_balance::prepare(&tx, &input, &[], latest()).unwrap();
    let apply = crate::opening_balance::Apply {
        input,
        plan_hash: prepared.review()["planHash"].as_str().unwrap().into(),
        idempotency_key: "synthetic-dated-opening".into(),
        confirm: true,
    };
    crate::opening_balance::apply(
        &tx,
        &apply,
        &[],
        latest(),
        Actor {
            subject: "user:synthetic",
            program: crate::opening_balance::PROGRAM,
        },
        1767225600000,
    )
    .unwrap();
    post(&tx, "2025-07-02", "A", "I", "40.00");
    let changes = tx.total_changes();
    let sheet = read(&tx, &balance("2025-07-02")).unwrap();
    assert_eq!(sheet["totalAssets"], 140);
    assert_eq!(sheet["equity"]["total"], 100);
    assert_eq!(sheet["periodResult"], 40);
    assert_eq!(sheet["balanced"], true);
    assert_eq!(
        read(&tx, &range(Kind::ProfitLoss, "2025-07-02", "2025-07-02")).unwrap()["result"],
        40
    );
    assert_eq!(
        read(&tx, &balance("2025-06-30")).unwrap_err().code,
        "archive_report_unsupported"
    );
    assert_eq!(
        read(&tx, &range(Kind::TrialBalance, "2025-06-30", "2025-07-02"))
            .unwrap_err()
            .code,
        "archive_report_unsupported"
    );
    assert_eq!(tx.total_changes(), changes);
}
#[test]
fn original_business_error_shapes_and_numeric_precision_are_explicit() {
    let mut db = fixture();
    let tx = db.transaction().unwrap();
    let invalid = read(&tx, &range(Kind::TrialBalance, "2026-02-30", "not-a-date")).unwrap();
    assert_eq!(
        invalid,
        json!({"ok":false,"appliedRules":["financial-statements:trial-balance"],"periodStart":"2026-02-30","periodEnd":"not-a-date","accounts":[],"totalDebit":0,"totalCredit":0,"balanced":false,"linesConsidered":0,"journalEntryCount":0,"errors":["periodStart must be YYYY-MM-DD","periodEnd must be YYYY-MM-DD"]})
    );
    let inverted = read(&tx, &range(Kind::ProfitLoss, "2026-02-01", "2026-01-01")).unwrap();
    assert_eq!(inverted["ok"], false);
    assert_eq!(
        inverted["errors"],
        json!(["periodStart must be before or equal to periodEnd"])
    );
    assert_eq!(inverted["income"], json!([]));
    let bad_balance = read(&tx, &balance(" 2026-01-01")).unwrap();
    assert_eq!(bad_balance["ok"], false);
    assert_eq!(
        bad_balance["errors"],
        json!(["asOfDate must be YYYY-MM-DD"])
    );
    assert_eq!(bad_balance["equity"], json!({"lines":[],"total":0}));
    let mut malformed = balance("2026-01-01");
    malformed.from = Some("2026-01-01".into());
    assert_eq!(
        read(&tx, &malformed).unwrap_err().code,
        "invalid_dated_report_input"
    );
    for (raw, json) in [
        ("0.00", "0"),
        ("7.00", "7"),
        ("1.20", "1.2"),
        ("-0.01", "-0.01"),
        ("9007199254740992.00", "9007199254740992"),
    ] {
        assert_eq!(
            serde_json::to_string(&number(&Money::parse_canonical(raw).unwrap()).unwrap()).unwrap(),
            json
        );
    }
    for raw in ["9007199254740993.01", "9007199254740992.01"] {
        assert_eq!(
            number(&Money::parse_canonical(raw).unwrap())
                .unwrap_err()
                .code,
            "dated_report_precision_refused"
        );
    }
}
