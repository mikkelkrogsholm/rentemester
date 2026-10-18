//! Deterministic exports of the same authorized report snapshot shown in the cockpit.
use rentemester_engine::money::Money;
use serde_json::Value;

pub struct Export {
    pub bytes: Vec<u8>,
    pub filename: String,
    pub content_type: &'static str,
}
fn text(value: &Value) -> String {
    value.as_str().unwrap_or("").to_owned()
}
fn amount(value: &Value) -> String {
    value
        .as_f64()
        .map(|v| format!("{v:.2}").replace('.', ","))
        .unwrap_or_default()
}
fn cell(value: &str) -> String {
    if value.contains([';', '"', '\r', '\n']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_owned()
    }
}
fn csv_row(values: &[String]) -> String {
    values.iter().map(|s| cell(s)).collect::<Vec<_>>().join(";") + "\r\n"
}
fn safe(value: &str) -> String {
    value
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect()
}
#[derive(Clone)]
struct Row {
    section: bool,
    total: bool,
    label: String,
    amount: String,
}
fn rows(report: &str, value: &Value) -> Vec<Row> {
    let mut rows = Vec::new();
    let mut section = |title: &str, items: &Value, total: &Value, total_title: &str| {
        rows.push(Row {
            section: true,
            total: false,
            label: title.into(),
            amount: String::new(),
        });
        if let Some(items) = items.as_array() {
            for line in items {
                rows.push(Row {
                    section: false,
                    total: false,
                    label: format!(
                        "{} {}",
                        line["accountNo"]
                            .as_str()
                            .filter(|s| *s != "—")
                            .unwrap_or(""),
                        text(&line["name"])
                    )
                    .trim()
                    .into(),
                    amount: amount(&line["amount"]),
                });
            }
        }
        rows.push(Row {
            section: false,
            total: true,
            label: total_title.into(),
            amount: amount(total),
        });
    };
    match report {
        "income-statement" => {
            section(
                "Indtægter",
                &value["income"],
                &value["totalIncome"],
                "Indtægter i alt",
            );
            section(
                "Udgifter",
                &value["expense"],
                &value["totalExpense"],
                "Udgifter i alt",
            );
            rows.push(Row {
                section: false,
                total: true,
                label: "Årets resultat".into(),
                amount: amount(&value["result"]),
            });
        }
        "balance" => {
            for (key, title, total) in [
                ("assets", "Aktiver", "Aktiver i alt"),
                ("liabilities", "Passiver", "Gæld i alt"),
                ("equity", "Egenkapital", "Egenkapital i alt"),
            ] {
                section(title, &value[key]["lines"], &value[key]["total"], total);
            }
            rows.push(Row {
                section: false,
                total: true,
                label: "Passiver og egenkapital i alt".into(),
                amount: amount(&value["totalLiabilitiesAndEquity"]),
            });
        }
        _ => {
            rows.push(Row {
                section: true,
                total: false,
                label: "Konto · Saldo".into(),
                amount: String::new(),
            });
            if let Some(items) = value["rows"].as_array() {
                for line in items {
                    rows.push(Row {
                        section: false,
                        total: false,
                        label: format!("{} {}", text(&line["accountNo"]), text(&line["name"])),
                        amount: amount(&line["balance"]),
                    });
                }
            }
            rows.push(Row {
                section: false,
                total: true,
                label: "Debet i alt".into(),
                amount: amount(&value["totalDebit"]),
            });
            rows.push(Row {
                section: false,
                total: true,
                label: "Kredit i alt".into(),
                amount: amount(&value["totalCredit"]),
            });
        }
    }
    rows
}
fn csv(report: &str, title: &str, value: &Value, today: &str) -> Vec<u8> {
    let mut result = String::from("\u{feff}sep=;\r\n");
    for (name, item) in [
        ("Rapport", title.to_owned()),
        ("Virksomhed", text(&value["company"]["name"])),
        ("CVR", text(&value["company"]["cvr"])),
        ("Regnskabsår", text(&value["selectedYear"])),
        ("Valuta", text(&value["company"]["currency"])),
        ("Udtrukket", today.into()),
        ("Kilde", "Rentemester".into()),
    ] {
        if name == "CVR" && item.is_empty() {
            continue;
        }
        result += &csv_row(&[name.into(), item]);
    }
    result += "\r\n";
    if report == "journal" && value["accountFilter"].is_object() {
        result += &csv_row(&[
            "Kontofilter".into(),
            format!(
                "{} {}",
                text(&value["accountFilter"]["accountNo"]),
                text(&value["accountFilter"]["name"])
            ),
        ]);
        result += "\r\n";
    }
    match report {
        "trial-balance" => {
            result += &csv_row(&[
                "Konto".into(),
                "Navn".into(),
                "Type".into(),
                "Debet".into(),
                "Kredit".into(),
                "Saldo".into(),
            ]);
            if let Some(items) = value["rows"].as_array() {
                for line in items {
                    result += &csv_row(&[
                        text(&line["accountNo"]),
                        text(&line["name"]),
                        text(&line["type"]),
                        amount(&line["debit"]),
                        amount(&line["credit"]),
                        amount(&line["balance"]),
                    ]);
                }
            }
            result += &csv_row(&[
                "".into(),
                "I alt".into(),
                "".into(),
                amount(&value["totalDebit"]),
                amount(&value["totalCredit"]),
                "".into(),
            ]);
        }
        "journal" => {
            result += &csv_row(&[
                "Dato".into(),
                "Bilag".into(),
                "Konto".into(),
                "Kontonavn".into(),
                "Tekst".into(),
                "Debet".into(),
                "Kredit".into(),
            ]);
            let mut entries = value["entries"].as_array().cloned().unwrap_or_default();
            entries.sort_by_key(|entry| (text(&entry["date"]), text(&entry["entryNo"])));
            let mut debit = Money::default();
            let mut credit = Money::default();
            for entry in entries {
                if let Some(lines) = entry["lines"].as_array() {
                    for line in lines {
                        debit.add(
                            &Money::parse(&line["debit"].to_string())
                                .expect("native report amount"),
                        );
                        credit.add(
                            &Money::parse(&line["credit"].to_string())
                                .expect("native report amount"),
                        );
                        result += &csv_row(&[
                            text(&entry["date"]),
                            text(&entry["entryNo"]),
                            text(&line["accountNo"]),
                            text(&line["accountName"]),
                            line["text"]
                                .as_str()
                                .unwrap_or(entry["text"].as_str().unwrap_or(""))
                                .into(),
                            if line["debit"].as_f64().unwrap_or(0.0) > 0.0 {
                                amount(&line["debit"])
                            } else {
                                String::new()
                            },
                            if line["credit"].as_f64().unwrap_or(0.0) > 0.0 {
                                amount(&line["credit"])
                            } else {
                                String::new()
                            },
                        ]);
                    }
                }
            }
            result += &csv_row(&[
                "".into(),
                "I alt".into(),
                "".into(),
                "".into(),
                "".into(),
                debit.decimal().replace('.', ","),
                credit.decimal().replace('.', ","),
            ]);
        }
        _ => {
            let year = text(&value["selectedYear"]);
            let prior = year[..4].parse::<i32>().unwrap_or(0) - 1;
            let prior_label = value["priorYearLabel"]
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| prior.to_string());
            let prior_date = value["priorAsOfDate"]
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| format!("{prior}-12-31"));
            let headers = if report == "balance" {
                vec![
                    "Konto".into(),
                    "Navn".into(),
                    format!("Pr. {}", text(&value["asOfDate"])),
                    format!("Pr. {prior_date}"),
                ]
            } else {
                vec![
                    "Konto".into(),
                    "Navn".into(),
                    format!("Beløb {year}"),
                    format!("Beløb {prior_label}"),
                ]
            };
            result += &csv_row(&headers);
            let sections = if report == "balance" {
                vec![
                    ("assets", "Aktiver", "Aktiver i alt"),
                    ("liabilities", "Passiver", "Gæld i alt"),
                    ("equity", "Egenkapital", "Egenkapital i alt"),
                ]
            } else {
                vec![
                    ("income", "Indtægter", "Indtægter i alt"),
                    ("expense", "Udgifter", "Udgifter i alt"),
                ]
            };
            for (key, title, total) in sections {
                result += &csv_row(&["".into(), title.into(), "".into(), "".into()]);
                let items = if report == "balance" {
                    &value[key]["lines"]
                } else {
                    &value[key]
                };
                if let Some(items) = items.as_array() {
                    for line in items {
                        result += &csv_row(&[
                            line["accountNo"]
                                .as_str()
                                .filter(|s| *s != "—")
                                .unwrap_or("")
                                .into(),
                            text(&line["name"]),
                            amount(&line["amount"]),
                            amount(&line["priorAmount"]),
                        ]);
                    }
                }
                let (sum, previous) = if report == "balance" {
                    (&value[key]["total"], &value[key]["priorTotal"])
                } else if key == "income" {
                    (&value["totalIncome"], &value["priorTotalIncome"])
                } else {
                    (&value["totalExpense"], &value["priorTotalExpense"])
                };
                result += &csv_row(&["".into(), total.into(), amount(sum), amount(previous)]);
            }
            let (label, total, previous) = if report == "balance" {
                (
                    "Passiver og egenkapital i alt",
                    &value["totalLiabilitiesAndEquity"],
                    &value["priorTotalLiabilitiesAndEquity"],
                )
            } else {
                ("Årets resultat", &value["result"], &value["priorResult"])
            };
            result += &csv_row(&["".into(), label.into(), amount(total), amount(previous)]);
        }
    }
    result.into_bytes()
}
fn pdf_amount(value: &str) -> String {
    let Some((whole, cents)) = value.split_once(',') else {
        return value.into();
    };
    let mut result = String::new();
    for (index, char) in whole.chars().enumerate() {
        if index > 0
            && (whole.len() - index).is_multiple_of(3)
            && whole.as_bytes()[index - 1] != b'-'
        {
            result.push('.');
        }
        result.push(char);
    }
    format!("{result},{cents}")
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
fn pdf(title: &str, value: &Value, today: &str, rows: &[Row]) -> Vec<u8> {
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
                content += &draw(
                    445,
                    y,
                    &pdf_amount(&row.amount),
                    size,
                    row.section || row.total,
                );
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
    objects.push(format!("<< /Producer (Rentemester deterministic statement renderer) /Title ({}) /CreationDate (D:{date}000000Z) /ModDate (D:{date}000000Z) >>",pdf_text(title)));
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
pub fn build(report: &str, format: &str, value: &Value, today: &str) -> Option<Export> {
    let (title, prefix) = match report {
        "income-statement" => ("Resultatopgørelse", "resultatopgorelse"),
        "balance" => ("Balance", "balance"),
        "trial-balance" => ("Saldobalance", "saldobalance"),
        "journal" => ("Posteringer", "posteringer"),
        _ => return None,
    };
    let (bytes, content_type) = match format {
        "csv" => (csv(report, title, value, today), "text/csv; charset=utf-8"),
        "pdf" if report != "journal" => (
            pdf(title, value, today, &rows(report, value)),
            "application/pdf",
        ),
        _ => return None,
    };
    Some(Export {
        bytes,
        content_type,
        filename: format!(
            "{prefix}-{}-{}.{format}",
            safe(&text(&value["slug"])),
            safe(&text(&value["selectedYear"]))
        ),
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pdf_keeps_last_rows_and_byte_offsets_across_pages() {
        let value = serde_json::json!({"company":{"name":"Synthetic ÆØÅ","currency":"DKK"},"selectedYear":"2026"});
        let rows = (0..100)
            .map(|n| Row {
                section: false,
                total: false,
                label: format!("account-{n}"),
                amount: "1,00".into(),
            })
            .collect::<Vec<_>>();
        let output = pdf("Saldobalance", &value, "2026-01-01", &rows);
        let text = String::from_utf8_lossy(&output);
        assert!(text.contains("/Count 3"));
        assert!(text.contains("account-99"));
        assert!(text.contains("Synthetic \\306\\330\\305"));
        let offset: usize = text
            .split("startxref\n")
            .nth(1)
            .unwrap()
            .lines()
            .next()
            .unwrap()
            .parse()
            .unwrap();
        assert_eq!(&output[offset..offset + 4], b"xref");
        let table = text.split("xref\n").nth(1).unwrap();
        for (number, line) in table.lines().skip(2).take(11).enumerate() {
            let offset: usize = line[..10].parse().unwrap();
            assert!(output[offset..].starts_with(format!("{} 0 obj", number + 1).as_bytes()));
        }
        assert_eq!(output, pdf("Saldobalance", &value, "2026-01-01", &rows));
    }
}
