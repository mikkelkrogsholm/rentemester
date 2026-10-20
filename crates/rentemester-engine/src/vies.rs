//! EU VAT identity, authoritative observations and bounded cache lifetime. No network or clock.
use crate::{Error, company_knowledge, vat_period};
use serde_json::{Value, json};

pub const TTL_DAYS: i64 = 90;
const COUNTRIES: &[&str] = &[
    "AT", "BE", "BG", "CY", "CZ", "DE", "DK", "EE", "EL", "ES", "FI", "FR", "HR", "HU", "IE", "IT",
    "LT", "LU", "LV", "MT", "NL", "PL", "PT", "RO", "SE", "SI", "SK",
];
pub fn refused(message: impl Into<String>) -> Error {
    Error::new("vies_refused", message)
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Vat {
    pub country_code: String,
    pub vat_number: String,
    pub normalized: String,
}
/// Only conventional spacing and separators are ignored; arbitrary punctuation is never removed.
pub fn normalize(input: &str) -> Result<Vat, Error> {
    if input.len() > 64 || !input.is_ascii() {
        return Err(refused("bounded EU VAT number required"));
    }
    let compact: String = input
        .trim()
        .chars()
        .filter(|c| *c != ' ' && *c != '-' && *c != '.')
        .map(|c| c.to_ascii_uppercase())
        .collect();
    if !(4..=16).contains(&compact.len()) || !compact.bytes().all(|c| c.is_ascii_alphanumeric()) {
        return Err(refused("plausible EU VAT number required"));
    }
    let (country, number) = compact.split_at(2);
    if !COUNTRIES.contains(&country) {
        return Err(refused("EU VIES country code required"));
    }
    Ok(Vat {
        country_code: country.into(),
        vat_number: number.into(),
        normalized: compact,
    })
}
pub fn foreign(input: &str) -> Result<Vat, Error> {
    let vat = normalize(input)?;
    if vat.country_code == "DK" {
        return Err(refused("foreign EU VAT number required"));
    }
    Ok(vat)
}
fn identity(record: &Value) -> Result<Vat, Error> {
    let vat = normalize(
        record["normalized"]
            .as_str()
            .ok_or_else(|| refused("missing normalized VAT identity"))?,
    )?;
    if record["countryCode"].as_str() != Some(&vat.country_code)
        || record["vatNumber"].as_str() != Some(&vat.vat_number)
    {
        return Err(refused("VIES identity fields disagree"));
    }
    Ok(vat)
}
fn timestamp(input: &str) -> Result<String, Error> {
    if !input.contains('T') {
        return Err(refused(
            "VIES timestamp requires explicit time and timezone",
        ));
    }
    company_knowledge::iso(input).map_err(|_| refused("invalid VIES timestamp"))
}
pub fn expiry(now: &str) -> Result<String, Error> {
    let now = timestamp(now)?;
    let date = vat_period::date(&now[..10])?
        .checked_add(time::Duration::days(TTL_DAYS))
        .ok_or_else(|| refused("VIES expiry out of range"))?;
    if !(1..=9999).contains(&date.year()) {
        return Err(refused("VIES expiry out of range"));
    }
    Ok(format!(
        "{:04}-{:02}-{:02}{}",
        date.year(),
        u8::from(date.month()),
        date.day(),
        &now[10..]
    ))
}
/// Turn a trusted provider observation into a cache record using the host's trusted clock.
pub fn record(observation: &Value, now: &str) -> Result<Value, Error> {
    let vat = identity(observation)?;
    let valid = observation["valid"]
        .as_bool()
        .ok_or_else(|| refused("authoritative boolean VIES observation required"))?;
    let text = |key: &str, max: usize| -> Result<Value, Error> {
        match &observation[key] {
            Value::Null => Ok(Value::Null),
            Value::String(s) if s.len() <= max => Ok(json!(s)),
            _ => Err(refused("invalid bounded VIES evidence")),
        }
    };
    Ok(
        json!({"countryCode":vat.country_code,"vatNumber":vat.vat_number,"normalized":vat.normalized,"valid":valid,"name":text("name",8192)?,"address":text("address",32768)?,"validatedAt":timestamp(now)?,"expiresAt":expiry(now)?,"rawResponse":text("rawResponse",262144)?}),
    )
}
/// Validate persisted/frozen record identity and the exact cache lifetime. Invalid answers are valid records.
pub fn validate_record(value: &Value) -> Result<Value, Error> {
    let at = value["validatedAt"]
        .as_str()
        .ok_or_else(|| refused("missing VIES timestamp"))?;
    let normalized = record(value, at)?;
    if timestamp(
        value["expiresAt"]
            .as_str()
            .ok_or_else(|| refused("missing VIES expiry"))?,
    )? != normalized["expiresAt"].as_str().unwrap()
    {
        return Err(refused("VIES cache lifetime must be 90 days"));
    }
    Ok(normalized)
}
/// Foreign EU sale eligibility is determined at issuance, never by a later cache refresh.
pub fn require_record(value: &Value, vat: &str, now: &str) -> Result<Value, Error> {
    let vat = foreign(vat)?;
    let value = validate_record(value)?;
    let now = timestamp(now)?;
    if value["normalized"].as_str() != Some(&vat.normalized)
        || value["valid"].as_bool() != Some(true)
    {
        return Err(refused(
            "valid matching foreign EU VIES validation required",
        ));
    }
    if value["validatedAt"].as_str().unwrap() > now.as_str()
        || value["expiresAt"].as_str().unwrap() < now.as_str()
    {
        return Err(refused(
            "VIES validation is future dated or expired; validate again",
        ));
    }
    Ok(value)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn strict_identity_and_foreign_basis() {
        assert_eq!(
            normalize(" de 123-456.789 ").unwrap().normalized,
            "DE123456789"
        );
        for bad in [
            "GB123456789",
            "NO123456789",
            "DE12/34",
            "DE12\n34",
            "DÉ1234",
            "DE",
            "../DE1234",
        ] {
            assert!(normalize(bad).is_err(), "{bad}");
        }
        assert!(normalize("DK12345678").is_ok());
        assert!(foreign("DK12345678").is_err());
    }
    #[test]
    fn expiry_future_invalid_and_identity_are_fail_closed() {
        let observation = json!({"countryCode":"DE","vatNumber":"123456789","normalized":"DE123456789","valid":true,"name":"Synthetic","address":null,"rawResponse":"{}"});
        let valid = record(&observation, "2026-01-01T12:00:00Z").unwrap();
        assert_eq!(valid["expiresAt"], "2026-04-01T12:00:00.000Z");
        assert!(require_record(&valid, "DE123456789", "2026-04-01T12:00:00Z").is_ok());
        for at in ["2026-04-01T12:00:00.001Z", "2025-12-31T12:00:00Z"] {
            assert!(require_record(&valid, "DE123456789", at).is_err());
        }
        let mut invalid = valid.clone();
        invalid["valid"] = json!(false);
        assert!(validate_record(&invalid).is_ok());
        assert!(require_record(&invalid, "DE123456789", "2026-01-02T12:00:00Z").is_err());
        let mut wrong = valid.clone();
        wrong["vatNumber"] = json!("000000000");
        assert!(validate_record(&wrong).is_err());
        let mut extended = valid;
        extended["expiresAt"] = json!("2026-04-02T12:00:00Z");
        assert!(validate_record(&extended).is_err());
    }
}
