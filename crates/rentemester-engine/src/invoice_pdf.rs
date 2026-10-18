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
        rows.push(Row {
            section: false,
            total: false,
            label: format!(
                "{} x {} DKK ekskl. moms",
                line["quantity"],
                unit_price(&line["unitPriceExVat"])
            ),
            amount: amount(&line["lineTotalExVat"]),
        });
    }
    for (key, label) in [
        ("netAmount", "I alt ekskl. moms"),
        ("vatAmount", "Moms 25%"),
        ("grossAmount", "Til betaling DKK"),
    ] {
        rows.push(Row {
            section: false,
            total: key == "grossAmount",
            label: label.into(),
            amount: amount(&payload["totals"][key]),
        });
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
    let value = json!({"company":{"name":payload["seller"]["name"],"cvr":payload["seller"]["vatOrCvr"],"currency":"DKK"},"selectedYear":payload["issueDate"].as_str().unwrap_or("").get(..4).unwrap_or("")});
    render(
        "Faktura",
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
}
