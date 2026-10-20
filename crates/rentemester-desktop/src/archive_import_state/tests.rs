use super::*;
pub(crate) fn package() -> Package {
    Package::build(
        "DKK",
        vec![
            SourceFile {
                year: 2025,
                kind: FileKind::Postings,
                bytes: b"control\r\n".to_vec(),
            },
            SourceFile {
                year: 2024,
                kind: FileKind::Postings,
                bytes: b"\xef\xbb\xbfquoted;\"raw\r\nCSV\"\r\n".to_vec(),
            },
        ],
    )
    .unwrap()
}
pub(crate) fn scope() -> Scope {
    Scope {
        origin: "http://127.0.0.1:32100".into(),
        company: "synthetic".into(),
        principal: "synthetic-owner".into(),
        ledger_uuid: "a".repeat(32),
        local_identity: Some("synthetic-owned-profile".into()),
        can_apply: true,
    }
}
pub(crate) fn plan(package: &Package) -> Value {
    json!({"ok":true,"plan":{"format":"rentemester-native-closed-history/1","status":"ready","planHash":"b".repeat(64),"source":{"sha256":package.sha256(),"sizeBytes":package.size()},"declaredCurrency":"DKK","currencyDeclaration":"operator","excludedLatestYear":2025,"years":[{"fiscalYear":2024,"postingCount":3,"balanceCount":0,"hasBalances":false,"status":"new"}],"limitations":["No live posting or opening balance"]}})
}
pub(crate) fn review() -> Review {
    let p = package();
    let v = plan(&p);
    Review::prepare(p, scope(), v, "synthetic-original-key".into()).unwrap()
}
fn archives(review: &Review) -> Value {
    json!({"ok":true,"errors":[],"years":[{"fiscalYear":2024,"postingCount":3,"balanceCount":0,"hasBalances":false,"sourceSystem":"dinero","parserContract":"rentemester-native-closed-history/1","declaredCurrency":"DKK","sourceHash":review.package().sha256(),"importId":7}]})
}
fn original(review: &Review) -> Value {
    json!({"ok":true,"contentType":"application/json","bytes":review.package().bytes()})
}
fn response(review: &Review) -> Value {
    json!({"ok":true,"archiveImport":{"importId":7,"sourceSha256":review.package().sha256(),"planHash":review.plan()["planHash"],"excludedLatestYear":2025,"declaredCurrency":"DKK","years":[{"fiscalYear":2024,"postingCount":3,"balanceCount":0}]},"idempotency":{"replayed":false,"receiptId":17,"createdAt":"2026-01-01T00:00:00Z","expiresAt":"2027-01-01T00:00:00Z"}})
}
#[test]
fn package_sorts_explicit_paths_and_preserves_original_bytes_bom_and_line_endings() {
    let p = package();
    let json: Value = serde_json::from_slice(p.bytes()).unwrap();
    assert_eq!(json["files"][0]["path"], "2024/Posteringer.csv");
    let bytes = STANDARD
        .decode(json["files"][0]["contentBase64"].as_str().unwrap())
        .unwrap();
    assert_eq!(bytes, b"\xef\xbb\xbfquoted;\"raw\r\nCSV\"\r\n");
    assert_eq!(p.sha256(), sha(p.bytes()));
    assert_eq!(p.currency(), "DKK");
    let outer: Value = serde_json::from_slice(&p.plan_body()).unwrap();
    assert_eq!(
        STANDARD
            .decode(outer["contentBase64"].as_str().unwrap())
            .unwrap(),
        p.bytes()
    );
}
#[test]
fn currency_missing_year_kind_duplicates_and_utf8_or_bounds_refuse_without_reencoding() {
    let source = || {
        vec![
            SourceFile {
                year: 2024,
                kind: FileKind::Postings,
                bytes: vec![],
            },
            SourceFile {
                year: 2025,
                kind: FileKind::Postings,
                bytes: vec![],
            },
        ]
    };
    for currency in ["", "dkk", "DK", "DKKK", "D K"] {
        assert!(Package::build(currency, source()).is_err());
    }
    let mut files = source();
    files[1].year = 2024;
    assert!(Package::build("DKK", files).is_err());
    let mut files = source();
    files[1].kind = FileKind::Balances;
    assert!(Package::build("DKK", files).is_err());
    let mut files = source();
    files[1].bytes = vec![0xff];
    assert!(Package::build("DKK", files).is_err());
    let mut files = source();
    files[0].bytes = vec![b'x'; MAX_FILE + 1];
    assert!(Package::build("DKK", files).is_err());
    let mut files = source();
    files[0].bytes = vec![b'x'; MAX_FILE];
    files[1].bytes = vec![b'x'; 3 * 1024 * 1024];
    assert!(Package::build("DKK", files).is_err());
}
#[test]
fn plan_must_match_complete_source_currency_exclusion_years_and_counts() {
    let p = package();
    for path in [
        "hash",
        "size",
        "currency",
        "excluded",
        "year",
        "duplicate",
        "status",
    ] {
        let mut v = plan(&p);
        match path {
            "hash" => v["plan"]["source"]["sha256"] = json!("c".repeat(64)),
            "size" => v["plan"]["source"]["sizeBytes"] = json!(0),
            "currency" => v["plan"]["declaredCurrency"] = json!("EUR"),
            "excluded" => v["plan"]["excludedLatestYear"] = json!(2026),
            "year" => v["plan"]["years"][0]["fiscalYear"] = json!(2023),
            "duplicate" => {
                let row = v["plan"]["years"][0].clone();
                v["plan"]["years"].as_array_mut().unwrap().push(row)
            }
            _ => v["plan"]["years"][0]["status"] = json!("imagined"),
        };
        assert!(
            Review::prepare(p.clone(), scope(), v, "key".into()).is_err(),
            "{path}"
        );
    }
}
#[test]
fn recovery_uses_original_plan_and_key_and_requires_exact_reselected_source_and_body() {
    let r = review();
    let attempt = Attempt::new(&r).unwrap();
    let recovered = Review::recover(package(), scope(), &attempt).unwrap();
    assert_eq!(recovered.body(), r.body());
    assert_eq!(recovered.plan(), r.plan());
    let changed = Package::build(
        "DKK",
        vec![
            SourceFile {
                year: 2024,
                kind: FileKind::Postings,
                bytes: b"changed".to_vec(),
            },
            SourceFile {
                year: 2025,
                kind: FileKind::Postings,
                bytes: b"control".to_vec(),
            },
        ],
    )
    .unwrap();
    assert!(Review::recover(changed, scope(), &attempt).is_err());
    let mut corrupt = attempt.clone();
    corrupt.body_sha256 = "0".repeat(64);
    assert!(Review::recover(package(), scope(), &corrupt).is_err());
    let mut changed = attempt;
    changed.idempotency_key = "replacement-key".into();
    assert!(Review::recover(package(), scope(), &changed).is_err());
}
#[test]
fn current_scope_and_read_context_cannot_adopt_another_principal_ledger_or_origin() {
    let r = review();
    for part in ["origin", "company", "principal", "ledger", "local", "right"] {
        let mut s = scope();
        match part {
            "origin" => s.origin.push('x'),
            "company" => s.company.push('x'),
            "principal" => s.principal.push('x'),
            "ledger" => s.ledger_uuid = "c".repeat(32),
            "local" => s.local_identity = None,
            _ => s.can_apply = false,
        };
        assert!(!r.current(&s), "{part}");
    }
    let journal = json!({"ok":true,"journal":{"slug":"synthetic","ledgerUuid":"a".repeat(32)}});
    let me = json!({"ok":true,"user":{"id":"u"},"companies":[{"slug":"synthetic","role":"bookkeeper","archived":false}]});
    assert!(
        !Scope::parse(
            "https://synthetic.invalid",
            "synthetic",
            None,
            &me,
            &journal
        )
        .unwrap()
        .can_apply
    );
    assert!(Scope::parse("https://synthetic.invalid", "other", None, &me, &journal).is_err());
}
#[test]
fn receipt_needs_exact_plan_full_years_canonical_archive_and_physical_source() {
    let r = review();
    let good = response(&r);
    let list = archives(&r);
    let source = original(&r);
    assert_eq!(receipt(&r, &good).unwrap(), 7);
    assert!(verify_receipt(&r, &good, &list, &source).is_ok());
    let mut unconfirmed = good.clone();
    unconfirmed.as_object_mut().unwrap().remove("idempotency");
    assert!(verify_receipt(&r, &unconfirmed, &list, &source).is_err());
    let mut wrong = good.clone();
    wrong["archiveImport"]["planHash"] = json!("d".repeat(64));
    assert!(verify_receipt(&r, &wrong, &list, &source).is_err());
    let mut missing = list.clone();
    missing["years"] = json!([]);
    assert!(verify_receipt(&r, &good, &missing, &source).is_err());
    let mut tampered = source.clone();
    tampered["bytes"] = json!([123, 125]);
    assert!(verify_receipt(&r, &good, &list, &tampered).is_err());
    assert!(verify_receipt(&r, &json!({"ok":true}), &list, &source).is_err());
}
#[test]
fn reconciliation_never_becomes_receipt_and_needs_each_current_physical_original() {
    let r = review();
    let list = archives(&r);
    assert_eq!(reconciliation_original_ids(&r, &list).unwrap(), vec![7]);
    assert!(verify_reconciliation(&r, &list, &[]).is_err());
    assert!(verify_reconciliation(&r, &list, &[(7, original(&r))]).is_ok());
    assert!(receipt(&r, &list).is_err());
    assert!(verify_reconciliation(&r, &json!({"ok":true,"errors":[],"years":[]}), &[]).is_ok());
    for code in [
        "IDEMPOTENCY_CONFLICT",
        "IDEMPOTENCY_OUTCOME_EXPIRED",
        "RESTORED_KEY_RECONCILIATION_REQUIRED",
        "internal",
        "outcome_unknown",
    ] {
        assert!(!known_refusal(&json!({"ok":false,"code":code})));
    }
    assert!(known_refusal(
        &json!({"ok":false,"code":"archive_plan_changed"})
    ));
}
