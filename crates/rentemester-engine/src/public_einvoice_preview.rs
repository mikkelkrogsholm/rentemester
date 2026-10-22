//! Original public-recipient preview over an immutable snapshot. This is a
//! preview format, independent of the stricter OIOUBL transport exporter.
use crate::{Error, money::Money, public_einvoice::Export};
use serde_json::Value;
use sha2::{Digest, Sha256};

fn refused(message: String) -> Error {
    Error::new("public_einvoice_preview_refused", message)
}

// ECMAScript whitespace (notably includes BOM and excludes U+0085).
fn whitespace(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\u{b}' | '\u{c}' | '\r' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200a}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202f}'
                | '\u{205f}'
                | '\u{3000}'
                | '\u{feff}'
    )
}
fn number(n: f64) -> String {
    ryu_js::Buffer::new().format(n).to_owned()
}
fn string(value: &Value) -> Option<String> {
    match value {
        Value::Null => None,
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => n.as_f64().map(number),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}
fn amount(value: &Value) -> Result<Option<String>, Error> {
    if value.is_null() || value.as_str() == Some("") {
        return Ok(None);
    }
    let n = match value {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => {
            let s = s.trim_matches(whitespace);
            if s.is_empty() {
                Some(0.0)
            } else {
                s.parse::<f64>().ok()
            }
        }
        _ => None,
    };
    let Some(n) = n.filter(|n| n.is_finite()) else {
        return Ok(None);
    };
    Ok(Some(Money::parse(&number(n))?.decimal()))
}
fn tag(lines: &mut Vec<String>, indent: &str, name: &str, value: Option<String>) {
    if let Some(value) = value.filter(|v| !v.is_empty()) {
        let value = value
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
            .replace('\'', "&apos;");
        lines.push(format!("{indent}<{name}>{value}</{name}>"));
    }
}
fn numeric(value: &Value) -> Option<String> {
    value.is_number().then(|| string(value)).flatten()
}
/// Source preview rules accept a normalized EAN even without the public flag.
/// Recorded amounts and classifications are rendered, never recalculated.
pub fn export(invoice_number: &str, payload: &Value) -> Result<Export, Error> {
    let invoice_number = payload["invoiceNumber"].as_str().unwrap_or(invoice_number);
    let buyer = &payload["buyer"];
    let ean = buyer["eanNumber"]
        .as_str()
        .map(|raw| raw.chars().filter(|c| !whitespace(*c)).collect::<String>())
        .filter(|s| s.len() == 13 && s.bytes().all(|b| b.is_ascii_digit()));
    if buyer["publicRecipient"] != true && ean.is_none() {
        return Err(refused(format!(
            "invoice {invoice_number} is not marked as a public-recipient e-invoice"
        )));
    }
    let ean = ean.ok_or_else(|| {
        refused(format!(
            "invoice {invoice_number} is missing buyer.eanNumber as 13 digits"
        ))
    })?;
    let mut xml = vec![
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>".to_owned(),
        "<PublicEInvoicePreview xmlns=\"urn:rentemester:dk:public-einvoice-preview:v1\">"
            .to_owned(),
    ];
    tag(
        &mut xml,
        "  ",
        "InvoiceNumber",
        Some(invoice_number.to_owned()),
    );
    for (name, key) in [("IssueDate", "issueDate"), ("DueDate", "dueDate")] {
        tag(&mut xml, "  ", name, string(&payload[key]));
    }
    tag(
        &mut xml,
        "  ",
        "Currency",
        Some(string(&payload["currency"]).unwrap_or_else(|| "DKK".into())),
    );
    tag(
        &mut xml,
        "  ",
        "Profile",
        Some("public-recipient-preview-only".into()),
    );
    tag(
        &mut xml,
        "  ",
        "Transport",
        Some("out_of_scope_peppol_access_point_required".into()),
    );
    for (name, key) in [("Seller", "seller"), ("Buyer", "buyer")] {
        xml.push(format!("  <{name}>"));
        for (name, field) in [
            ("Name", "name"),
            ("Address", "address"),
            ("VatOrCvr", "vatOrCvr"),
        ] {
            tag(&mut xml, "    ", name, string(&payload[key][field]));
        }
        if key == "buyer" {
            tag(&mut xml, "    ", "EanNumber", Some(ean.clone()));
        }
        xml.push(format!("  </{name}>"));
    }
    xml.push("  <Delivery>".into());
    for (name, key) in [
        ("DeliveryDate", "deliveryDate"),
        ("DeliveryPeriodStart", "deliveryPeriodStart"),
        ("DeliveryPeriodEnd", "deliveryPeriodEnd"),
    ] {
        tag(&mut xml, "    ", name, string(&payload[key]));
    }
    xml.push("  </Delivery>".into());
    xml.push("  <Totals>".into());
    tag(
        &mut xml,
        "    ",
        "NetAmount",
        amount(&payload["totals"]["netAmount"])?,
    );
    tag(
        &mut xml,
        "    ",
        "VatRate",
        numeric(&payload["totals"]["vatRate"]),
    );
    for (name, key) in [("VatAmount", "vatAmount"), ("GrossAmount", "grossAmount")] {
        tag(&mut xml, "    ", name, amount(&payload["totals"][key])?);
    }
    xml.push("  </Totals>".into());
    xml.push("  <Lines>".into());
    if let Some(items) = payload["lines"].as_array() {
        for (i, line) in items.iter().enumerate() {
            xml.push("      <Line>".into());
            tag(
                &mut xml,
                "        ",
                "LineNumber",
                Some((i + 1).to_string()),
            );
            tag(
                &mut xml,
                "        ",
                "Description",
                string(&line["description"]),
            );
            tag(&mut xml, "        ", "Quantity", numeric(&line["quantity"]));
            for (name, key) in [
                ("UnitPriceExVat", "unitPriceExVat"),
                ("LineTotalExVat", "lineTotalExVat"),
            ] {
                tag(&mut xml, "        ", name, amount(&line[key])?);
            }
            tag(
                &mut xml,
                "        ",
                "TaxClassification",
                string(&line["taxClassification"]),
            );
            tag(&mut xml, "        ", "VatRate", numeric(&line["vatRate"]));
            xml.push("      </Line>".into());
        }
    }
    xml.push("  </Lines>".into());
    xml.push("</PublicEInvoicePreview>".into());
    let xml = xml.join("\n");
    let sha256 = format!("{:x}", Sha256::digest(xml.as_bytes()));
    Ok(Export {
        xml,
        sha256,
        receiver_endpoint_id: ean,
    })
}

#[cfg(test)]
mod tests;
