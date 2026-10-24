//! Pure receive metadata conversion. A signed download URL is not XMLDSig evidence.
use crate::{Error, document::Metadata};
use roxmltree::{Document, Node, ParsingOptions};
use serde_json::{Value, json};

pub const MAX_XML_BYTES: usize = 25 * 1024 * 1024;
const MAX_NODES: u32 = 100_000;
const MAX_DEPTH: usize = 64;
fn invalid() -> Error {
    Error::new(
        "digisense_received_invalid",
        "Received XML or metadata is invalid",
    )
}
fn text(node: Node<'_, '_>, name: &str) -> Option<String> {
    node.descendants()
        .find(|n| n.is_element() && n.tag_name().name().eq_ignore_ascii_case(name))
        .and_then(|n| n.text())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}
fn party<'a, 'i>(root: Node<'a, 'i>, name: &str) -> Option<Node<'a, 'i>> {
    root.descendants()
        .find(|n| n.is_element() && n.tag_name().name().eq_ignore_ascii_case(name))
}
fn listed(record: &Value, key: &str) -> Option<String> {
    record.get(key).and_then(Value::as_str).map(str::to_owned)
}
fn nonempty(value: Option<String>) -> Option<String> {
    value.filter(|s| !s.is_empty())
}
pub fn normalize_participant(id: &str) -> &str {
    let trimmed = id.trim();
    trimmed.rsplit(':').next().unwrap_or(trimmed)
}
fn amount(value: Option<String>) -> Option<Value> {
    let filtered: String = value?
        .chars()
        .filter(|c| c.is_ascii_digit() || matches!(c, '.' | ',' | '-'))
        .collect();
    let n: f64 = filtered.replacen(',', ".", 1).parse().ok()?;
    serde_json::Number::from_f64(n).map(Value::Number)
}
fn identity(value: &str) -> String {
    normalize_participant(value)
        .trim()
        .to_ascii_uppercase()
        .strip_prefix("DK")
        .unwrap_or(normalize_participant(value).trim())
        .to_owned()
}
/// Fail closed on any explicit destination or XML recipient identifier mismatch.
/// The caller must obtain `legal_cvr` from current protected company state, not request JSON.
pub fn validate_destination(record: &Value, xml: &[u8], legal_cvr: &str) -> Result<(), Error> {
    let document = parse(xml)?;
    let root = document.root_element();
    let expected = identity(legal_cvr);
    if expected.len() != 8 || !expected.bytes().all(|c| c.is_ascii_digit()) {
        return Err(invalid());
    }
    let customer = party(root, "AccountingCustomerParty");
    let ids = listed(record, "destinationParticipantId")
        .into_iter()
        .chain(
            customer
                .into_iter()
                .flat_map(|n| n.descendants())
                .filter(|n| {
                    n.is_element() && matches!(n.tag_name().name(), "CompanyID" | "EndpointID")
                })
                .filter_map(|n| n.text().map(str::to_owned)),
        );
    for id in ids {
        if identity(&id) != expected {
            return Err(Error::new(
                "digisense_received_wrong_company",
                "Received document destination does not match the bound legal company",
            ));
        }
    }
    Ok(())
}
fn parse(xml: &[u8]) -> Result<Document<'_>, Error> {
    if xml.is_empty() || xml.len() > MAX_XML_BYTES {
        return Err(invalid());
    }
    let xml = std::str::from_utf8(xml).map_err(|_| invalid())?;
    let doc = Document::parse_with_options(
        xml,
        ParsingOptions {
            allow_dtd: false,
            nodes_limit: MAX_NODES,
        },
    )
    .map_err(|_| invalid())?;
    if doc
        .descendants()
        .any(|node| node.ancestors().take(MAX_DEPTH + 1).count() > MAX_DEPTH)
    {
        return Err(invalid());
    }
    Ok(doc)
}
/// Source precedence: listing fallback, UBL fields, then supported caller fields.
/// Neither conversion nor overrides weaken ordinary Metadata::validate requirements.
pub fn build_received_metadata(
    record: &Value,
    xml: &[u8],
    overrides: &Value,
) -> Result<Metadata, Error> {
    if !record.is_object() || !(overrides.is_null() || overrides.is_object()) {
        return Err(invalid());
    }
    let document = parse(xml)?;
    let root = document.root_element();
    let supplier = party(root, "AccountingSupplierParty");
    let customer = party(root, "AccountingCustomerParty");
    let invoice = text(root, "ID");
    let kind = listed(record, "documentType").unwrap_or_else(|| "undefined".into());
    let description = match invoice.as_ref() {
        Some(n) => format!("Modtaget e-faktura {n} ({kind})"),
        None => format!("Modtaget e-faktura ({kind})"),
    };
    let ptext = |node: Option<Node<'_, '_>>, name: &str| node.and_then(|n| text(n, name));
    let mut value = json!({
        "source":"digisense_modtag", "documentType":"purchase_sale",
        "issueDate":text(root,"IssueDate").or_else(|| listed(record,"receivedAt").map(|s|s.split('T').next().unwrap_or("").to_owned())),
        "invoiceNo":invoice.or_else(||listed(record,"documentId")),
        "deliveryDescription":description,
        "amountIncVat":amount(text(root,"PayableAmount")),
        "currency":text(root,"DocumentCurrencyCode").unwrap_or_else(||"DKK".into()),
        "sender":{
            "name":nonempty(listed(record,"senderName")).or_else(||ptext(supplier,"Name")),
            "address":ptext(supplier,"StreetName").or_else(||ptext(supplier,"Line")),
            "vatOrCvr":ptext(supplier,"CompanyID").or_else(||listed(record,"senderParticipantId").map(|s|normalize_participant(&s).to_owned()))
        },
        "recipient":{
            "name":nonempty(listed(record,"destinationName")).or_else(||ptext(customer,"Name")),
            "address":ptext(customer,"StreetName").or_else(||ptext(customer,"Line")),
            "vatOrCvr":ptext(customer,"CompanyID").or_else(||listed(record,"destinationParticipantId").map(|s|normalize_participant(&s).to_owned()))
        },
        "vatAmount":amount(text(root,"TaxAmount"))
    });
    if let Some(fields) = overrides.as_object() {
        for (key, replacement) in fields {
            if key == "source" {
                continue;
            }
            if matches!(key.as_str(), "sender" | "recipient") && replacement.is_object() {
                let target = value[key].as_object_mut().ok_or_else(invalid)?;
                for (field, v) in replacement.as_object().ok_or_else(invalid)? {
                    target.insert(field.clone(), v.clone());
                }
            } else {
                value[key] = replacement.clone();
            }
        }
    }
    // DigiSense inbound originals cannot be converted into another voucher type.
    if value["documentType"] != "purchase_sale" {
        return Err(invalid());
    }
    serde_json::from_value(value).map_err(|_| invalid())
}

