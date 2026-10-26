use super::*;
const NOW: &str = "2026-10-26T12:00:00.000Z";
fn pair() -> (Connection, Connection) {
    let mut c = Connection::open_in_memory().unwrap();
    let mut l = Connection::open_in_memory().unwrap();
    crate::fresh::initialize_empty_pair(&mut c, &mut l, "Synthetic sources").unwrap();
    (c, l)
}
fn ctx<'a>(key: &'a str) -> Context<'a> {
    Context {
        actor: "user:synthetic",
        principal: "synthetic",
        idempotency_key: key,
        expected_version: None,
        now: NOW,
        source_check: None,
    }
}
fn sample(state: &str, hash: &str) -> Observation {
    let mut o = observation(
        "synthetic",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "vat_filing",
        "2026-07-01:2026-09-30",
        "Indberet moms".into(),
        "Læs kilden",
        json!({"hash":hash}),
        checked(
            state,
            NOW,
            if state == "resolved" {
                vec![reference("synthetic", "period", "2026-07-01:2026-09-30")]
            } else {
                vec![]
            },
        ),
    )
    .unwrap();
    o.draft["type"] = json!("obligation");
    o
}
#[test]
fn sources_native_all_workbench_pages_and_unknown_facts_are_honest() {
    let (_c, mut l) = pair();
    l.execute_batch("UPDATE companies SET vat_period_type='quarter';")
        .unwrap();
    for n in 0..101 {
        l.execute("INSERT INTO bank_transactions(transaction_date,text,amount,currency)VALUES('2026-10-20',?1,-10,'DKK')",[format!("Synthetic {n}")]).unwrap();
    }
    let tx = l
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let before = tx.total_changes();
    let mut calls = 0;
    let all = collect(
        &tx,
        ReadContext {
            slug: "synthetic",
            as_of: "2026-10-26",
            now: NOW,
            today: "2026-10-26",
            latest: Date::parse("2026-12-10").unwrap(),
            integrity: Ok(vec![]),
        },
        |_| {
            calls += 1;
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(calls, 1);
    assert_eq!(
        all.iter()
            .filter(|o| o.draft["source"]["kind"] == "bank_work")
            .count(),
        101
    );
    assert!(
        all.iter()
            .any(|o| o.draft["source"]["kind"] == "vat_filing")
    );
    let annual = all
        .iter()
        .find(|o| o.draft["source"]["kind"] == "annual_reporting")
        .unwrap();
    assert_eq!(annual.draft["relevance"], "unknown");
    assert!(annual.draft["deadline"].is_null());
    assert_eq!(
        check_bank(&tx, "synthetic", "999", NOW).unwrap().state,
        "unknown"
    );
    assert_eq!(
        check_bank(&tx, "synthetic", "1", NOW).unwrap().state,
        "open"
    );
    assert_eq!(tx.total_changes(), before);
}
#[test]
fn sources_native_vat_zero_refund_cadences_and_actual_reported_reference() {
    for cadence in ["month", "quarter", "half-year"] {
        let (_c, mut l) = pair();
        l.execute("UPDATE companies SET vat_period_type=?1", [cadence])
            .unwrap();
        let tx = l
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        let all = collect(
            &tx,
            ReadContext {
                slug: "synthetic",
                as_of: "2026-10-26",
                now: NOW,
                today: "2026-10-26",
                latest: Date::parse("2026-12-10").unwrap(),
                integrity: Ok(vec![]),
            },
            |_| Ok(()),
        )
        .unwrap();
        assert!(
            all.iter()
                .any(|o| o.draft["source"]["kind"] == "vat_filing"),
            "zero VAT still needs filing: {cadence}"
        );
    }
    for has_reference in [false, true] {
        let (_c, mut l) = pair();
        l.execute_batch("UPDATE companies SET cvr='87654321',vat_period_type='quarter';INSERT INTO accounts(account_no,name,type,normal_balance)VALUES('B','Bank','asset','debit'),('R','Receivable','asset','debit'),('C','Creditor','liability','credit');INSERT INTO account_role_mappings(role,account_no,version,confirmed_by)VALUES('bank','B',1,'synthetic'),('debtors','R',1,'synthetic'),('creditors','C',1,'synthetic');").unwrap();
        let tx = l
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        let packet = crate::periods::compute(&tx, "2026-07-01", "2026-09-30", Ok(vec![])).unwrap();
        assert_eq!(packet["blockers"], 0, "{packet}");
        let review = crate::periods::review(&tx, &packet, "synthetic").unwrap();
        let mut input:crate::periods::original::CloseInput=serde_json::from_value(json!({"periodStart":"2026-07-01","periodEnd":"2026-09-30","kind":"vat_period","status":"closed","packetHash":packet["hash"],"reviewId":review["id"],"confirm":true})).unwrap();
        crate::periods::original::atomic(&tx, Some(&packet), || {
            crate::periods::original::close_current(&tx, &input, &packet, "synthetic", "2026-10-26")
        })
        .unwrap();
        assert_eq!(
            check_vat(&tx, "synthetic", "2026-07-01:2026-09-30", NOW)
                .unwrap()
                .state,
            "open"
        );
        input.status = Some("reported".into());
        input.reference = has_reference.then(|| "SYNTHETIC-RECORDED-REFERENCE".into());
        crate::periods::original::atomic(&tx, None, || {
            Ok(crate::periods::original::report_existing(&tx, &input, "synthetic")?.unwrap())
        })
        .unwrap();
        tx.execute("UPDATE companies SET vat_period_type=NULL", [])
            .unwrap();
        let actual = check_vat(&tx, "synthetic", "2026-07-01:2026-09-30", NOW).unwrap();
        assert_eq!(
            actual.state,
            if has_reference { "resolved" } else { "unknown" }
        );
        if has_reference {
            assert_eq!(actual.evidence[0].kind, "period");
        }
    }
}
#[test]
fn sources_sync_exact_replay_noop_product_completion_reopen_and_recovery() {
    let (mut c, _) = pair();
    let companies = vec!["synthetic".into()];
    let sync = |c: &mut Connection, key: &str, o: Observation| {
        let tx = c
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        let r = sync_in_tx(
            &tx,
            &json!({"companySlugs":companies}),
            &companies,
            &ctx(key),
            |_| Ok(()),
            |_, _| Ok((vec![o.clone()], false)),
        )
        .unwrap();
        tx.commit().unwrap();
        r
    };
    assert_eq!(sync(&mut c, "first", sample("open", "1")).sync.created, 1);
    assert_eq!(sync(&mut c, "repeat", sample("open", "1")).sync.updated, 0);
    let before: i64 = c
        .query_row("SELECT count(*) FROM rm_task_events", [], |r| r.get(0))
        .unwrap();
    let tx = c
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    let r = sync_in_tx(
        &tx,
        &json!({"companySlugs":companies}),
        &companies,
        &ctx("first"),
        |_| Ok(()),
        |_, _| panic!("replay probes no source"),
    )
    .unwrap();
    assert_eq!(r.sync.created, 1);
    tx.rollback().unwrap();
    assert_eq!(
        c.query_row::<i64, _, _>("SELECT count(*) FROM rm_task_events", [], |r| r.get(0))
            .unwrap(),
        before
    );
    assert_eq!(
        sync(&mut c, "resolved", sample("resolved", "2"))
            .sync
            .resolved,
        1
    );
    let task = list(&c, |_| Ok(true)).unwrap().pop().unwrap();
    assert_eq!(
        task.completion.as_ref().unwrap().assurance,
        "product_verified"
    );
    assert_eq!(
        sync(&mut c, "unknown", sample("unknown", "3"))
            .sync
            .reopened,
        1
    );
    let task = list(&c, |_| Ok(true)).unwrap().pop().unwrap();
    assert!(task.completion.is_none());
    assert!(task.verification_required);
}

#[test]
fn sources_annual_unknown_relevance_records_resolved_fact_without_false_completion() {
    let (mut c, _) = pair();
    let companies = vec!["synthetic".into()];
    for (key, state) in [("annual-open", "open"), ("annual-resolved", "resolved")] {
        let mut o = sample(state, state);
        o.draft["origin"] = json!("proposal");
        o.draft["relevance"] = json!("unknown");
        o.draft["source"]["kind"] = json!("annual_reporting");
        let tx = c
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        sync_in_tx(
            &tx,
            &json!({}),
            &companies,
            &ctx(key),
            |_| Ok(()),
            |_, _| Ok((vec![o.clone()], false)),
        )
        .unwrap();
        tx.commit().unwrap();
    }
    let task = list(&c, |_| Ok(true)).unwrap().pop().unwrap();
    assert_eq!(task.source.unwrap().state, "resolved");
    assert_eq!(task.status, "open");
    assert!(task.verification_required);
    assert!(task.completion.is_none());
}
#[test]
fn sources_original_integrity_failure_cannot_become_verified_or_erase_known_tasks() {
    let (mut c, mut l) = pair();
    let tx = c
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    let task =
        synchronize_in_tx(&tx, &sample("open", "1").draft, &ctx("seed"), |_| Ok(())).unwrap();
    tx.commit().unwrap();
    let tx = l.transaction().unwrap();
    let failed = Err(Error::new(
        "original_unavailable",
        "Synthetic missing original",
    ));
    assert_eq!(
        check(&tx, "synthetic", &task, NOW, failed.clone())
            .unwrap()
            .state,
        "unknown"
    );
    let (observed, failed) = observe(
        &tx,
        ReadContext {
            slug: "synthetic",
            as_of: "2026-10-26",
            now: NOW,
            today: "2026-10-26",
            latest: Date::parse("2026-12-10").unwrap(),
            integrity: failed,
        },
        &[task],
        true,
        |_| Ok(()),
    );
    assert!(failed);
    assert_eq!(observed.len(), 1);
    assert_eq!(observed[0].check.state, "unknown");
    assert!(observed[0].check.evidence.is_empty());
}
