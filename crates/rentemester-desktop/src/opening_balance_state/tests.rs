use super::*;
pub(crate) fn snapshot() -> Snapshot {
    let profile = json!({"id":1,"name":"Synthetic opening","currency":"EUR"});
    Snapshot::parse("synthetic",json!({"ok":true,"company":profile}),json!({"ok":true,"accounts":{"slug":"synthetic","company":profile,"accounts":[{"accountNo":"A","type":"asset","active":true},{"accountNo":"E","type":"equity","active":true}]}}),json!({"ok":true,"user":{"id":"synthetic-owner"}}),json!({"ok":true,"journal":{"slug":"synthetic","ledgerUuid":"a".repeat(32),"entries":[],"fiscalYears":[{"label":"explicit-shifted-label","start":"2024-07-01","end":"2025-06-30","source":"live"}]}}),json!({"ok":true,"openingBalance":null})).unwrap()
}
pub(crate) fn review() -> Review {
    let before = snapshot();
    let bytes = input(
        "2024-07-01",
        &[
            LineInput {
                account_no: "A".into(),
                debit: "100.00".into(),
                ..Default::default()
            },
            LineInput {
                account_no: "E".into(),
                credit: "100.00".into(),
                ..Default::default()
            },
        ],
        "Synthetic adoption",
        &[],
        &before,
    )
    .unwrap();
    let value: Value = serde_json::from_slice(&bytes).unwrap();
    let plan = json!({"input":value,"planHash":"b".repeat(64),"ruleVersion":"server-rule/1","sourceBasis":{"kind":"operator_supplied","currency":"EUR","sourceArchives":[]},"totals":{"debit":"100.00","credit":"100.00"},"existing":null,"blockers":[],"status":"ready"});
    Review::parse(before, &bytes, json!({"ok":true,"plan":plan})).unwrap()
}
pub(crate) fn record() -> Value {
    let r = review();
    json!({"ok":true,"openingBalance":{"cutOverDate":"2024-07-01","journalEntryId":1,"journalEntryNo":"explicit-1","journalEntryHash":"c".repeat(64),"planHash":r.plan["planHash"],"sourceBasis":r.plan["sourceBasis"],"createdAt":"2026-10-20T12:00:00.000Z","actor":"user:synthetic-owner"},"idempotency":{"receiptId":1,"replayed":false,"createdAt":"2026-10-20T12:00:00.000Z","expiresAt":"2026-11-19T12:00:00.000Z"}})
}
#[test]
fn exact_input_review_receipt_and_no_local_accounting_defaults() {
    let r = review();
    assert!(r.ready());
    assert!(r.current(&r.before));
    assert_eq!(r.input["lines"][0]["creditAmount"], "0.00");
    assert_eq!(r.plan["sourceBasis"]["currency"], "EUR");
    let body = r.apply_body(" original-key ").unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["idempotencyKey"], " original-key ");
    assert_eq!(v["input"], r.input);
    attempt_body(&body, &r.plan).unwrap();
    assert!(r.receipt(&record()));
    assert!(r.canonical(&record()));
    for field in [
        "actor",
        "cutOverDate",
        "planHash",
        "journalEntryHash",
        "sourceBasis",
    ] {
        let mut bad = record();
        bad["openingBalance"][field] = json!("changed");
        assert!(!r.canonical(&bad));
    }
    let mut bad = record();
    bad["idempotency"]["receiptId"] = json!(0);
    assert!(!r.receipt(&bad));
}
#[test]
fn altered_or_blocked_plan_or_changed_current_binding_cannot_be_sent() {
    let r = review();
    for field in ["profile", "chart", "me", "journal", "opening"] {
        let mut changed = r.before.clone();
        match field {
            "profile" => changed.profile["currency"] = json!("DKK"),
            "chart" => changed.chart["accounts"] = json!([]),
            "me" => changed.me["user"]["id"] = json!("other"),
            "journal" => changed.journal["entries"] = json!([{"id":7}]),
            _ => changed.opening = json!({"planHash":"other"}),
        };
        assert!(!r.current(&changed));
    }
    let bytes = serde_json::to_vec(&r.input).unwrap();
    let mut altered = r.plan.clone();
    altered["input"]["cutOverDate"] = json!("2024-01-01");
    assert!(Review::parse(r.before.clone(), &bytes, json!({"ok":true,"plan":altered})).is_err());
    let mut blocked = r.clone();
    blocked.plan["status"] = json!("blocked");
    blocked.plan["blockers"] = json!(["Human document evidence required"]);
    assert!(!blocked.ready());
    assert!(blocked.apply_body("key").is_err());
    let mut forged = r.apply_body("key").unwrap();
    forged.extend_from_slice(b"garbage");
    assert!(attempt_body(&forged, &r.plan).is_err());
}
#[test]
fn money_citations_accounts_and_server_year_bounds_are_explicit() {
    let data = snapshot();
    for amount in ["1,00", "01.00", "1.0", "-1.00", "1e2", " 1.00"] {
        assert!(
            input(
                "2024-07-01",
                &[
                    LineInput {
                        account_no: "A".into(),
                        debit: amount.into(),
                        ..Default::default()
                    },
                    LineInput {
                        account_no: "E".into(),
                        credit: "100.00".into(),
                        ..Default::default()
                    }
                ],
                "",
                &[],
                &data
            )
            .is_err()
        );
    }
    let mut line = LineInput {
        account_no: "missing".into(),
        debit: "1.00".into(),
        ..Default::default()
    };
    assert!(input("2024-07-01", &[line.clone(), line.clone()], "", &[], &data).is_err());
    line.account_no = "A".into();
    assert!(input("2024-07-01", &[line.clone(), line], "", &[1, 1], &data).is_err());
    let response = json!({"ok":true,"journal":data.journal});
    assert_eq!(
        journal_year(&response, "2024-07-01").as_deref(),
        Some("explicit-shifted-label")
    );
    assert!(journal_year(&response, "2024-06-30").is_none());
    let mut journal = response;
    let opening = record()["openingBalance"].clone();
    journal["journal"]["entries"] = json!([{"id":1,"entryNo":"explicit-1","date":"2024-07-01","reversalOfEntryId":null,"reversedByEntryId":null}]);
    assert!(journal_matches(
        &journal,
        &opening,
        "synthetic",
        &data.ledger
    ));
    assert!(!journal_matches(&journal, &opening, "other", &data.ledger));
    journal["journal"]["entries"][0]["reversedByEntryId"] = json!(9);
    assert!(!journal_matches(
        &journal,
        &opening,
        "synthetic",
        &data.ledger
    ));
}
