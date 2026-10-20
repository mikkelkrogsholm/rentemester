//! The source registry has one declared profile; generic imports still refuse ambiguous dates.
use super::{Error, calendar, fail};
use serde_json::{Value, json};
use unicode_normalization::UnicodeNormalization;
const COLUMNS: [(&str, &str); 12] = [
    ("dato", "transaction_date"),
    ("rentedato", "booking_date"),
    ("tekst", "text"),
    ("beløb", "amount"),
    ("valuta", "currency"),
    ("saldo", "balance_after"),
    ("afsender", "counterparty_name"),
    ("oprindelig afsender", "counterparty_name"),
    ("modtagerkonto", "counterparty_account"),
    ("besked", "message"),
    ("arkivreference", "archive_reference"),
    ("kundereference", "customer_reference"),
];
pub fn profiles() -> Value {
    let columns = COLUMNS
        .into_iter()
        .map(|(k, v)| (k.to_owned(), Value::String(v.into())))
        .collect::<serde_json::Map<_, _>>();
    json!([{"name":"danske-bank","label":"Danske Bank account export","delimiter":";","encoding":"utf8","dateOrder":"dmy","columns":columns}])
}
pub(super) fn field(header: &str) -> Option<&'static str> {
    let normalized = header
        .trim()
        .to_lowercase()
        .nfd()
        .filter(|c| !('\u{0300}'..='\u{036f}').contains(c))
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("_")
        .replace('_', " ");
    COLUMNS
        .iter()
        .find_map(|(key, value)| (*key == normalized).then_some(*value))
}
pub(super) fn date(value: &str) -> Result<String, Error> {
    if calendar::valid_iso_date(value) {
        return Ok(value.into());
    }
    let parts = value.split(['-', '/', '.']).collect::<Vec<_>>();
    if parts.len() == 3
        && (1..=2).contains(&parts[0].len())
        && (1..=2).contains(&parts[1].len())
        && parts[2].len() == 4
        && parts.iter().all(|p| p.bytes().all(|c| c.is_ascii_digit()))
    {
        let iso = format!("{}-{:0>2}-{:0>2}", parts[2], parts[1], parts[0]);
        if calendar::valid_iso_date(&iso) {
            return Ok(iso);
        }
    }
    Err(fail(
        "danske-bank date must be a valid declared DMY or ISO date",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bank::{parse, parse_with_profile};
    #[test]
    fn declared_dmy_bom_quoted_raw_sender_fallback_and_reference_fields() {
        let csv = "\u{feff}Dato;Rentedato;Te\u{0301}kst;Beløb;Valuta;Saldo;Afsender;Oprindelig_afsender;Modtagerkonto;Besked;Arkivreference;Kundereference;Ukendt\r\n3.4.2026;04.04.2026;\"Synthetic; cited\r\nsecond line\";-20,00;DKK;1.130,00;;Synthetic Sender;1234 5678;\"message \"\"quoted\"\"\";AR-1;CR-1;retained\r\n";
        let rows = parse_with_profile(csv, Some(" DANSKE-BANK ")).unwrap();
        let row = &rows[0];
        assert_eq!(row.date, "2026-04-03");
        assert_eq!(row.booking_date.as_deref(), Some("2026-04-04"));
        assert_eq!(row.text, "Synthetic; cited\nsecond line");
        assert_eq!(row.amount.decimal(), "-20.00");
        assert_eq!(row.balance_after.as_ref().unwrap().decimal(), "1130.00");
        assert_eq!(row.counterparty_name.as_deref(), Some("Synthetic Sender"));
        assert_eq!(row.counterparty_account.as_deref(), Some("1234 5678"));
        assert_eq!(row.message.as_deref(), Some("message \"quoted\""));
        assert_eq!(row.archive_reference.as_deref(), Some("AR-1"));
        assert_eq!(row.customer_reference.as_deref(), Some("CR-1"));
        let raw = row.raw.as_ref().unwrap();
        assert_eq!(raw["Ukendt"], "retained");
        assert_eq!(raw["Afsender"], "");
        assert!(raw.contains_key("Te\u{0301}kst"));
        let first = csv.replace(";;Synthetic Sender;", ";First Sender;Synthetic Sender;");
        assert_eq!(
            parse_with_profile(&first, Some("danske-bank")).unwrap()[0]
                .counterparty_name
                .as_deref(),
            Some("First Sender")
        );
    }
    #[test]
    fn named_dates_never_change_generic_ambiguity_or_invent_fx_columns() {
        let ambiguous = "Dato;Tekst;Beløb\n03.04.2026;Synthetic;100\n";
        assert!(parse(ambiguous).is_err());
        assert_eq!(
            parse_with_profile(ambiguous, Some("danske-bank")).unwrap()[0].date,
            "2026-04-03"
        );
        for profile in [None, Some(""), Some("  ")] {
            let row =
                parse_with_profile("date;text;amount\n2026-04-03;Generic;100\n", profile).unwrap();
            assert!(row[0].raw.is_none());
            assert!(row[0].profile.is_none());
        }
        for csv in [
            "Dato;Tekst;Beløb\n31.04.2026;Invalid date;1\n",
            "Dato;Tekst;Beløb\n29.02.2026;Invalid leap date;1\n",
            "Dato;Tekst;Beløb;Valuta;amount_dkk;fx_rate_to_dkk\n03.04.2026;Unvalued FX;100;EUR;745;7.45\n",
            "date,text,amount\n2026-04-03,Wrong profile columns,100\n",
        ] {
            assert!(parse_with_profile(csv, Some("danske-bank")).is_err());
        }
        assert!(parse_with_profile(ambiguous, Some("unknown")).is_err());
        let list = profiles();
        assert_eq!(list.as_array().unwrap().len(), 1);
        assert_eq!(list[0]["name"], "danske-bank");
        assert_eq!(list[0]["delimiter"], ";");
        assert_eq!(list[0]["encoding"], "utf8");
        assert_eq!(list[0]["dateOrder"], "dmy");
        assert_eq!(list[0]["columns"].as_object().unwrap().len(), 12);
    }
}
