use super::*;
fn amount(s: &str) -> ArchiveDecimal {
    ArchiveDecimal::parse_csv(s).unwrap()
}
fn year() -> Year {
    let posting = |n, no: &str, date: &str, voucher: &str, s: &str| Posting {
        line_no: n,
        account_no: no.into(),
        account_name: format!("Original {no}"),
        transaction_date: date.into(),
        voucher: voucher.into(),
        voucher_type: "Original type".into(),
        text: "Original description".into(),
        vat_type: "Source VAT lexeme".into(),
        amount: amount(s).money(),
        source_amount: amount(s),
        running_balance: Some(amount("1.234567").money()),
        source_running_balance: Some(amount("1.234567")),
    };
    let balance = |n, no: &str, s: &str| Closing {
        line_no: n,
        account_no: no.into(),
        account_name: format!("Original {no}"),
        amount: amount(s).money(),
        source_amount: amount(s),
    };
    Year {
        fiscal_year: 2024,
        source_system: "dinero".into(),
        import_id: 11,
        source_hash: "a".repeat(64),
        parser_contract: crate::archive_import::FORMAT.into(),
        declared_currency: "DKK".into(),
        has_balances: true,
        postings: vec![
            posting(0, "A", "2023-07-01", "42", "0.005000"),
            posting(1, "I", "2023-07-01", "42", "-0.005000"),
            posting(2, "A", "2024-02-02", "", "0.005000"),
            posting(3, "X", "2024-02-02", "", "-0.004999"),
        ],
        balances: vec![
            balance(0, "A", "100000000000000"),
            balance(1, "L", "-20"),
            balance(2, "I", "-20"),
            balance(3, "X", "5"),
            balance(4, "E", "-99999999999965"),
        ],
    }
}
fn chart() -> Vec<Classification> {
    [
        ("A", "asset", "debit"),
        ("L", "liability", "credit"),
        ("E", "equity", "credit"),
        ("I", "income", "credit"),
        ("X", "expense", "debit"),
    ]
    .into_iter()
    .map(|(no, kind, n)| Classification {
        account_no: no.into(),
        kind: kind.into(),
        normal_balance: n.into(),
    })
    .collect()
}
fn context() -> Context {
    Context {
        slug: "synthetic".into(),
        ledger_uuid: "b".repeat(32),
        company: json!({"name":"Current name","currency":"EUR","fiscalYearStartMonth":7}),
        fiscal_years: json!([{ "label":"2024","source":"archive","start":null,"end":null}]),
    }
}
#[test]
fn exact_source_closing_reducers_large_money_polarity_vat_and_single_result() {
    let mut y = year();
    let c = context();
    let mut chart = chart();
    // A debit-normal VAT balance uses the existing native asset classification.
    chart.push(Classification {
        account_no: "V".into(),
        kind: "vat".into(),
        normal_balance: "debit".into(),
    });
    y.balances.push(Closing {
        line_no: 5,
        account_no: "V".into(),
        account_name: "Original VAT".into(),
        amount: amount("0").money(),
        source_amount: amount("0"),
    });
    let tb = render(&c, &y, &chart, None, Resource::TrialBalance).unwrap();
    assert_eq!(tb["totalDebit"], "100000000000005.00");
    assert_eq!(tb["totalCredit"], "100000000000005.00");
    assert_eq!(tb["balanced"], true);
    let pnl = render(&c, &y, &chart, None, Resource::IncomeStatement).unwrap();
    assert_eq!(pnl["totalIncome"], "20.00");
    assert_eq!(pnl["totalExpense"], "5.00");
    assert_eq!(pnl["result"], "15.00");
    assert!(pnl["priorResult"].is_null());
    let sheet = render(&c, &y, &chart, None, Resource::Balance).unwrap();
    assert_eq!(sheet["totalAssets"], "100000000000000.00");
    assert_eq!(sheet["equity"]["total"], "99999999999980.00");
    assert_eq!(sheet["balanced"], true);
    assert_eq!(
        sheet["equity"]["lines"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|l| l["accountNo"] == "—")
            .count(),
        1
    );
    assert!(sheet["asOfDate"].is_null());
    assert!(sheet["periodStart"].is_null());
    assert_eq!(sheet["declaredCurrency"], "DKK");
    assert_eq!(sheet["company"]["currency"], "EUR");
    assert_eq!(sheet["evidence"]["observedPostingStart"], "2023-07-01");
}
#[test]
fn journal_preserves_raw_halfcents_zero_lines_group_identity_and_whole_voucher_filter() {
    let y = year();
    let c = context();
    let chart = chart();
    let all = render(&c, &y, &chart, None, Resource::Journal).unwrap();
    let entries = all["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 3);
    assert_eq!(
        entries
            .iter()
            .flat_map(|e| e["lines"].as_array().unwrap())
            .count(),
        4
    );
    assert!(
        entries
            .iter()
            .all(|e| e["id"].is_null() && e["documentId"].is_null())
    );
    let filtered = render(&c, &y, &chart, Some("I"), Resource::Journal).unwrap();
    let e = &filtered["entries"][0];
    assert_eq!(e["lines"].as_array().unwrap().len(), 2);
    assert_eq!(e["lines"][0]["sourceAmount"], "0.005000");
    assert_eq!(e["lines"][0]["debit"], "0.01");
    assert_eq!(e["lines"][1]["credit"], "0.01");
    assert_eq!(e["lines"][0]["sourceRunningBalance"], "1.234567");
    assert_eq!(e["lines"][0]["vatType"], "Source VAT lexeme");
    assert!(e["sourceId"].as_str().unwrap().ends_with(":voucher:42"));
    assert!(render(&c, &y, &chart, Some("unknown"), Resource::Journal).is_err());
    assert!(render(&c, &y, &chart, Some("A"), Resource::Balance).is_err());
}
#[test]
fn unknown_classification_never_drops_money_or_fabricates_complete_totals() {
    let y = year();
    let c = context();
    let mut chart = chart();
    chart.retain(|c| c.account_no != "X");
    let tb = render(&c, &y, &chart, None, Resource::TrialBalance).unwrap();
    assert_eq!(tb["rows"].as_array().unwrap().len(), 5);
    assert_eq!(tb["completeness"]["status"], "ready");
    assert_eq!(tb["totalDebit"], "100000000000005.00");
    let pnl = render(&c, &y, &chart, None, Resource::IncomeStatement).unwrap();
    assert_eq!(pnl["completeness"]["status"], "incomplete");
    assert!(pnl["totalIncome"].is_null());
    assert!(pnl["result"].is_null());
    assert_eq!(pnl["income"][0]["amount"], "20.00");
    assert_eq!(
        pnl["completeness"]["unclassifiedAccounts"][0]["sourceRows"][0]["sourceAmount"],
        "5.000000"
    );
    let overview = render(&c, &y, &chart, None, Resource::Overview).unwrap();
    assert!(overview["profitAndLoss"]["resultat"].is_null());
    assert_eq!(overview["activity"]["status"], "incomplete");
    assert_eq!(overview["profitAndLoss"]["months"][0]["month"], "2023-07");
    assert_eq!(
        overview["profitAndLoss"]["months"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert!(overview["profitAndLoss"]["months"][1]["income"].is_null());
    assert!(overview["bank"]["balance"].is_null());
    assert!(overview["receivables"]["openCount"].is_null());
    assert!(overview["exceptions"]["count"].is_null());
    chart.push(Classification {
        account_no: "X".into(),
        kind: "expense".into(),
        normal_balance: "debit".into(),
    });
    assert_eq!(
        render(&c, &y, &chart, None, Resource::IncomeStatement).unwrap()["result"],
        "15.00"
    );
}
#[test]
fn missing_and_explicit_empty_closing_files_are_distinct_unavailable_without_hiding_activity() {
    for has_file in [false, true] {
        let mut y = year();
        y.has_balances = has_file;
        y.balances.clear();
        let c = context();
        let chart = chart();
        for r in [
            Resource::TrialBalance,
            Resource::IncomeStatement,
            Resource::Balance,
            Resource::Overview,
        ] {
            let report = render(&c, &y, &chart, None, r).unwrap();
            assert_eq!(report["completeness"]["status"], "unavailable");
            assert_eq!(
                report["completeness"]["reasons"][0],
                if has_file {
                    "empty_closing_balances"
                } else {
                    "missing_closing_balances"
                }
            );
        }
        let report = render(&c, &y, &chart, None, Resource::TrialBalance).unwrap();
        assert!(report["totalDebit"].is_null());
        assert!(report["balanced"].is_null());
        let journal = render(&c, &y, &chart, None, Resource::Journal).unwrap();
        assert_eq!(journal["completeness"]["status"], "ready");
        assert_eq!(journal["entries"].as_array().unwrap().len(), 3);
        let overview = render(&c, &y, &chart, None, Resource::Overview).unwrap();
        assert_eq!(overview["activity"]["status"], "ready");
        assert_eq!(overview["profitAndLoss"]["months"][0]["income"], "0.01");
    }
}
