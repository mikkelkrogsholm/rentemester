use super::*;
use serde_json::json;
fn snapshot() -> Value {
    json!({"invoiceType":"full","vatTreatment":"standard","status":"issued","invoiceNumber":"SYN-2026-1","issueDate":"2026-10-20","dueDate":"2026-11-03","currency":"DKK","seller":{"name":"Synthetic Seller","address":"Synthetic Street 1","vatOrCvr":"DK12345678"},"buyer":{"name":"Synthetic Buyer","address":"Synthetic Street 2","eanNumber":"5790000000000","publicRecipient":true},"lines":[{"description":"Synthetic service","quantity":1,"unitPriceExVat":100,"lineTotalExVat":100}],"totals":{"netAmount":100,"vatRate":0.25,"vatAmount":25,"grossAmount":125},"futureEvidence":{"immutable":"retained"}})
}
#[test]
fn deterministic_source_profile_namespaces_amounts_and_sha() {
    let p = snapshot();
    let copy = p.clone();
    let a = export("SYN-2026-1", &p).unwrap();
    let b = export("SYN-2026-1", &p).unwrap();
    assert_eq!(a, b);
    assert_eq!(p, copy);
    assert_eq!(a.receiver_endpoint_id, "5790000000000");
    assert_eq!(a.sha256, format!("{:x}", Sha256::digest(a.xml.as_bytes())));
    for text in [
        "urn:oasis:names:specification:ubl:schema:xsd:Invoice-2",
        "<cbc:UBLVersionID>2.0</cbc:UBLVersionID>",
        "<cbc:CustomizationID>OIOUBL-2.02</cbc:CustomizationID>",
        "Procurement-BilSim-1.0",
        "schemeID=\"DK:CVR\">DK12345678",
        "schemeID=\"GLN\">5790000000000",
        "<cbc:BuyerReference>SYN-2026-1</cbc:BuyerReference>",
        "unitCode=\"EA\">1",
        "<cbc:PriceAmount currencyID=\"DKK\">100.00</cbc:PriceAmount>",
        "<cbc:TaxAmount currencyID=\"DKK\">25.00</cbc:TaxAmount>",
        "<cbc:PayableAmount currencyID=\"DKK\">125.00</cbc:PayableAmount>",
    ] {
        assert!(a.xml.contains(text), "missing {text}");
    }
    assert!(!a.xml.ends_with('\n'));
}
#[test]
fn escapes_all_xml_contexts_and_keeps_snapshot_references() {
    let mut p = snapshot();
    p["seller"]["name"] = json!("Seller & <A> \"Q\" 'S'");
    p["buyer"]["buyerReference"] = json!("department <one>");
    p["buyer"]["orderReference"] = json!("order & two");
    p["lines"][0]["unitCode"] = json!("EA\" injected=\"x");
    p["lines"][0]["description"] = json!("Service & <data>");
    let xml = export("SYN-2026-1", &p).unwrap().xml;
    assert!(xml.contains("Seller &amp; &lt;A&gt; &quot;Q&quot; &apos;S&apos;"));
    assert!(xml.contains("<cbc:BuyerReference>department &lt;one&gt;</cbc:BuyerReference>"));
    assert!(xml.contains("<cac:OrderReference>\n    <cbc:ID>order &amp; two</cbc:ID>"));
    assert!(xml.contains("unitCode=\"EA&quot; injected=&quot;x\""));
    assert!(!xml.contains("<data>"));
}
#[test]
fn source_unit_and_reference_fallbacks_and_ean_eligibility() {
    let mut p = snapshot();
    p["buyer"]["publicRecipient"] = json!(false);
    p["buyer"]["eanNumber"] = json!("579 0000000000");
    p["buyer"]["orderReference"] = json!("source-order");
    p["unitCode"] = json!("H87");
    let r = export("SYN-2026-1", &p).unwrap();
    assert_eq!(r.receiver_endpoint_id, "5790000000000");
    assert!(r.xml.contains("unitCode=\"EA\""));
    assert!(
        r.xml
            .contains("<cbc:BuyerReference>source-order</cbc:BuyerReference>")
    );
    p["buyer"]["eanNumber"] = Value::Null;
    assert!(export("SYN-2026-1", &p).is_err());
}
#[test]
fn exact_subcent_price_and_fractional_quantity_never_round_in_xml() {
    let mut p = snapshot();
    p["lines"][0]["quantity"] = json!(3);
    p["lines"][0]["unitPriceExVat"] = json!(0.335);
    p["lines"][0]["lineTotalExVat"] = json!(1.01);
    p["totals"] = json!({"netAmount":1.01,"vatRate":0.25,"vatAmount":0.25,"grossAmount":1.26});
    let r = export("SYN-2026-1", &p).unwrap();
    assert!(r.xml.contains(">0.335</cbc:PriceAmount>"));
    assert!(!r.xml.contains(">0.34</cbc:PriceAmount>"));
    p["lines"][0]["quantity"] = json!(1e-3);
    p["lines"][0]["unitPriceExVat"] = json!(1000);
    p["lines"][0]["lineTotalExVat"] = json!(1);
    p["totals"] = json!({"netAmount":1,"vatRate":0.25,"vatAmount":0.25,"grossAmount":1.25});
    assert!(
        export("SYN-2026-1", &p)
            .unwrap()
            .xml
            .contains(">0.001</cbc:InvoicedQuantity>")
    );
    assert_eq!(number(&json!(1e20)).unwrap(), "100000000000000000000");
    assert_eq!(number(&json!(1e-20)).unwrap(), "0.00000000000000000001");
}
#[test]
fn refuses_recorded_money_line_math_and_immutable_vat_rounding_mismatch() {
    for (path, value) in [
        (vec!["totals", "netAmount"], json!(100.001)),
        (vec!["totals", "grossAmount"], json!(126)),
        (vec!["totals", "vatAmount"], json!(24)),
        (vec!["totals", "netAmount"], json!("100.00")),
    ] {
        let mut p = snapshot();
        p[path[0]][path[1]] = value;
        assert!(export("SYN-2026-1", &p).is_err());
    }
    let mut p = snapshot();
    p["lines"][0]["lineTotalExVat"] = json!(101);
    assert!(export("SYN-2026-1", &p).is_err());
    p["lines"] = json!([{"description":"tiny one","quantity":1,"unitPriceExVat":0.01,"lineTotalExVat":0.01},{"description":"tiny two","quantity":1,"unitPriceExVat":0.01,"lineTotalExVat":0.01}]);
    p["totals"] = json!({"netAmount":0.02,"vatRate":0.25,"vatAmount":0.01,"grossAmount":0.03});
    let error = export("SYN-2026-1", &p).unwrap_err();
    assert!(error.message.contains("never rewrites"));
}
#[test]
fn refuses_bad_dates_endpoints_and_unsupported_tax_or_currency() {
    for (field, value) in [
        ("issueDate", json!("2026-02-30")),
        ("dueDate", json!("2026-10-19")),
        ("currency", json!("EUR")),
        ("vatTreatment", json!("domestic_reverse_charge")),
        ("invoiceType", json!("credit_note")),
        ("invoiceNumber", json!("other")),
    ] {
        let mut p = snapshot();
        p[field] = value;
        assert!(export("SYN-2026-1", &p).is_err());
    }
    for bad in [json!("SE12345678"), json!("DK123"), Value::Null] {
        let mut p = snapshot();
        p["seller"]["vatOrCvr"] = bad;
        assert!(export("SYN-2026-1", &p).is_err());
    }
    for bad in [
        json!("579000000000"),
        json!("579000000000x"),
        json!("5790000000000\u{85}"),
    ] {
        let mut p = snapshot();
        p["buyer"]["eanNumber"] = bad;
        assert!(export("SYN-2026-1", &p).is_err());
    }
    let mut p = snapshot();
    p["lines"][0]["taxClassification"] = json!("exempt");
    assert!(export("SYN-2026-1", &p).is_err());
    p = snapshot();
    p["totals"]["vatRate"] = json!(25);
    assert!(export("SYN-2026-1", &p).is_err());
}
#[test]
fn bounded_text_and_xml_and_illegal_codepoints_fail_closed() {
    let mut p = snapshot();
    p["buyer"]["name"] = json!("bad\u{0}name");
    assert!(export("SYN-2026-1", &p).is_err());
    p = snapshot();
    p["buyer"]["address"] = json!("a".repeat(MAX_TEXT + 1));
    assert!(export("SYN-2026-1", &p).is_err());
    let mut xml = Xml {
        value: String::new(),
    };
    assert!(xml.line(&"x".repeat(MAX_XML)).is_err());
    p = snapshot();
    p["lines"] = json!([{"description":"a","quantity":0,"unitPriceExVat":100,"lineTotalExVat":0}]);
    assert!(export("SYN-2026-1", &p).is_err());
}
