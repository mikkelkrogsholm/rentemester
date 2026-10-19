use super::*;
use rusqlite::{Connection, TransactionBehavior};
fn query(from: &str, to: &str) -> Query {
    Query {
        scope: "company".into(),
        from: from.into(),
        to: to.into(),
        company_slugs: vec!["synthetic".into()],
        group_profile_id: None,
        account: None,
        party: None,
        dimension: None,
        currency: None,
        cursor: None,
        aggregate: "none".into(),
        limit: 200,
    }
}
fn fixture() -> (Connection, Connection) {
    let mut c = Connection::open_in_memory().unwrap();
    let mut l = Connection::open_in_memory().unwrap();
    crate::fresh::initialize_empty_pair(&mut c, &mut l, "Synthetic CFO").unwrap();
    l.execute_batch("INSERT INTO accounts(account_no,name,type,normal_balance)VALUES('A','Synthetic asset','asset','debit'),('E','Synthetic equity','equity','credit')").unwrap();
    (c, l)
}
fn post(tx: &Transaction<'_>, date: &str, document: Option<i64>) {
    let request=serde_json::from_value(json!({"transactionDate":date,"text":"Synthetic evidence","documentId":document.map(|n|n.to_string()),"currency":"DKK","lines":[{"accountNo":"A","debitAmount":"12.34"},{"accountNo":"E","creditAmount":"12.34"}]})).unwrap();
    let result = crate::journal_post::post(
        tx,
        &request,
        &crate::journal_preview::manual_journal_rule_version().unwrap(),
        Date::parse("2026-10-20").unwrap(),
        crate::journal_post::Actor {
            subject: "user:synthetic",
            program: "rentemester-rust",
        },
    )
    .unwrap();
    assert_eq!(result["ok"], true, "{result}");
}
fn collect_rows(c: &Transaction<'_>, l: &Transaction<'_>, q: &Query) -> CompanyRows {
    collect(l, c, "synthetic", q, &BTreeSet::from(["synthetic".into()])).unwrap()
}
#[test]
fn actual_journal_and_later_reversal_reconcile_each_date_without_read_writes() {
    let (mut c, mut l) = fixture();
    let ct = c
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let tx = l
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    post(&tx, "2025-12-30", None);
    let reverse = serde_json::from_value(
        json!({"entryId":"1","transactionDate":"2026-01-02","reason":"Synthetic dated correction"}),
    )
    .unwrap();
    let r = crate::journal_reversal::reverse(
        &tx,
        &reverse,
        &crate::journal_preview::manual_journal_rule_version().unwrap(),
        Date::parse("2026-10-20").unwrap(),
        crate::journal_post::Actor {
            subject: "user:synthetic",
            program: "rentemester-rust",
        },
    )
    .unwrap();
    assert_eq!(r["ok"], true);
    let changes = tx.total_changes();
    let old = collect_rows(&ct, &tx, &query("2025-01-01", "2025-12-31"));
    assert_eq!(old.rows.len(), 2);
    assert_eq!(old.rows[0].amount.decimal(), "12.34");
    let new = collect_rows(&ct, &tx, &query("2026-01-01", "2026-12-31"));
    assert_eq!(new.rows.len(), 2);
    assert_eq!(new.rows[0].amount.decimal(), "-12.34");
    assert_eq!(new.rows[0].journal_entry_id, Some(2));
    let all = collect_rows(&ct, &tx, &query("2025-01-01", "2026-12-31"));
    let mut q = query("2025-01-01", "2026-12-31");
    q.account = Some("A".into());
    let v = policy::render(
        &q,
        &policy::Selection {
            companies: vec!["synthetic".into()],
            partial: false,
        },
        &[all],
    )
    .unwrap();
    assert_eq!(v["reconciliation"]["amountByCurrency"]["DKK"], "0.00");
    assert_eq!(changes, tx.total_changes());
}
#[test]
fn active_reviewed_dimensions_keep_whole_line_and_replacement_provenance() {
    let (mut c, mut l) = fixture();
    let ct = c
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let tx = l
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    post(&tx, "2026-01-01", None);
    for (operation, body) in [
        (
            "define",
            json!({"confirm":true,"dimensionId":"project","kind":"project","name":"Synthetic project"}),
        ),
        (
            "member",
            json!({"confirm":true,"dimensionId":"project","memberId":"alpha","name":"Synthetic alpha"}),
        ),
        (
            "member",
            json!({"confirm":true,"dimensionId":"project","memberId":"beta","name":"Synthetic beta"}),
        ),
    ] {
        let i = serde_json::from_value(body).unwrap();
        crate::dimensions::master(&tx, operation, &i, "synthetic").unwrap();
    }
    let mut i:crate::dimensions::AssignmentInput=serde_json::from_value(json!({"journalLineId":1,"allocations":[{"dimensionId":"project","memberId":"alpha","amountMinor":1234,"currency":"DKK"}],"confirm":true})).unwrap();
    let p = crate::dimensions::plan(&tx, &i).unwrap();
    i.plan_hash = Some(p["plan"]["planHash"].as_str().unwrap().into());
    crate::dimensions::apply(&tx, &i, false, "synthetic").unwrap();
    let rows = collect_rows(&ct, &tx, &query("2026-01-01", "2026-12-31"));
    assert_eq!(rows.rows[0].dimensions[0].member_id, "alpha");
    i.allocations[0].member_id = "beta".into();
    i.expected_assignment_id = Some(1);
    i.reason = Some("Synthetic replacement".into());
    let p = crate::dimensions::plan(&tx, &i).unwrap();
    i.plan_hash = Some(p["plan"]["planHash"].as_str().unwrap().into());
    crate::dimensions::apply(&tx, &i, true, "synthetic").unwrap();
    let rows = collect_rows(&ct, &tx, &query("2026-01-01", "2026-12-31"));
    assert_eq!(rows.rows[0].dimensions[0].member_id, "beta");
    assert_eq!(rows.rows[0].amount.decimal(), "12.34");
}
#[test]
fn source_corruption_currency_mismatch_and_unattested_archive_refuse() {
    let (mut c, mut l) = fixture();
    let ct = c
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let tx = l
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    post(&tx, "2026-01-01", None);
    let q = query("2026-01-01", "2026-12-31");
    tx.execute("UPDATE companies SET currency='EUR'", [])
        .unwrap();
    assert!(
        collect(
            &tx,
            &ct,
            "synthetic",
            &q,
            &BTreeSet::from(["synthetic".into()])
        )
        .is_err()
    );
    tx.execute("UPDATE companies SET currency='DKK'", [])
        .unwrap();
    tx.execute("INSERT INTO import_archive_years(source_system,fiscal_year,balance_count)VALUES('synthetic',2024,0)",[]).unwrap();
    assert_eq!(
        collect(
            &tx,
            &ct,
            "synthetic",
            &q,
            &BTreeSet::from(["synthetic".into()])
        )
        .unwrap_err()
        .code,
        "cfo_archive_unavailable"
    );
    tx.execute_batch(
        "DROP TRIGGER journal_lines_no_update;UPDATE journal_lines SET debit_amount=99 WHERE id=1",
    )
    .unwrap();
    assert!(
        collect(
            &tx,
            &ct,
            "synthetic",
            &q,
            &BTreeSet::from(["synthetic".into()])
        )
        .is_err()
    );
}
#[test]
fn canonical_party_snapshot_is_scope_checked_and_gdpr_overlay_preserves_financial_rows() {
    let (mut c, mut l) = fixture();
    let ct = c
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let tx = l
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let create=serde_json::from_value(json!({"partyId":"synthetic-party","kind":"organization","name":"Synthetic canonical supplier","identifiers":[{"country":"DK","identifierKind":"dk_cvr","identifier":"12345678"}],"source":"synthetic","observedAt":"2026-10-19T00:00:00.000Z","reviewAssertion":"Synthetic reviewed identity","role":"vendor"})).unwrap();
    let party = crate::parties::create(
        &ct,
        &create,
        "synthetic",
        "user:synthetic",
        "2026-10-19T00:00:00.000Z",
    )
    .unwrap();
    crate::parties::role(
        &ct,
        &party,
        "synthetic",
        &crate::parties::Role {
            role: "vendor".into(),
            defaults: None,
        },
        "user:synthetic",
        "2026-10-19T00:00:00.000Z",
    )
    .unwrap();
    let metadata=serde_json::from_value(json!({"source":"synthetic","documentType":"purchase_sale","issueDate":"2026-01-01","deliveryDescription":"Synthetic supplies","invoiceNo":"SYN-CFO-1","amountIncVat":12.34,"vatAmount":0,"sender":{"name":"Original extraction","address":"Synthetic street1","vatOrCvr":"DK12345678"},"recipient":{"name":"Synthetic buyer","address":"Synthetic street2","vatOrCvr":"DK87654321"}})).unwrap();
    crate::documents::register(
        &tx,
        crate::documents::Original {
            filename: "synthetic.txt",
            hash: &"a".repeat(64),
            path: "synthetic-original",
            mime: "text/plain",
        },
        &metadata,
        "synthetic",
        "2026-10-19",
    )
    .unwrap();
    let i:crate::document_party_links::PlanInput=serde_json::from_value(json!({"documentId":1,"role":"vendor","jurisdiction":"DK","identifierKind":"dk_cvr","identifier":"12345678"})).unwrap();
    let snapshot = crate::document_party_links::capture(&ct, &i, "synthetic").unwrap();
    let plan = crate::document_party_links::plan(&tx, &i, &snapshot).unwrap();
    let mut body = serde_json::to_value(&i).unwrap();
    body["planHash"] = plan["plan"]["planHash"].clone();
    body["confirm"] = json!(true);
    let apply = serde_json::from_value(body).unwrap();
    crate::document_party_links::apply(
        &tx,
        &apply,
        &snapshot,
        "user:synthetic",
        "user:synthetic",
        "2026-10-19T00:00:00.000Z",
    )
    .unwrap();
    post(&tx, "2026-01-01", Some(1));
    let q = query("2026-01-01", "2026-12-31");
    let rows = collect_rows(&ct, &tx, &q);
    assert_eq!(
        rows.rows[0].party_name.as_deref(),
        Some("Synthetic canonical supplier")
    );
    assert_eq!(
        rows.rows[0].document_party_name.as_deref(),
        Some("Original extraction")
    );
    assert!(rows.rows[0].party_link_provenance.is_some());
    // Unit corruption/authority fixture: remove the role from current control scope only.
    ct.execute(
        "DELETE FROM rm_party_company_roles WHERE party_id=?",
        [&party],
    )
    .unwrap();
    let rows = collect_rows(&ct, &tx, &q);
    assert!(rows.rows[0].party_id.is_none());
    assert!(rows.rows[0].party_name.is_none());
    assert!(rows.rows[0].document_party_name.is_none());
    assert_eq!(rows.rows[0].amount.decimal(), "12.34");
    tx.execute("INSERT INTO gdpr_erasures(subject_key,source,source_row_id,redacted_fields,rule_id,reason)VALUES('synthetic','documents',1,'[\"name\"]','synthetic','unit overlay')",[]).unwrap();
    let rows = collect_rows(&ct, &tx, &q);
    assert!(rows.rows[0].text.is_none());
    assert_eq!(rows.rows.len(), 2);
}
