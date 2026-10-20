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
fn money(value: &Value) -> Option<Money> {
    if let Some(s) = value.as_str() {
        Money::parse_canonical(s).ok()
    } else if value.is_number() {
        Money::parse(&value.to_string()).ok()
    } else {
        None
    }
}
fn amount(value: &Value) -> String {
    money(value)
        .map(|m| m.decimal().replace('.', ","))
        .unwrap_or_default()
}
fn currency(value: &Value) -> String {
    value["declaredCurrency"]
        .as_str()
        .unwrap_or(value["company"]["currency"].as_str().unwrap_or(""))
        .to_owned()
}
fn source(value: &Value) -> String {
    if value["source"] == "archive" {
        format!(
            "Arkiv · Dinero · {} · import {}",
            text(&value["evidence"]["sourceHash"]),
            value["evidence"]["importId"]
        )
    } else {
        "Rentemester · live".into()
    }
}
fn opening_headers(value: &Value) -> Vec<(String, String)> {
    let Some(date) = value["openingBalanceBasis"]["cutOverDate"].as_str() else {
        return vec![];
    };
    let mut headers = vec![(
        "Grundlag".into(),
        format!("Aktiv bogføring fra {date}; arkivbeløb er ikke medregnet"),
    )];
    if value["liveCoverage"]["status"] == "partial" {
        headers.push((
            "Dækning".into(),
            format!("Del af regnskabsåret; historik før {date} er ikke dokumenteret"),
        ));
    }
    headers
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
        "vat" => {
            let state = if value["momsangivelseReady"] == true {
                "Klar til indberetning"
            } else {
                "Foreløbig - kan ikke indberettes"
            };
            for label in [
                format!(
                    "Periode: {} - {}",
                    text(&value["periodStart"]),
                    text(&value["periodEnd"])
                ),
                format!("Frist: {}", text(&value["deadline"])),
                state.into(),
            ] {
                rows.push(Row {
                    section: true,
                    total: false,
                    label,
                    amount: String::new(),
                });
            }
            for (key, label) in [
                ("salgsmoms", "Salgsmoms"),
                ("momsAfVarekobUdland", "Moms af varekøb i udlandet"),
                ("momsAfYdelseskobUdland", "Moms af ydelseskøb i udlandet"),
                ("kobsmoms", "Købsmoms"),
                ("momstilsvar", "Momstilsvar"),
                ("rubrikA", "Rubrik A"),
                ("rubrikB", "Rubrik B"),
                ("rubrikC", "Rubrik C"),
            ] {
                rows.push(Row {
                    section: false,
                    total: key == "momstilsvar",
                    label: label.into(),
                    amount: amount(&value["rubrikker"][key]),
                });
            }
            for key in ["vatReportErrors", "vatReportWarnings"] {
                if let Some(notes) = value[key].as_array() {
                    for note in notes {
                        let chars: Vec<_> = text(note).chars().collect();
                        for chunk in chars.chunks(75) {
                            rows.push(Row {
                                section: false,
                                total: false,
                                label: chunk.iter().collect(),
                                amount: String::new(),
                            });
                        }
                    }
                }
            }
        }
        "journal" => {
            let mut debit = Money::default();
            let mut credit = Money::default();
            if let Some(entries) = value["entries"].as_array() {
                for entry in entries {
                    rows.push(Row {
                        section: true,
                        total: false,
                        label: format!(
                            "{} · {} · {}",
                            text(&entry["date"]),
                            text(&entry["entryNo"]),
                            text(&entry["text"])
                        ),
                        amount: String::new(),
                    });
                    if let Some(lines) = entry["lines"].as_array() {
                        for line in lines {
                            if let Some(m) = money(&line["debit"]) {
                                debit.add(&m);
                            }
                            if let Some(m) = money(&line["credit"]) {
                                credit.add(&m);
                            }
                            for (key, label) in [("debit", "Debet"), ("credit", "Kredit")] {
                                rows.push(Row {
                                    section: false,
                                    total: false,
                                    label: format!(
                                        "{} · {} {} · {label}",
                                        line["date"]
                                            .as_str()
                                            .unwrap_or(entry["date"].as_str().unwrap_or("")),
                                        text(&line["accountNo"]),
                                        text(&line["accountName"])
                                    ),
                                    amount: amount(&line[key]),
                                });
                            }
                        }
                    }
                }
            }
            for (label, total) in [("Debet i alt", debit), ("Kredit i alt", credit)] {
                rows.push(Row {
                    section: false,
                    total: true,
                    label: label.into(),
                    amount: total.decimal().replace('.', ","),
                });
            }
        }
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
        ("Valuta", currency(value)),
        ("Udtrukket", today.into()),
        ("Kilde", source(value)),
    ] {
        if name == "CVR" && item.is_empty() {
            continue;
        }
        result += &csv_row(&[name.into(), item]);
    }
    for (name, value) in opening_headers(value) {
        result += &csv_row(&[name, value]);
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
                        debit.add(&money(&line["debit"]).expect("native report amount"));
                        credit.add(&money(&line["credit"]).expect("native report amount"));
                        result += &csv_row(&[
                            line["date"]
                                .as_str()
                                .unwrap_or(entry["date"].as_str().unwrap_or(""))
                                .to_owned(),
                            text(&entry["entryNo"]),
                            text(&line["accountNo"]),
                            text(&line["accountName"]),
                            line["text"]
                                .as_str()
                                .unwrap_or(entry["text"].as_str().unwrap_or(""))
                                .into(),
                            if money(&line["debit"]).is_some_and(|m| m != Money::default()) {
                                amount(&line["debit"])
                            } else {
                                String::new()
                            },
                            if money(&line["credit"]).is_some_and(|m| m != Money::default()) {
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
            let prior_label = value["priorYearLabel"]
                .as_str()
                .unwrap_or("ikke tilgængelig");
            let prior_date = value["priorAsOfDate"]
                .as_str()
                .unwrap_or("ikke tilgængelig");
            let headers = if report == "balance" {
                vec![
                    "Konto".into(),
                    "Navn".into(),
                    format!(
                        "Pr. {}",
                        value["asOfDate"]
                            .as_str()
                            .unwrap_or("ukendt arkivafslutning")
                    ),
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
            format!("Valuta {}", currency(value)),
            format!("Udtrukket {today}"),
            if value["source"] == "archive" {
                format!(
                    "Kilde: Arkiv · Dinero · import {}",
                    value["evidence"]["importId"]
                )
            } else {
                format!("Kilde: {}", source(value))
            },
        ]);
        if value["source"] == "archive" {
            headers.push(format!(
                "SHA-256: {}",
                text(&value["evidence"]["sourceHash"])
            ));
            headers.push("Afslutningsdato og sammenligningsperiode: ikke dokumenteret".into());
        }
        headers.extend(
            opening_headers(value)
                .into_iter()
                .map(|(name, value)| format!("{name}: {value}")),
        );
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
    if value["source"] == "archive"
        && report != "journal"
        && value["completeness"]["status"] != "ready"
    {
        return None;
    }
    let (title, prefix) = match report {
        "income-statement" => ("Resultatopgørelse", "resultatopgorelse"),
        "balance" => ("Balance", "balance"),
        "trial-balance" => ("Saldobalance", "saldobalance"),
        "journal" => ("Posteringer", "posteringer"),
        "vat" if value["vatRegistered"] == true => ("Moms", "moms"),
        _ => return None,
    };
    let (bytes, content_type) = match format {
        "csv" => (csv(report, title, value, today), "text/csv; charset=utf-8"),
        "pdf" => (
            pdf(title, value, today, &rows(report, value)),
            "application/pdf",
        ),
        _ => return None,
    };
    Some(Export {
        bytes,
        content_type,
        filename: format!(
            "{prefix}-{}-{}-{}.{format}",
            safe(&text(&value["slug"])),
            safe(&text(&value["selectedYear"])),
            if value["source"] == "archive" {
                "archive"
            } else {
                "live"
            }
        ),
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn opening_cutover_partial_year_is_disclosed_in_both_export_formats() {
        let value = serde_json::json!({"slug":"synthetic","source":"live","selectedYear":"2027","company":{"name":"Synthetic","currency":"DKK"},"openingBalanceBasis":{"basis":"live","cutOverDate":"2026-10-01","archiveAmountsIncluded":false},"liveCoverage":{"status":"partial","periodStart":"2026-07-01","periodEnd":"2027-06-30"},"income":[],"expense":[{"accountNo":"4000","name":"Expense","amount":"10.00","priorAmount":null}],"totalIncome":"0.00","totalExpense":"10.00","result":"-10.00"});
        let csv = String::from_utf8(
            build("income-statement", "csv", &value, "2026-10-20")
                .unwrap()
                .bytes,
        )
        .unwrap();
        assert!(csv.contains("Grundlag;\"Aktiv bogføring fra 2026-10-01"));
        assert!(csv.contains("Dækning;\"Del af regnskabsåret;"));
        assert!(csv.contains("historik før 2026-10-01 er ikke dokumenteret"));
        assert!(csv.contains("-10,00"));
        let pdf = build("income-statement", "pdf", &value, "2026-10-20").unwrap();
        let text = String::from_utf8_lossy(&pdf.bytes);
        assert!(text.contains("2026-10-01") && text.contains("ikke dokumenteret"));
        assert!(text.contains("-10,00"));
    }
    #[test]
    fn archive_exports_keep_exact_money_currency_source_and_unknown_comparison() {
        let mut value = serde_json::json!({"slug":"synthetic","source":"archive","selectedYear":"2024","company":{"name":"Synthetic","currency":"EUR"},"declaredCurrency":"DKK","evidence":{"sourceHash":"exact-original","importId":7},"completeness":{"status":"ready"},"income":[{"accountNo":"8000","name":"Income","amount":"9007199254740993.01","priorAmount":null}],"expense":[],"totalIncome":"9007199254740993.01","totalExpense":"0.00","result":"9007199254740993.01","priorYearLabel":null});
        let csv = build("income-statement", "csv", &value, "2026-01-01").unwrap();
        let text = String::from_utf8(csv.bytes).unwrap();
        assert!(text.contains("9007199254740993,01"));
        assert!(text.contains("Valuta;DKK"));
        assert!(text.contains("exact-original"));
        assert!(text.contains("ikke tilgængelig"));
        assert!(!text.contains("2023"));
        assert!(csv.filename.ends_with("-archive.csv"));
        let pdf = build("income-statement", "pdf", &value, "2026-01-01").unwrap();
        assert!(String::from_utf8_lossy(&pdf.bytes).contains("9.007.199.254.740.993,01"));
        value["completeness"]["status"] = serde_json::json!("incomplete");
        assert!(build("income-statement", "csv", &value, "2026-01-01").is_none());
        assert!(build("income-statement", "pdf", &value, "2026-01-01").is_none());
    }
    #[test]
    fn archive_journal_exports_do_not_parse_json_quoted_money_or_invent_native_ids() {
        let value = serde_json::json!({"slug":"synthetic","source":"archive","selectedYear":"2024","company":{"name":"Synthetic"},"declaredCurrency":"DKK","evidence":{"sourceHash":"exact","importId":1},"entries":[{"id":null,"date":"2024-03-01","entryNo":"42","text":"Original","lines":[{"accountNo":"1000","accountName":"Bank","text":"Source","debit":"9007199254740993.01","credit":"0.00"},{"accountNo":"2000","accountName":"Equity","text":"Source","debit":"0.00","credit":"9007199254740993.01"}]}]});
        let csv = build("journal", "csv", &value, "2026-01-01").unwrap();
        assert!(
            String::from_utf8(csv.bytes)
                .unwrap()
                .contains("I alt;;;;9007199254740993,01;9007199254740993,01")
        );
        let pdf = build("journal", "pdf", &value, "2026-01-01").unwrap();
        assert!(pdf.bytes.starts_with(b"%PDF-"));
        assert!(String::from_utf8_lossy(&pdf.bytes).contains("42"));
    }
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
