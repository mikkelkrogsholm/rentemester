//! Deterministic issue-time invoice evidence. GET serves saved bytes, never rerenders.
use serde_json::{Value, json};
struct Row {
    section: bool,
    total: bool,
    label: String,
    amount: String,
}
fn text(value: &Value) -> String {
    value.as_str().unwrap_or("").into()
}
fn amount(value: &Value) -> String {
    value
        .as_f64()
        .map(|v| format!("{v:.2}").replace('.', ","))
        .unwrap_or_default()
}
fn unit_price(value: &Value) -> String {
    let raw = value.to_string();
    if raw
        .split('.')
        .nth(1)
        .is_some_and(|fraction| fraction.len() > 2)
        || raw.contains(['e', 'E'])
    {
        raw.replace('.', ",")
    } else {
        amount(value)
    }
}
pub fn build(payload: &Value) -> Vec<u8> {
    let mut rows = Vec::new();
    let currency = payload["currency"].as_str().unwrap_or("DKK");
    for (label, value) in [
        ("Fakturanummer", text(&payload["invoiceNumber"])),
        ("Fakturadato", text(&payload["issueDate"])),
        ("Forfaldsdato", text(&payload["dueDate"])),
        ("Sælger", text(&payload["seller"]["name"])),
        ("Adresse", text(&payload["seller"]["address"])),
        ("CVR/moms", text(&payload["seller"]["vatOrCvr"])),
        ("Køber", text(&payload["buyer"]["name"])),
        ("Adresse", text(&payload["buyer"]["address"])),
        ("CVR/moms", text(&payload["buyer"]["vatOrCvr"])),
    ] {
        if value.is_empty() {
            continue;
        }
        let chars: Vec<_> = format!("{label}: {value}").chars().collect();
        for chunk in chars.chunks(80) {
            rows.push(Row {
                section: false,
                total: false,
                label: chunk.iter().collect(),
                amount: String::new(),
            });
        }
    }
    rows.push(Row {
        section: true,
        total: false,
        label: "Leverance".into(),
        amount: String::new(),
    });
    for line in payload["lines"].as_array().into_iter().flatten() {
        let description = text(&line["description"]);
        let chars: Vec<_> = description.chars().collect();
        for chunk in chars.chunks(72) {
            rows.push(Row {
                section: false,
                total: false,
                label: chunk.iter().collect(),
                amount: String::new(),
            });
        }
        if line.get("quantity").is_some() && line.get("unitPriceExVat").is_some() {
            rows.push(Row {
                section: false,
                total: false,
                label: format!(
                    "{} x {} {} ekskl. moms",
                    line["quantity"],
                    unit_price(&line["unitPriceExVat"]),
                    currency
                ),
                amount: amount(&line["lineTotalExVat"]),
            });
        }
        if let Some(class) = line["taxClassification"].as_str() {
            let label = match class {
                "taxable" => "Moms 25%".to_owned(),
                "exempt" => "Momsfritaget".to_owned(),
                "reverse_charge" => format!(
                    "Omvendt betalingspligt: {}",
                    text(&line["reverseChargeBasis"])
                ),
                _ => String::new(),
            };
            rows.push(Row {
                section: false,
                total: false,
                label,
                amount: String::new(),
            });
        }
    }
    let domestic = payload["vatTreatment"] == "domestic_reverse_charge";
    let explicit = payload["lines"].as_array().is_some_and(|lines| {
        lines
            .iter()
            .any(|line| line.get("taxClassification").is_some())
    });
    let uniform = payload["lines"].as_array().is_some_and(|lines| {
        lines
            .iter()
            .all(|line| line["taxClassification"] == "taxable")
    });
    let exempt = payload["lines"].as_array().is_some_and(|lines| {
        lines
            .iter()
            .all(|line| line["taxClassification"] == "exempt")
    });
    let vat_label = if !explicit || uniform {
        "Moms 25%"
    } else if exempt {
        "Momsfritaget"
    } else {
        "Moms i alt"
    };
    for (key, label) in [
        ("netAmount", "I alt ekskl. moms"),
        ("vatAmount", vat_label),
        ("grossAmount", "Til betaling DKK"),
    ] {
        if domestic && key == "vatAmount" {
            continue;
        }
        rows.push(Row {
            section: false,
            total: key == "grossAmount",
            label: if key == "grossAmount" {
                format!("Til betaling {currency}")
            } else {
                label.into()
            },
            amount: amount(&payload["totals"][key]),
        });
    }
    if currency != "DKK" {
        rows.push(Row {
            section: false,
            total: false,
            label: format!(
                "Kurs DKK pr. {currency}: {}",
                payload["totals"]["fxRateToDkk"]
            ),
            amount: String::new(),
        });
        for (key, label) in [
            ("netAmountDkk", "Grundlag DKK"),
            ("vatAmountDkk", "Moms DKK"),
            ("grossAmountDkk", "Bogføringsbeløb DKK"),
        ] {
            rows.push(Row {
                section: false,
                total: false,
                label: label.into(),
                amount: amount(&payload["totals"][key]),
            });
        }
    }
    for key in ["reverseChargeBasis", "reverseChargeNote"] {
        if let Some(value) = payload[key].as_str() {
            let chars: Vec<_> = value.chars().collect();
            for chunk in chars.chunks(80) {
                rows.push(Row {
                    section: false,
                    total: false,
                    label: chunk.iter().collect(),
                    amount: String::new(),
                });
            }
        }
    }
    if payload["payment"].is_object() {
        rows.push(Row {
            section: true,
            total: false,
            label: "Betaling".into(),
            amount: String::new(),
        });
        for (key, label) in [
            ("bankName", "Bank"),
            ("registrationNo", "Registreringsnr."),
            ("accountNo", "Kontonr."),
            ("iban", "IBAN"),
        ] {
            if let Some(s) = payload["payment"][key].as_str().filter(|s| !s.is_empty()) {
                rows.push(Row {
                    section: false,
                    total: false,
                    label: format!("{label}: {s}"),
                    amount: String::new(),
                });
            }
        }
    }
    let value = json!({"company":{"name":payload["seller"]["name"],"cvr":payload["seller"]["vatOrCvr"],"currency":currency},"selectedYear":payload["issueDate"].as_str().unwrap_or("").get(..4).unwrap_or("")});
    render(
        if payload["invoiceType"] == "simplified" {
            "Forenklet faktura"
        } else {
            "Faktura"
        },
        &value,
        payload["issueDate"].as_str().unwrap_or(""),
        &rows,
    )
}
fn pdf_text(value: &str) -> String {
    value
        .encode_utf16()
        .map(|c| match c {
            40 => "\\(".into(),
            41 => "\\)".into(),
            92 => "\\\\".into(),
            0..=127 => char::from_u32(u32::from(c)).unwrap().to_string(),
            128..=255 => format!("\\{c:03o}"),
            _ => "?".into(),
        })
        .collect()
}
fn draw(x: i32, y: i32, value: &str, size: i32, bold: bool) -> String {
    format!(
        "BT /{} {size} Tf {x} {y} Td ({}) Tj ET\n",
        if bold { "F2" } else { "F1" },
        pdf_text(value)
    )
}
fn render(title: &str, value: &Value, today: &str, rows: &[Row]) -> Vec<u8> {
    let pages = rows.len().max(1).div_ceil(44);
    let font1 = 3 + pages * 2;
    let font2 = font1 + 1;
    let info = font2 + 1;
    let kids = (0..pages)
        .map(|n| format!("{} 0 R", 3 + n * 2))
        .collect::<Vec<_>>()
        .join(" ");
    let mut objects = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".into(),
        format!("<< /Type /Pages /Count {pages} /Kids [{kids}] >>"),
    ];
    for page in 0..pages {
        let mut content = draw(50, 792, title, 16, true);
        let mut y = 768;
        let mut headers = vec![text(&value["company"]["name"])];
        if let Some(cvr) = value["company"]["cvr"].as_str().filter(|s| !s.is_empty()) {
            headers.push(format!("CVR {cvr}"));
        }
        headers.extend([
            format!("Regnskabsår {}", text(&value["selectedYear"])),
            format!("Valuta {}", text(&value["company"]["currency"])),
            format!("Udtrukket {today}"),
            "Kilde: Rentemester".into(),
        ]);
        for header in headers {
            content += &draw(50, y, &header, 10, false);
            y -= 14;
        }
        y -= 14;
        for row in rows.iter().skip(page * 44).take(44) {
            let size = if row.section { 11 } else { 10 };
            content += &draw(50, y, &row.label, size, row.section || row.total);
            if !row.amount.is_empty() {
                content += &draw(445, y, &row.amount, size, row.section || row.total);
            }
            if row.total {
                content += &format!("50 {} m 545 {} l 0.5 w S\n", y + 12, y + 12);
            }
            y -= 14;
        }
        objects.push(format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Resources << /Font << /F1 {font1} 0 R /F2 {font2} 0 R >> >> /Contents {} 0 R >>",4+page*2));
        objects.push(format!(
            "<< /Length {} >>\nstream\n{content}endstream",
            content.len()
        ));
    }
    objects.push(
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>".into(),
    );
    objects.push(
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold /Encoding /WinAnsiEncoding >>"
            .into(),
    );
    let date = today.replace('-', "");
    objects.push(format!("<< /Producer (Rentemester invoice renderer) /Title ({}) /CreationDate (D:{date}000000Z) /ModDate (D:{date}000000Z) >>",pdf_text(title)));
    let mut bytes = b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n".to_vec();
    let mut offsets = vec![0];
    for (index, object) in objects.iter().enumerate() {
        offsets.push(bytes.len());
        bytes.extend_from_slice(format!("{} 0 obj\n{object}\nendobj\n", index + 1).as_bytes());
    }
    let xref = bytes.len();
    bytes.extend_from_slice(
        format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes(),
    );
    for offset in offsets.iter().skip(1) {
        bytes.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    bytes.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R /Info {info} 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn printed_unit_price_keeps_the_precision_used_by_the_invoice_calculation() {
        let value = json!({"invoiceNumber":"SYN-1","issueDate":"2026-10-18","dueDate":"2026-11-01","seller":{"name":"Synthetic Seller","address":"Synthetic 1","vatOrCvr":"DK87654321"},"buyer":{"name":"Synthetic Buyer","address":"Synthetic 2","vatOrCvr":"DK12345678"},"lines":[{"description":"Synthetic","quantity":3,"unitPriceExVat":0.335,"lineTotalExVat":1.01}],"totals":{"netAmount":1.01,"vatAmount":0.25,"grossAmount":1.26}});
        let bytes = build(&value);
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("3 x 0,335 DKK ekskl. moms"));
        assert!(text.contains("1,01"));
        assert!(bytes.starts_with(b"%PDF-"));
        assert_eq!(bytes, build(&value));
    }
    #[test]
    fn mixed_exempt_and_reverse_pdfs_show_the_retained_tax_decisions() {
        for (extra, lines, label, absent) in [
            (
                json!({}),
                json!([
                {"description":"Taxable","quantity":1,"unitPriceExVat":100},
                {"description":"Exempt","quantity":1,"unitPriceExVat":50,"taxClassification":"exempt"}]),
                "Moms i alt",
                None,
            ),
            (
                json!({}),
                json!([{ "description":"Exempt","quantity":1,"unitPriceExVat":100,"taxClassification":"exempt"}]),
                "Momsfritaget",
                Some("Moms 25%"),
            ),
            (
                json!({"vatTreatment":"domestic_reverse_charge","reverseChargeBasis":"DK_MOMSLOVEN_§46_STK_1_NR_6","reverseChargeNote":"Omvendt betalingspligt"}),
                json!([{ "description":"Service","quantity":1,"unitPriceExVat":100}]),
                "Omvendt betalingspligt",
                Some("Moms 25%"),
            ),
        ] {
            let mut input = json!({"issueDate":"2026-10-20","lines":lines});
            input
                .as_object_mut()
                .unwrap()
                .extend(extra.as_object().unwrap().clone());
            let computed =
                crate::sales_invoice::compute(&serde_json::from_value(input).unwrap()).unwrap();
            let bytes = build(&computed);
            let text = String::from_utf8_lossy(&bytes);
            assert!(text.contains(label));
            if let Some(absent) = absent {
                assert!(!text.contains(absent));
            }
            assert_eq!(bytes, build(&computed));
        }
    }
}

