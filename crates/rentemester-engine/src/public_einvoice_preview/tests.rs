use super::*;
use serde_json::json;

#[test]
fn matches_original_typescript_complete_bytes_and_hash() {
    // Fixtures were generated with the original buildPublicEInvoiceXml and
    // formatAmount functions, extracted from src/core/public-einvoice.ts.
    for (input, expected, hash) in [
        (
            include_str!("rich.json"),
            include_str!("rich.xml"),
            include_str!("rich.sha256"),
        ),
        (
            include_str!("sparse.json"),
            include_str!("sparse.xml"),
            include_str!("sparse.sha256"),
        ),
    ] {
        let case: Value = serde_json::from_str(input).unwrap();
        let original = case["payload"].clone();
        let result = export(case["invoiceNumber"].as_str().unwrap(), &case["payload"]).unwrap();
        assert_eq!(result.xml, expected);
        assert_eq!(result.sha256, hash);
        assert_eq!(result.receiver_endpoint_id, "1234567890123");
        assert_eq!(case["payload"], original);
    }
}

#[test]
fn source_public_recipient_and_ean_failures() {
    for payload in [
        json!({}),
        json!({"buyer":{"publicRecipient":false}}),
        json!({"buyer":{"eanNumber":"123-4567890123"}}),
    ] {
        let error = export("PRE003", &payload).unwrap_err();
        assert_eq!(error.code, "public_einvoice_preview_refused");
        assert_eq!(
            error.message,
            "invoice PRE003 is not marked as a public-recipient e-invoice"
        );
    }
    for ean in [
        Value::Null,
        json!(1234567890123_u64),
        json!("123"),
        json!("1234567890123\u{85}"),
    ] {
        let error = export(
            "PRE004",
            &json!({"buyer":{"publicRecipient":true,"eanNumber":ean}}),
        )
        .unwrap_err();
        assert_eq!(
            error.message,
            "invoice PRE004 is missing buyer.eanNumber as 13 digits"
        );
    }
    assert!(export("PRE005", &json!({"buyer":{"eanNumber":"1234567890123"}})).is_ok());
}

#[test]
fn snapshot_number_wins_and_optional_empty_currency_is_omitted() {
    let result = export(
        "ROW",
        &json!({"invoiceNumber":"SNAPSHOT", "currency":"", "buyer":{"eanNumber":"1234567890123"}}),
    )
    .unwrap();
    assert!(
        result
            .xml
            .contains("<InvoiceNumber>SNAPSHOT</InvoiceNumber>")
    );
    assert!(!result.xml.contains("<Currency>"));
    assert!(!result.xml.ends_with('\n'));
}