#[cfg(test)]
mod tests {
    use super::*;
    const XML: &[u8] = br#"<Invoice><ID>INV-7</ID><IssueDate>2026-01-02</IssueDate><DocumentCurrencyCode>EUR</DocumentCurrencyCode><AccountingSupplierParty><Party><Name>XML supplier</Name><StreetName>Road 7</StreetName><CompanyID>DK12345678</CompanyID></Party></AccountingSupplierParty><AccountingCustomerParty><Party><Name>Buyer</Name><CompanyID>DK87654321</CompanyID></Party></AccountingCustomerParty><PayableAmount>125.50</PayableAmount><TaxAmount>25.10</TaxAmount></Invoice>"#;
    #[test]
    fn source_precedence_and_partial_party_override() {
        let record = json!({"senderName":"Listing supplier","documentType":"Invoice","documentId":"fallback","destinationParticipantId":"DK:CVR:87654321"});
        let m = build_received_metadata(
            &record,
            XML,
            &json!({"source":"other","invoiceNo":"human","sender":{"address":"Override road"}}),
        )
        .unwrap();
        assert_eq!(m.source, "digisense_modtag");
        assert_eq!(m.invoice_no.as_deref(), Some("human"));
        assert_eq!(m.currency.as_deref(), Some("EUR"));
        assert_eq!(m.amount_inc_vat.unwrap().as_f64(), Some(125.5));
        let sender = m.sender.unwrap();
        assert_eq!(sender.name.as_deref(), Some("Listing supplier"));
        assert_eq!(sender.address.as_deref(), Some("Override road"));
        assert_eq!(sender.vat_or_cvr.as_deref(), Some("DK12345678"));
        validate_destination(&record, XML, "87654321").unwrap();
    }
    #[test]
    fn malformed_entities_depth_and_unsupported_overrides_refuse() {
        for xml in [
            b"<Invoice>".as_slice(),
            b"<!DOCTYPE x [<!ENTITY y SYSTEM 'file:///tmp/no'>]><x>&y;</x>",
            &[0xff],
        ] {
            assert!(build_received_metadata(&json!({}), xml, &Value::Null).is_err());
        }
        let deep = format!("{}{}", "<x>".repeat(65), "</x>".repeat(65));
        assert!(build_received_metadata(&json!({}), deep.as_bytes(), &Value::Null).is_err());
        assert!(build_received_metadata(&json!({}), XML, &json!({"unknown":"x"})).is_err());
        assert!(
            build_received_metadata(&json!({}), XML, &json!({"documentType":"internal_voucher"}))
                .is_err()
        );
    }
    #[test]
    fn missing_fields_still_fail_normal_validation_and_overrides_cannot_hide_destination() {
        let m = build_received_metadata(&json!({}), b"<Invoice/>", &Value::Null).unwrap();
        assert!(m.validate().is_err());
        assert!(validate_destination(&json!({}), XML, "12345678").is_err());
        assert!(
            validate_destination(
                &json!({"destinationParticipantId":"DK:CVR:12345678"}),
                XML,
                "87654321"
            )
            .is_err()
        );
    }
}