#[cfg(test)]
mod foreign_tests {
    use super::*;
    #[test]
    fn foreign_pdf_labels_original_amount_and_separate_dkk_valuation() {
        let input=serde_json::from_value(json!({"issueDate":"2025-08-01","currency":"EUR","fxRateToDkk":7.46,"vatTreatment":"foreign_reverse_charge","reverseChargeBasis":"EU_B2B_SERVICES","reverseChargeNote":"Reverse charge","lines":[{"description":"Synthetic service","quantity":1,"unitPriceExVat":100}]})).unwrap();
        let value = crate::sales_invoice::compute(&input).unwrap();
        let bytes = build(&value);
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("100,00 EUR ekskl. moms"));
        assert!(text.contains("Til betaling EUR"));
        assert!(text.contains("Kurs DKK pr. EUR: 7.46"));
        assert!(text.contains("746,00"));
        assert!(!text.contains("Til betaling DKK"));
        assert_eq!(bytes, build(&value));
    }
}

#[cfg(test)]
mod simplified_tests {
    use super::*;
    #[test]
    fn reduced_pdf_has_truthful_title_and_no_null_quantity_or_invented_buyer() {
        let input=serde_json::from_value(json!({"invoiceType":"simplified","issueDate":"2026-10-20","lines":[{"description":"Synthetic reduced service"}],"totals":{"grossAmount":100.07,"vatRate":0.25,"vatComputationBasis":"VAT_20_OF_GROSS"}})).unwrap();
        let payload = crate::sales_invoice::compute(&input).unwrap();
        let bytes = build(&payload);
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("Forenklet faktura"));
        assert!(text.contains("Synthetic reduced service"));
        assert!(text.contains("80,06"));
        assert!(text.contains("20,01"));
        assert!(text.contains("100,07"));
        assert!(!text.contains("null"));
        assert!(!text.contains(" x "));
        assert!(!text.contains("Køber:"));
        assert_eq!(bytes, build(&payload));
    }
}
