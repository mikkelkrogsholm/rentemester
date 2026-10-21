//! The original invented synthetic CSV format. Parsing confers no posting authority.
//! Integer source amounts are øre; business admissibility belongs to the opening engine.
use crate::{Error, fiscal_year::Date, identifiers::AccountNo, money::Money, opening_balance};
use serde::Serialize;
use std::collections::BTreeMap;

pub const SYSTEM: &str = "synthetic-csv";

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnclassifiedAccount {
    pub account_no: AccountNo,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpeningLine {
    pub account_no: AccountNo,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub debit_amount: Option<Money>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credit_amount: Option<Money>,
}
impl OpeningLine {
    pub fn opening_line(&self) -> opening_balance::Line {
        opening_balance::Line {
            account_no: self.account_no.clone(),
            debit_amount: self.debit_amount.clone().unwrap_or_default(),
            credit_amount: self.credit_amount.clone().unwrap_or_default(),
            text: None,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    pub source_system: String,
    pub cut_over_date: Date,
    pub chart_of_accounts: Vec<UnclassifiedAccount>,
    pub opening_balances: Vec<OpeningLine>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

fn refused(message: impl Into<String>) -> Error {
    Error::new("synthetic_import_refused", message)
}

fn trim(value: &str) -> &str {
    // ECMAScript trim includes the UTF-8 BOM, including on header lines.
    value.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}')
}

fn ore(cell: &str, side: &str, line: usize) -> Result<Option<Money>, Error> {
    if cell.is_empty() {
        return Ok(None);
    }
    let digits = cell.strip_prefix('-').unwrap_or(cell);
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return Err(refused(format!(
            "line {line}: {side} '{cell}' is not an integer øre amount"
        )));
    }
    // Construct the exact minor-unit value, without floating point or DKK scaling.
    let minor_units = cell
        .parse()
        .map_err(|_| refused(format!("line {line}: {side} amount could not be parsed")))?;
    Ok(Some(Money(minor_units)))
}

pub fn parse(bytes: &[u8]) -> Result<Source, Error> {
    let raw = std::str::from_utf8(bytes).map_err(|_| refused("import file must be UTF-8"))?;
    if trim(raw).is_empty() {
        return Err(refused("import file is empty"));
    }
    let mut errors = Vec::new();
    let mut headers = BTreeMap::new();
    let mut chart_of_accounts = Vec::new();
    let mut opening_balances = Vec::new();
    let mut saw_column_header = false;
    for (index, raw_line) in raw.split('\n').enumerate() {
        let line_number = index + 1;
        let line = trim(raw_line);
        if line.is_empty() {
            continue;
        }
        if let Some(body) = line.strip_prefix('#') {
            let body = trim(body);
            if let Some((key, value)) = body.split_once(':') {
                if !key.is_empty() {
                    headers.insert(trim(key).to_lowercase(), trim(value).to_owned());
                }
            }
            continue;
        }
        // This is deliberately not a general CSV reader: no quoting or escaping.
        let cells: Vec<_> = line.split(',').map(trim).collect();
        let cell = |index: usize| cells.get(index).copied().unwrap_or("");
        match cell(0).to_lowercase().as_str() {
            // Preserve original recognition: remaining column labels are not validated.
            "section" => saw_column_header = true,
            "account" | "opening" => {
                let section = cell(0).to_lowercase();
                if cell(1).is_empty() {
                    errors.push(format!(
                        "line {line_number}: {section} row is missing an accountNo"
                    ));
                    continue;
                }
                let account_no = AccountNo::parse(cell(1))?;
                if section == "account" {
                    chart_of_accounts.push(UnclassifiedAccount {
                        account_no,
                        name: cell(2).to_owned(),
                    });
                    continue;
                }
                let debit_amount = match ore(cell(3), "debit", line_number) {
                    Ok(value) => value,
                    Err(error) => {
                        errors.push(error.message);
                        continue;
                    }
                };
                let credit_amount = match ore(cell(4), "credit", line_number) {
                    Ok(value) => value,
                    Err(error) => {
                        errors.push(error.message);
                        continue;
                    }
                };
                if debit_amount.is_none() && credit_amount.is_none() {
                    errors.push(format!(
                        "line {line_number}: opening row for account '{}' has no amount",
                        cell(1)
                    ));
                    continue;
                }
                opening_balances.push(OpeningLine {
                    account_no,
                    debit_amount,
                    credit_amount,
                });
            }
            _ => errors.push(format!("line {line_number}: unknown section '{}'", cell(0))),
        }
    }
    if !saw_column_header {
        errors.push(
            "import file is missing the 'section,accountNo,name,debit,credit' header row"
                .to_owned(),
        );
    }
    let date_text = headers.get("cutoverdate").map(String::as_str).unwrap_or("");
    let cut_over_date = if date_text.is_empty() {
        errors.push(
            "import file is missing the '# cutOverDate: YYYY-MM-DD' cut-over date header"
                .to_owned(),
        );
        None
    } else {
        match Date::parse(date_text) {
            Ok(date) => Some(date),
            Err(_) => {
                errors.push("cut-over date must be canonical Gregorian YYYY-MM-DD".to_owned());
                None
            }
        }
    };
    if !errors.is_empty() {
        return Err(refused(errors.join("; ")));
    }
    Ok(Source {
        source_system: SYSTEM.to_owned(),
        cut_over_date: cut_over_date.expect("validated cut-over date"),
        chart_of_accounts,
        opening_balances,
        note: headers.remove("note").filter(|note| !note.is_empty()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn original_sample_preserves_unclassified_chart_and_exact_ore() {
        let source = parse(include_bytes!("../../../examples/import-synthetic.csv")).unwrap();
        assert_eq!(source.source_system, SYSTEM);
        assert_eq!(source.cut_over_date.to_string(), "2026-01-01");
        assert_eq!(source.chart_of_accounts.len(), 4);
        assert_eq!(source.chart_of_accounts[0].account_no.as_str(), "2000");
        assert_eq!(source.chart_of_accounts[0].name, "Bank");
        let chart = serde_json::to_value(&source.chart_of_accounts[0]).unwrap();
        assert_eq!(chart.as_object().unwrap().len(), 2);
        assert_eq!(
            source.opening_balances[0]
                .debit_amount
                .as_ref()
                .unwrap()
                .decimal(),
            "1500.00"
        );
        assert!(source.opening_balances[0].credit_amount.is_none());
        let mut debit = Money::default();
        let mut credit = Money::default();
        for line in &source.opening_balances {
            let line = line.opening_line();
            debit.add(&line.debit_amount);
            credit.add(&line.credit_amount);
        }
        assert_eq!(debit, credit);
        assert!(source.note.is_some());
    }

    #[test]
    fn bom_whitespace_case_and_last_directive_match_source_contract() {
        let source = parse("\u{feff} # SOURCE: ignored\r\n# cutOverDate: 2025-01-01\r\n# CUTOVERDATE: 2026-07-01\r\n# NOTE: first\r\n# note: final: note\r\n# ignored comment\r\n SeCtIoN,ignored\r\n ACCOUNT, A-1 , ,ignored\r\n OPENING, A-1 ,,000001,-0\r\n".as_bytes()).unwrap();
        assert_eq!(source.source_system, SYSTEM);
        assert_eq!(source.cut_over_date.to_string(), "2026-07-01");
        assert_eq!(source.note.as_deref(), Some("final: note"));
        assert_eq!(source.chart_of_accounts[0].name, "");
        let line = &source.opening_balances[0];
        assert_eq!(line.debit_amount.as_ref().unwrap().decimal(), "0.01");
        assert_eq!(line.credit_amount.as_ref().unwrap().decimal(), "0.00");
    }

    #[test]
    fn exact_large_signed_amounts_and_business_checks_remain_separate() {
        let source =
            parse(b"# cutOverDate: 2026-01-01\nsection\nopening,X,,-9007199254740993,1\n").unwrap();
        let line = &source.opening_balances[0];
        assert_eq!(
            line.debit_amount.as_ref().unwrap().decimal(),
            "-90071992547409.93"
        );
        assert_eq!(line.credit_amount.as_ref().unwrap().decimal(), "0.01");
        // Negative, two-sided, unbalanced and unknown-account business checks are not parsing.
        assert!(source.chart_of_accounts.is_empty());
    }

    #[test]
    fn rejects_invalid_source_without_decimal_or_quoted_extensions() {
        for (input, message) in [
            ("", "empty"),
            ("section", "cut-over"),
            ("# cutOverDate: 2026-01-01", "header row"),
            ("# cutOverDate: 2026-02-30\nsection", "Gregorian"),
            ("# cutOverDate: 2026-1-1\nsection", "Gregorian"),
            (
                "# cutOverDate: 2026-01-01\nsection\naccount,,Name",
                "accountNo",
            ),
            (
                "# cutOverDate: 2026-01-01\nsection\nopening,,Name,1",
                "accountNo",
            ),
            (
                "# cutOverDate: 2026-01-01\nsection\nopening,X,,,",
                "no amount",
            ),
            (
                "# cutOverDate: 2026-01-01\nsection\nunknown,X",
                "unknown section",
            ),
        ] {
            assert!(
                parse(input.as_bytes())
                    .unwrap_err()
                    .message
                    .contains(message),
                "{input}"
            );
        }
        for amount in ["1.00", "+1", "1e2", "-", "\"1\"", "NaN", "１"] {
            for row in [
                format!("opening,X,,{amount},"),
                format!("opening,X,,,{amount}"),
            ] {
                let input = format!("# cutOverDate: 2026-01-01\nsection\n{row}");
                assert!(
                    parse(input.as_bytes())
                        .unwrap_err()
                        .message
                        .contains("integer øre")
                );
            }
        }
        assert!(parse(&[0xff]).unwrap_err().message.contains("UTF-8"));
    }
}
