//! Finite Dinero contacts CSV import. No network, database writes or accounting effects.
mod cvr;
#[cfg(test)]
mod tests;
use crate::{Error, company_knowledge::trim};
pub use cvr::{CvrCompany, Management};
use serde::Deserialize;
use std::collections::BTreeMap;
pub fn refused(message: impl Into<String>) -> Error {
    Error::new("contact_import_invalid", message)
}
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Input {
    pub file_name: String,
    pub content: String,
    #[serde(default)]
    pub enrich_cvr: bool,
    pub confirm: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Customer,
    Vendor,
}
impl Role {
    pub fn table(self) -> &'static str {
        match self {
            Self::Customer => "customers",
            Self::Vendor => "vendors",
        }
    }
}
#[derive(Clone, Debug)]
pub struct Contact {
    pub name: String,
    pub address: Option<String>,
    pub vat_or_cvr: Option<String>,
    pub country_code: Option<String>,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub website: Option<String>,
    pub ean_number: Option<String>,
    pub attention_person: Option<String>,
    pub payment_terms_days: Option<i64>,
    pub total_sales: f64,
    pub total_purchases: f64,
    pub contact_type: Option<String>,
    pub danish_cvr: Option<String>,
}
impl Contact {
    pub fn roles(&self) -> Vec<Role> {
        let mut roles = vec![];
        if self.total_sales != 0.0 {
            roles.push(Role::Customer);
        }
        if self.total_purchases != 0.0 {
            roles.push(Role::Vendor);
        }
        if roles.is_empty() {
            roles.push(Role::Vendor);
        }
        roles
    }
}
#[derive(Clone, Debug)]
pub struct Parsed {
    pub contacts: Vec<Contact>,
    pub errors: Vec<String>,
}
const HEADERS: &[&str] = &[
    "kontaktnavn",
    "adresse",
    "postnummer",
    "by",
    "landekode",
    "cvr-nummer",
    "ean-nummer",
    "telefon",
    "e-mail",
    "att. person",
    "hjemmeside",
    "betalingsfrist i dage",
    "total salg",
    "total køb",
    "kontakttype",
];
fn cell(s: Option<&String>) -> Option<String> {
    s.map(|s| trim(s))
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}
/// One parser serves detection and data; physical line numbers remain useful after multiline fields.
fn csv(raw: &str) -> Result<Vec<(usize, Vec<String>)>, Error> {
    if raw.len() > 12 * 1024 * 1024 {
        return Err(refused("kontakt-CSV overskrider størrelsesgrænsen"));
    }
    let raw = raw.strip_prefix('\u{feff}').unwrap_or(raw);
    let mut chars = raw.chars().peekable();
    let mut records = vec![];
    let mut fields = vec![];
    let mut field = String::new();
    let mut quoted = false;
    let mut closed = false;
    let mut line = 1;
    let mut start = 1;
    while let Some(ch) = chars.next() {
        if quoted {
            if ch == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    field.push('"');
                } else {
                    quoted = false;
                    closed = true;
                }
            } else {
                field.push(ch);
                if ch == '\n' {
                    line += 1;
                }
            }
            continue;
        }
        if closed && ![';', '\r', '\n'].contains(&ch) {
            if ch == ' ' || ch == '\t' {
                continue;
            }
            return Err(refused(format!(
                "linje {line}: ugyldige tegn efter CSV-citat"
            )));
        }
        match ch {
            '"' => {
                if !trim(&field).is_empty() || closed {
                    return Err(refused(format!("linje {line}: ugyldigt CSV-citat")));
                }
                field.clear();
                quoted = true;
            }
            ';' => {
                fields.push(std::mem::take(&mut field));
                closed = false;
            }
            '\r' | '\n' => {
                if ch == '\r' && chars.peek() == Some(&'\n') {
                    chars.next();
                }
                fields.push(std::mem::take(&mut field));
                if fields.iter().any(|v| !trim(v).is_empty()) {
                    records.push((start, std::mem::take(&mut fields)));
                }
                fields.clear();
                closed = false;
                line += 1;
                start = line;
            }
            _ => field.push(ch),
        }
    }
    if quoted {
        return Err(refused(format!("linje {start}: uafsluttet CSV-citat")));
    }
    fields.push(field);
    if fields.iter().any(|v| !trim(v).is_empty()) {
        records.push((start, fields));
    }
    if records.is_empty() {
        return Err(refused("kontakt-CSV'en er tom"));
    }
    if records.len() > 100001 {
        return Err(refused("kontakt-CSV overskrider rækkegrænsen"));
    }
    Ok(records)
}
pub fn parse(raw: &str) -> Result<Parsed, Error> {
    let records = csv(raw)?;
    let mut columns = BTreeMap::new();
    for (i, h) in records[0].1.iter().enumerate() {
        let h = trim(h).to_lowercase();
        if HEADERS.contains(&h.as_str()) && columns.insert(h, i).is_some() {
            return Err(refused("kontakt-CSV har dublerede kendte kolonner"));
        }
    }
    if ["kontaktnavn", "cvr-nummer", "kontakttype"]
        .iter()
        .any(|k| !columns.contains_key(*k))
    {
        return Err(refused(
            "Filen blev ikke genkendt. Understøttet format: Dinero — Kontakter (kunder og leverandører).",
        ));
    }
    let mut contacts = vec![];
    let mut errors = vec![];
    for (line, row) in records.into_iter().skip(1) {
        let get = |key: &str| cell(columns.get(key).and_then(|i| row.get(*i)));
        let Some(name) = get("kontaktnavn") else {
            errors.push(format!("linje {line}: kontakt uden navn — sprunget over"));
            continue;
        };
        let activity = |key: &str| -> Result<f64, Error> {
            let value = get(key)
                .unwrap_or_default()
                .chars()
                .filter(|c| !c.is_whitespace())
                .collect::<String>();
            if value.is_empty() {
                return Ok(0.0);
            }
            value
                .parse::<f64>()
                .ok()
                .filter(|n| n.is_finite())
                .ok_or_else(|| {
                    refused(format!(
                        "linje {line}: ugyldigt {key} — kontakt sprunget over"
                    ))
                })
        };
        let (total_sales, total_purchases) = match (activity("total salg"), activity("total køb"))
        {
            (Ok(a), Ok(b)) => (a, b),
            (Err(e), _) | (_, Err(e)) => {
                errors.push(e.message);
                continue;
            }
        };
        let terms = get("betalingsfrist i dage");
        let terms = if let Some(t) = terms.filter(|s| s.bytes().all(|b| b.is_ascii_digit())) {
            match t
                .parse::<i64>()
                .ok()
                .filter(|n| *n <= 9_007_199_254_740_991)
            {
                Some(v) => Some(v),
                None => {
                    errors.push(format!("linje {line}: betalingsfrist er ikke et sikkert heltal — kontakt sprunget over"));
                    continue;
                }
            }
        } else {
            None
        };
        let country = get("landekode");
        let tax = get("cvr-nummer").map(|s| {
            s.chars()
                .filter(|c| !c.is_whitespace())
                .collect::<String>()
                .to_uppercase()
        });
        let mut danish_cvr = None;
        let tax = tax.map(|s| {
            let digits = s.strip_prefix("DK").unwrap_or(&s);
            if country.as_ref().is_some_and(|c| c.to_uppercase() == "DK")
                && digits.len() == 8
                && digits.bytes().all(|b| b.is_ascii_digit())
            {
                danish_cvr = Some(digits.to_owned());
                format!("DK{digits}")
            } else {
                s
            }
        });
        let street = get("adresse").map(|s| {
            s.trim_end_matches(|c: char| c == ',' || c.is_whitespace())
                .to_owned()
        });
        let cityline = [get("postnummer"), get("by")]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" ");
        let address = [street, (!cityline.is_empty()).then_some(cityline)]
            .into_iter()
            .flatten()
            .filter(|v| !v.is_empty())
            .collect::<Vec<_>>()
            .join(", ");
        contacts.push(Contact {
            name,
            address: (!address.is_empty()).then_some(address),
            vat_or_cvr: tax,
            country_code: country,
            email: get("e-mail"),
            phone: get("telefon"),
            website: get("hjemmeside"),
            ean_number: get("ean-nummer"),
            attention_person: get("att. person"),
            payment_terms_days: terms,
            total_sales,
            total_purchases,
            contact_type: get("kontakttype"),
            danish_cvr,
        });
    }
    Ok(Parsed { contacts, errors })
}
