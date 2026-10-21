use super::*;
use serde_json::json;
fn schedule() -> Value {
    json!({"contract":"rentemester-imported-receivables-v1","sourceDocumentHash":"a".repeat(64),"invoices":[{"id":"opening","invoiceDate":"2024-12-20","grossAmount":30000,"controlAccountNo":"5520","recognitionRef":"opening","documentHash":"b".repeat(64),"payments":[{"id":"paid","paymentDate":"2025-03-05","amount":30000,"paymentRef":"source-payment","documentHash":"c".repeat(64)}]},{"id":"current","invoiceDate":"2025-03-20","grossAmount":25000,"controlAccountNo":"5520","recognitionRef":"source-invoice","documentHash":"d".repeat(64),"payments":[{"id":"credit","eventKind":"credit_note","paymentDate":"2025-03-21","amount":500,"paymentRef":"source-credit","documentHash":"e".repeat(64)}]}]})
}
#[test]
fn dated_exact_balances_include_fully_settled_sources() {
    let s = validate(&schedule()).unwrap();
    for (d, want) in [
        ("2024-12-19", "0.00"),
        ("2025-01-01", "30000.00"),
        ("2025-03-05", "0.00"),
        ("2025-03-20", "25000.00"),
        ("2025-03-21", "24500.00"),
    ] {
        assert_eq!(balance(&s, Date::parse(d).unwrap(), "5520").decimal(), want);
    }
    assert_eq!(s.invoices.len(), 2);
    assert_eq!(s.invoices[0].events[0].event_kind, "payment");
}
#[test]
fn canonical_preserves_declaration_shape_and_ecmascript_numbers() {
    let v: Value =
        serde_json::from_str(r#"{"z":1e-7,"a":1.0,"x":1e20,"b":1e21,"small":-0.0}"#).unwrap();
    assert_eq!(
        canonical(&v).unwrap(),
        r#"{"a":1,"b":1e+21,"small":0,"x":100000000000000000000,"z":1e-7}"#
    );
    let mut v = schedule();
    let a = validate(&v).unwrap();
    v["invoices"][0]["payments"][0]["eventKind"] = json!("payment");
    let b = validate(&v).unwrap();
    assert_ne!(a.hash, b.hash);
    assert_eq!(
        a.invoices[0].events[0].event_kind,
        b.invoices[0].events[0].event_kind
    );
}
#[test]
fn invalid_source_evidence_or_overpayment_is_rejected() {
    for mutate in 0..7 {
        let mut v = schedule();
        match mutate {
            0 => v["invoices"][0]["grossAmount"] = json!(0.001),
            1 => v["invoices"][0]["payments"][0]["amount"] = json!(30000.01),
            2 => v["invoices"][1]["id"] = json!(" opening "),
            3 => v["invoices"][0]["payments"][0]["paymentDate"] = json!("2024-12-19"),
            4 => v["invoices"][0]["documentHash"] = json!("no proof"),
            5 => v["invoices"][0]["dueDate"] = json!("2024-12-19"),
            _ => v["invoices"][0]["payments"][0]["eventKind"] = json!("refund"),
        };
        assert!(validate(&v).is_err(), "{mutate}");
    }
}
#[test]
fn original_external_identity_and_reference_are_retained() {
    let mut v = schedule();
    v["invoices"][0]["id"] = json!(" opening ");
    v["invoices"][0]["recognitionRef"] = json!(" ref ");
    v["invoices"][0]["payments"][0]["id"] = json!(" paid ");
    v["invoices"][0]["payments"][0]["paymentRef"] = json!(" receipt ");
    v["invoices"][0]["customerName"] = json!(" Customer ");
    let s = validate(&v).unwrap();
    assert_eq!(s.invoices[0].id, " opening ");
    assert_eq!(s.invoices[0].recognition_ref, " ref ");
    assert_eq!(s.invoices[0].events[0].id, " paid ");
    assert_eq!(s.invoices[0].events[0].payment_ref, " receipt ");
    assert_eq!(s.invoices[0].customer_name.as_deref(), Some("Customer"));
}

#[test]
fn duplicate_identity_uses_ecmascript_whitespace() {
    let mut v = schedule();
    v["invoices"][1]["id"] = json!("\u{feff}opening\u{feff}");
    assert!(validate(&v).is_err());
}
