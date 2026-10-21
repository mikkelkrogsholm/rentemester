use super::*;
use rusqlite::{Connection, TransactionBehavior};
#[test]
fn original_payable_dated_filters_receipts_partial_full_and_historical_creditor() {
    let mut c = Connection::open_in_memory().unwrap();
    let mut db = Connection::open_in_memory().unwrap();
    crate::fresh::initialize_empty_pair(&mut c, &mut db, "Synthetic supplier buyer").unwrap();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    tx.execute_batch("UPDATE companies SET cvr='87654321',address='Synthetic1',vat_period_type='quarter' WHERE id=1;INSERT INTO accounts(account_no,name,type,normal_balance,default_vat_code)VALUES('E','Synthetic expense','expense','debit','DK_PURCHASE_25'),('V','Synthetic VAT','vat','debit',NULL),('C','Synthetic creditor','liability','credit',NULL),('C2','Synthetic later creditor','liability','credit',NULL),('B','Synthetic bank','asset','debit',NULL);INSERT INTO account_role_mappings(role,account_no,version,confirmed_by)VALUES('creditors','C',1,'synthetic'),('input_vat','V',1,'synthetic');INSERT INTO bank_accounts(slug,name,ledger_account_no)VALUES('synthetic','Synthetic bank','B');INSERT INTO vendors(id,name,vat_or_cvr)VALUES(1,'Synthetic Alpha','DK12345678');").unwrap();
    let latest = Date::parse("2026-10-22").unwrap();
    let now = 1792368000000;
    let mut requests = vec![];
    for (id, date, due, name, amount, vat, vendor) in [
        (
            1,
            "2026-10-17",
            "2026-10-18",
            "Synthetic Alpha",
            125,
            25,
            Some(1),
        ),
        (
            2,
            "2026-10-16",
            "2026-10-17",
            "Synthetic Beta",
            250,
            50,
            None,
        ),
    ] {
        let meta=serde_json::from_value(json!({"source":"synthetic","documentType":"purchase_sale","issueDate":date,"invoiceNo":format!("SYN-{id}"),"deliveryDescription":"Synthetic supplies","amountIncVat":amount,"vatAmount":vat,"sender":{"name":name,"address":"Synthetic2","vatOrCvr":"DK12345678"},"recipient":{"name":"Synthetic supplier buyer","address":"Synthetic1","vatOrCvr":"DK87654321"}})).unwrap();
        crate::documents::register(
            &tx,
            crate::documents::Original {
                filename: "synthetic.txt",
                path: "synthetic.txt",
                hash: &format!("{id:064x}"),
                mime: "text/plain",
            },
            &meta,
            "synthetic",
            "2026-10-18",
        )
        .unwrap();
        let mut value = json!({"documentId":id,"billDate":date,"dueDate":due,"expenseAccount":"E","confirm":true,"idempotencyKey":format!("register-{id}")});
        if let Some(vendor) = vendor {
            value["vendorId"] = json!(vendor);
        }
        let input: Register = serde_json::from_value(value).unwrap();
        let registered = mutate(
            &tx,
            Action::Register(&input),
            "service_synthetic",
            PrincipalKind::ServiceAccount,
            latest,
            now,
        )
        .unwrap();
        assert_eq!(registered["data"]["payableId"], id);
        assert_eq!(registered["appliedRules"][0], "DK-PAYABLE-001");
        assert!(registered["data"]["entryHash"].is_string());
        requests.push(input);
    }
    let csv = "date;text;amount;currency\n2026-10-19;Synthetic partial;-50;DKK\n2026-10-20;Synthetic full;-75;DKK\n";
    crate::bank::import(
        &tx,
        csv,
        &rentemester_engine::bank::parse(csv).unwrap(),
        Some("synthetic"),
        "synthetic",
        "2026-10-20T00:00:00Z",
        "synthetic-bank.txt",
    )
    .unwrap();
    tx.execute_batch("UPDATE account_role_mappings SET status='superseded' WHERE role='creditors';INSERT INTO account_role_mappings(role,account_no,version,confirmed_by)VALUES('creditors','C2',2,'synthetic');").unwrap();
    let partial:Pay=serde_json::from_value(json!({"payableId":1,"bankTransactionId":1,"amount":50,"date":"2026-10-19","paymentAccount":"B","confirm":true,"idempotencyKey":" partial raw key "})).unwrap();
    assert_eq!(
        partial.payload(),
        json!({"payableId":1,"bankTransactionId":1,"amount":50,"date":"2026-10-19","paymentAccount":"B","note":null})
    );
    let first = mutate(
        &tx,
        Action::Pay(&partial),
        "service_synthetic",
        PrincipalKind::ServiceAccount,
        latest,
        now + 1,
    )
    .unwrap();
    assert_eq!(first["data"]["openBalance"], 75.0);
    assert_eq!(first["appliedRules"], json!(["DK-PAYABLE-PAYMENT-001"]));
    let full: Pay =
        serde_json::from_value(json!({"payableId":1,"bankTransactionId":2,"confirm":true}))
            .unwrap();
    let full = mutate(
        &tx,
        Action::Pay(&full),
        "human",
        PrincipalKind::User,
        latest,
        now + 2,
    )
    .unwrap();
    assert!(full["data"].get("idempotency").is_none());
    assert_eq!(full["data"]["openBalance"], 0.0);
    let repeated = mutate(
        &tx,
        Action::Pay(&partial),
        "service_synthetic",
        PrincipalKind::ServiceAccount,
        latest,
        now + 3,
    )
    .unwrap();
    assert_eq!(repeated["data"]["openBalance"], 75.0);
    assert_eq!(repeated["data"]["idempotency"]["replayed"], true);
    assert_eq!(
        mutate(
            &tx,
            Action::Register(&requests[0]),
            "service_synthetic",
            PrincipalKind::ServiceAccount,
            latest,
            now + 4
        )
        .unwrap()["data"]["idempotency"]["replayed"],
        true
    );
    let query = Query {
        as_of: Some("2026-10-19".into()),
        as_of_date: Some("2020-01-01".into()),
        ..Query::default()
    };
    let all = list(&tx, &query, "1999-01-01").unwrap();
    assert_eq!(all["data"]["totalOpenBalance"], 325.0);
    assert_eq!(all["data"]["rows"][0]["payableId"], 2);
    assert_eq!(all["data"]["rows"][1]["paidAmount"], 50.0);
    let q = Query {
        status: Some("overdue".into()),
        as_of: Some("2026-10-19".into()),
        min_days: Some(2),
        ..Query::default()
    };
    assert_eq!(
        list(&tx, &q, "1999-01-01").unwrap()["data"]["totalOpenBalance"],
        250.0
    );
    let q = Query {
        status: Some("paid".into()),
        supplier: Some(" ALPHA ".into()),
        vendor_id: Some(1),
        from: Some("2026-10-17".into()),
        to: Some("2026-10-17".into()),
        ..Query::default()
    };
    assert_eq!(list(&tx, &q, "2026-10-20").unwrap()["data"]["count"], 1);
    assert_eq!(list(&tx, &q, "2026-10-19").unwrap()["data"]["count"], 0);
    let reversed = Query {
        from: Some("2026-10-20".into()),
        to: Some("2026-10-01".into()),
        ..Query::default()
    };
    assert_eq!(
        list(&tx, &reversed, "2026-10-22").unwrap()["data"]["count"],
        0
    );
    assert_eq!(tx.query_row("SELECT count(*) FROM journal_lines l JOIN accounts a ON a.id=l.account_id WHERE a.account_no='C2'",[],|r|r.get::<_,i64>(0)).unwrap(),0);
    assert!(crate::journal_hash::verify(&tx).unwrap().ok);
    let mut changed = partial;
    changed.note = Some("Different valid intent".into());
    assert_eq!(
        mutate(
            &tx,
            Action::Pay(&changed),
            "service_synthetic",
            PrincipalKind::ServiceAccount,
            latest,
            now + 5
        )
        .unwrap_err()
        .code,
        "IDEMPOTENCY_CONFLICT"
    );
}
