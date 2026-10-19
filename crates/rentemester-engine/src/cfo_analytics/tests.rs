use super::*;
pub(super) fn query() -> Query {
    Query {
        scope: "company".into(),
        from: "2026-01-01".into(),
        to: "2026-12-31".into(),
        company_slugs: vec!["synthetic".into()],
        group_profile_id: None,
        account: None,
        party: None,
        dimension: None,
        currency: None,
        cursor: None,
        aggregate: "none".into(),
        limit: 1,
    }
}
fn row(id: &str, date: &str, amount: &str) -> Row {
    Row {
        company_slug: "synthetic".into(),
        source_type: "ledger".into(),
        source_hash: "a".repeat(64),
        source_id: id.into(),
        journal_entry_id: Some(1),
        journal_entry_no: Some("SYN-1".into()),
        document_id: None,
        document_hash: None,
        party_id: None,
        party_name: None,
        document_party_name: None,
        party_link_provenance: None,
        account_no: "A".into(),
        account_name: None,
        transaction_date: date.into(),
        currency: "EUR".into(),
        amount: Money::parse(amount).unwrap(),
        text: None,
        dimensions: vec![],
    }
}
fn source(rows: Vec<Row>) -> CompanyRows {
    CompanyRows {
        company_slug: "synthetic".into(),
        rows,
        evidence: json!({"companySlug":"synthetic","status":"ready","postedWithoutDocument":1,"openExceptions":0}),
    }
}
#[test]
fn full_universe_exact_sums_and_cursor_use_same_tuple_comparator() {
    let q = query();
    let s = Selection {
        companies: vec!["synthetic".into()],
        partial: false,
    };
    let rows = source(vec![
        row("journal:2:line:2", "2026-06-01", "9007199254740991.01"),
        row("journal:10:line:1", "2026-01-01", "-9007199254740991.00"),
        row("journal:3:line:3", "2026-08-01", "0.10"),
    ]);
    let a = render(&q, &s, std::slice::from_ref(&rows)).unwrap();
    assert_eq!(a["reconciliation"]["amountByCurrency"]["EUR"], "0.11");
    assert_eq!(a["reconciliation"]["rowCount"], 3);
    assert_eq!(a["rows"][0]["sourceId"], "journal:10:line:1");
    let mut q2 = q.clone();
    q2.cursor = Some(a["page"]["nextCursor"].as_str().unwrap().into());
    let b = render(&q2, &s, &[rows]).unwrap();
    assert_eq!(b["rows"][0]["sourceId"], "journal:2:line:2");
    assert_eq!(b["reconciliation"], a["reconciliation"]);
}
#[test]
fn filters_select_whole_lines_and_partial_portfolio_never_sums() {
    let mut q = query();
    q.scope = "portfolio".into();
    q.aggregate = "sum".into();
    q.dimension = Some("region:west".into());
    q.party = Some("party-synthetic".into());
    q.currency = Some("eur".into());
    let s = Selection {
        companies: vec!["synthetic".into()],
        partial: true,
    };
    let mut r = row("journal:1:line:1", "2026-06-01", "20.00");
    r.party_id = Some("party-synthetic".into());
    r.dimensions = vec![Dimension {
        dimension_id: "region".into(),
        member_id: "west".into(),
        amount_minor: 500,
        currency: "EUR".into(),
    }];
    let a = render(&q, &s, &[source(vec![r])]).unwrap();
    assert_eq!(a["rows"][0]["amount"], "20.00");
    assert_eq!(a["status"], "incomplete");
    assert!(a["reconciliation"].get("amountByCurrency").is_none());
    assert_eq!(a["reconciliation"]["rowCount"], 1);
}
#[test]
fn invalid_dates_cursors_scope_and_duplicate_source_rows_refuse() {
    for c in ["bad", "AA", "Zm9v", "===="] {
        let mut q = query();
        q.cursor = Some(c.into());
        assert!(q.validate().is_err());
    }
    let mut q = query();
    q.from = "2026-02-30".into();
    assert!(q.validate().is_err());
    let q = query();
    let r = row("journal:1:line:1", "2026-06-01", "1.00");
    assert!(
        render(
            &q,
            &Selection {
                companies: vec!["synthetic".into()],
                partial: false
            },
            &[source(vec![r.clone(), r])]
        )
        .is_err()
    );
    assert!(
        render(
            &q,
            &Selection {
                companies: vec!["different".into()],
                partial: false
            },
            &[source(vec![])]
        )
        .is_err()
    );
}

#[test]
fn explicit_portfolio_totals_keep_verified_currencies_separate_without_fx() {
    let mut q = query();
    q.scope = "portfolio".into();
    q.aggregate = "sum".into();
    let left = source(vec![row("journal:1:line:1", "2026-01-01", "12.34")]);
    let mut right = source(vec![row("journal:1:line:1", "2026-01-01", "1.01")]);
    right.company_slug = "other".into();
    right.rows[0].company_slug = "other".into();
    right.rows[0].currency = "DKK".into();
    let s = Selection {
        companies: vec!["synthetic".into(), "other".into()],
        partial: false,
    };
    let result = render(&q, &s, &[left, right]).unwrap();
    assert_eq!(
        result["reconciliation"]["amountByCurrency"],
        json!({"EUR":"12.34","DKK":"1.01"})
    );
    assert_eq!(result["reconciliation"]["rowCount"], 2);
}
