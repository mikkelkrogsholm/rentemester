//! CSV bank input: exact values, structural validation and explicit date semantics.
use crate::{Error, calendar, currency::Currency, fx::FxRate, money::Money};
use std::collections::{BTreeMap, BTreeSet};
mod bytes;
pub use bytes::decode_bytes;
mod profiles;
pub use profiles::profiles;
/// The source advisory matcher uses Unicode Letter/Number categories and '-'.
pub fn match_token_character(value: char) -> bool {
    use unicode_properties::{GeneralCategory, UnicodeGeneralCategory};
    value == '-'
        || matches!(
            value.general_category(),
            GeneralCategory::UppercaseLetter
                | GeneralCategory::LowercaseLetter
                | GeneralCategory::TitlecaseLetter
                | GeneralCategory::ModifierLetter
                | GeneralCategory::OtherLetter
                | GeneralCategory::DecimalNumber
                | GeneralCategory::LetterNumber
                | GeneralCategory::OtherNumber
        )
}
pub struct Row {
    pub date: String,
    pub booking_date: Option<String>,
    pub text: String,
    pub amount: Money,
    pub currency: String,
    pub reference: Option<String>,
    pub balance_after: Option<Money>,
    pub amount_dkk: Option<Money>,
    pub rate: Option<FxRate>,
    pub profile: Option<String>,
    pub counterparty_name: Option<String>,
    pub counterparty_account: Option<String>,
    pub message: Option<String>,
    pub archive_reference: Option<String>,
    pub customer_reference: Option<String>,
    pub raw: Option<BTreeMap<String, String>>,
}
fn fail(message: impl Into<String>) -> Error {
    Error::new("invalid_bank_csv", message)
}
fn canonical(value: &str) -> String {
    let key = value
        .trim()
        .to_lowercase()
        .replace('å', "a")
        .replace('ø', "o")
        .replace('é', "e")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("_");
    match key.as_str() {
        "bogforingsdato" | "dato" | "date" => "transaction_date",
        "rentedato" | "valordato" | "posteringsdato" => "booking_date",
        "tekst" | "description" | "beskrivelse" => "text",
        "belob" => "amount",
        "valuta" => "currency",
        "ref" | "bilagsnummer" => "reference",
        "belob_dkk" => "amount_dkk",
        "saldo" | "balance" | "running_balance" => "balance_after",
        "kurs" | "valutakurs" => "fx_rate_to_dkk",
        _ => return key,
    }
    .into()
}
fn records(content: &str, separator: char) -> Result<Vec<Vec<String>>, Error> {
    let normalized = content
        .trim_start_matches('\u{feff}')
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    let mut chars = normalized.chars().peekable();
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    while let Some(c) = chars.next() {
        if c == '"' {
            if quoted && chars.peek() == Some(&'"') {
                field.push('"');
                chars.next();
            } else {
                quoted = !quoted;
            }
        } else if !quoted && (c == separator || c == '\n') {
            row.push(field.trim().to_owned());
            field.clear();
            if c == '\n' {
                if row.len() > 1 || !row[0].is_empty() {
                    rows.push(std::mem::take(&mut row));
                } else {
                    row.clear();
                }
            }
        } else {
            field.push(c);
        }
    }
    if quoted {
        return Err(fail("CSV has an unterminated quoted field"));
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field.trim().to_owned());
        if row.len() > 1 || !row[0].is_empty() {
            rows.push(row);
        }
    }
    Ok(rows)
}
/// Bank exports use comma decimals, grouped thousands, parentheses or trailing debit minus.
fn numeric(value: &str) -> Result<String, Error> {
    let mut input = value.trim();
    if let Some((number, currency)) = input.rsplit_once(char::is_whitespace)
        && currency.len() == 3
        && currency.bytes().all(|b| b.is_ascii_alphabetic())
    {
        input = number.trim();
    }
    let mut negative = false;
    if input.starts_with('(') && input.ends_with(')') {
        negative = true;
        input = input[1..input.len() - 1].trim();
    }
    if input.ends_with('-') {
        if negative {
            return Err(fail("conflicting amount signs"));
        }
        negative = true;
        input = input[..input.len() - 1].trim();
    }
    let compact: String = input.chars().filter(|c| !c.is_whitespace()).collect();
    if negative && compact.starts_with(['-', '+']) {
        return Err(fail("conflicting amount signs"));
    }
    let (sign, digits) = if compact.starts_with(['-', '+']) {
        (&compact[..1], &compact[1..])
    } else {
        ("", compact.as_str())
    };
    let normalized = if let Some((integer, fraction)) = digits.split_once(',') {
        if fraction.is_empty() || !fraction.bytes().all(|b| b.is_ascii_digit()) {
            return Err(fail("invalid decimal amount"));
        }
        let groups: Vec<_> = integer.split('.').collect();
        if groups
            .iter()
            .any(|s| s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()))
            || groups.len() > 1 && (groups[0].len() > 3 || groups[1..].iter().any(|s| s.len() != 3))
        {
            return Err(fail("invalid thousands grouping"));
        }
        format!("{}.{}", groups.concat(), fraction)
    } else {
        let groups: Vec<_> = digits.split('.').collect();
        if groups.len() > 1
            && groups
                .iter()
                .all(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
            && groups[1..].iter().all(|s| s.len() == 3)
        {
            groups.concat()
        } else {
            digits.to_owned()
        }
    };
    if normalized.is_empty()
        || !normalized.bytes().all(|b| b.is_ascii_digit() || b == b'.')
        || normalized.matches('.').count() > 1
    {
        return Err(fail("invalid numeric token"));
    }
    Ok(format!("{}{normalized}", if negative { "-" } else { sign }))
}
fn date(value: &str) -> Result<String, Error> {
    if calendar::valid_iso_date(value) {
        return Ok(value.into());
    }
    let parts: Vec<_> = value.split(['-', '/', '.']).collect();
    if parts.len() == 3 && parts[0].len() == 2 && parts[1].len() == 2 && parts[2].len() == 4 {
        let day = parts[0]
            .parse::<u8>()
            .map_err(|_| fail("invalid bank date"))?;
        let month = parts[1]
            .parse::<u8>()
            .map_err(|_| fail("invalid bank date"))?;
        if day <= 12 && month <= 12 && day != month {
            return Err(fail("ambiguous date; use YYYY-MM-DD"));
        }
        let iso = format!("{}-{}-{}", parts[2], parts[1], parts[0]);
        if calendar::valid_iso_date(&iso) {
            return Ok(iso);
        }
    }
    Err(fail("bank date must use YYYY-MM-DD"))
}
pub fn parse(content: &str) -> Result<Vec<Row>, Error> {
    parse_with_profile(content, None)
}
pub(super) fn profile_name(profile: Option<&str>) -> Result<Option<String>, Error> {
    let profile = profile
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(str::to_lowercase);
    if profile.as_deref().is_some_and(|p| p != "danske-bank") {
        return Err(fail("unknown bank import profile (known: danske-bank)"));
    }
    Ok(profile)
}
pub fn parse_with_profile(content: &str, profile: Option<&str>) -> Result<Vec<Row>, Error> {
    let profile = profile_name(profile)?;
    let named = profile.is_some();
    if content.len() > 25 * 1024 * 1024 {
        return Err(fail("bank CSV exceeds 25 MiB"));
    }
    let header = content
        .lines()
        .find(|s| !s.trim().is_empty())
        .ok_or_else(|| fail("empty bank CSV"))?;
    let separator = if named {
        ';'
    } else {
        [',', ';', '\t']
            .into_iter()
            .max_by_key(|sep| {
                let mut quoted = false;
                header
                    .chars()
                    .filter(|c| {
                        if *c == '"' {
                            quoted = !quoted;
                        }
                        !quoted && c == sep
                    })
                    .count()
            })
            .unwrap()
    };
    let mut parsed = records(content, separator)?.into_iter();
    let original_headers = parsed.next().ok_or_else(|| fail("empty bank CSV"))?;
    let headers: Vec<Option<String>> = original_headers
        .iter()
        .map(|h| {
            if named {
                profiles::field(h).map(str::to_owned)
            } else {
                Some(canonical(h))
            }
        })
        .collect();
    let mut seen = BTreeSet::new();
    for header in headers.iter().flatten() {
        if !seen.insert(header.as_str()) && !named {
            return Err(fail(format!("duplicate canonical CSV column {header}")));
        }
    }
    for required in ["transaction_date", "text", "amount"] {
        if !seen.contains(required) {
            return Err(fail(format!("CSV missing required column {required}")));
        }
    }
    let mut rows = Vec::new();
    for values in parsed {
        if values.len() != headers.len() {
            return Err(fail("CSV row has a different field count from header"));
        }
        if rows.len() >= 100_000 {
            return Err(fail("too many bank rows"));
        }
        let mut fields = BTreeMap::new();
        for (header, value) in headers.iter().zip(&values) {
            if let Some(header) = header {
                let target = fields.entry(header.as_str()).or_insert("");
                if !named || target.trim().is_empty() {
                    *target = value.as_str();
                }
            }
        }
        let get = |key| fields.get(key).copied().unwrap_or("");
        let optional = |key| {
            let s = get(key);
            (!s.is_empty()).then(|| s.to_owned())
        };
        let amount = Money::parse(&numeric(get("amount"))?)?;
        let currency = if get("currency").is_empty() {
            "DKK".into()
        } else {
            get("currency").to_uppercase()
        };
        Currency::parse(&currency)?;
        let amount_dkk = optional("amount_dkk")
            .map(|s| numeric(&s).and_then(|v| Money::parse(&v)))
            .transpose()?;
        let rate = optional("fx_rate_to_dkk")
            .map(|s| numeric(&s).and_then(|v| FxRate::parse(&v)))
            .transpose()?;
        if get("text").is_empty() {
            return Err(fail("bank row text is required"));
        }
        if currency == "DKK" && amount_dkk.as_ref().is_some_and(|dkk| dkk != &amount) {
            return Err(fail(
                "DKK bank amount_dkk must equal its signed settlement amount",
            ));
        }
        if currency != "DKK"
            && (amount_dkk.is_none()
                || rate
                    .as_ref()
                    .is_none_or(|r| amount_dkk.as_ref() != Some(&r.apply(&amount))))
        {
            return Err(fail(
                "foreign bank row requires a positive FX rate and amount_dkk equal to amount * rate",
            ));
        }
        rows.push(Row {
            date: if named {
                profiles::date(get("transaction_date"))?
            } else {
                date(get("transaction_date"))?
            },
            booking_date: optional("booking_date")
                .map(|s| if named { profiles::date(&s) } else { date(&s) })
                .transpose()?,
            text: get("text").into(),
            amount,
            currency,
            reference: optional("reference"),
            balance_after: optional("balance_after")
                .map(|s| numeric(&s).and_then(|v| Money::parse(&v)))
                .transpose()?,
            amount_dkk,
            rate,
            profile: profile.clone(),
            counterparty_name: if named {
                optional("counterparty_name")
            } else {
                None
            },
            counterparty_account: if named {
                optional("counterparty_account")
            } else {
                None
            },
            message: if named { optional("message") } else { None },
            archive_reference: if named {
                optional("archive_reference")
            } else {
                None
            },
            customer_reference: if named {
                optional("customer_reference")
            } else {
                None
            },
            raw: named.then(|| {
                original_headers
                    .iter()
                    .cloned()
                    .zip(values.iter().cloned())
                    .collect()
            }),
        });
    }
    if rows.is_empty() {
        return Err(fail("CSV has no bank transactions"));
    }
    Ok(rows)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn advisory_token_categories_match_source_letters_numbers_and_hyphen_only() {
        for value in ['Æ', 'ø', '漢', '²', 'Ⅳ', '٣', '-'] {
            assert!(match_token_character(value));
        }
        for value in ['\u{0345}', '\u{0301}', '_', '/', '—', ' ', '€'] {
            assert!(!match_token_character(value));
        }
    }
    #[test]
    fn optional_localized_statement_balance_is_preserved_not_inferred() {
        let rows =
            parse("dato;tekst;beløb;saldo\n2026-10-18;Synthetic outgoing;250,00-;1.250,50-\n")
                .unwrap();
        assert_eq!(
            rows[0].balance_after.as_ref().unwrap().decimal(),
            "-1250.50"
        );
        assert!(
            parse("date;text;amount\n2026-10-18;Synthetic no balance;1\n").unwrap()[0]
                .balance_after
                .is_none()
        );
        assert!(
            parse("date;text;amount;balance_after\n2026-10-18;Synthetic invalid;1;unknown\n")
                .is_err()
        );
    }
    #[test]
    fn quoted_multiline_localized_amounts_and_dates() {
        let rows=parse("\u{feff}Dato;Tekst;Beløb\r\n18.10.2026;\"Synthetic; supplies\nsecond line\";1.234,56-\r\n").unwrap();
        assert_eq!(rows[0].amount.decimal(), "-1234.56");
        assert!(rows[0].text.contains('\n'));
        for csv in [
            "date;dato;text;amount\n2026-10-18;2026-10-18;x;1",
            "date;text;amount\n05/04/2026;x;1",
            "date;text;amount\n2026-10-18;x;0xff",
            "date;text;amount\n2026-10-18;\"x;1",
            "date;text;amount;amount_dkk\n2026-10-18;Synthetic mismatch;-100;100",
        ] {
            assert!(parse(csv).is_err());
        }
    }
}
