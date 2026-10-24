//! Explicit creation settings. Fresh ledger only; no chart, balances or copied company data.
use rentemester_engine::Error;
use rusqlite::{Connection, Transaction, params};
use serde::Deserialize;
use serde_json::{Value, json};
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Input {
    pub name: String,
    pub slug: Option<String>,
    pub cvr: Option<String>,
    pub address: Option<String>,
    pub postal_code: Option<String>,
    pub city: Option<String>,
    pub payment_terms_days: Option<i64>,
    pub fiscal_year_start_month: Option<String>,
    pub fiscal_year_label_strategy: Option<String>,
    #[serde(default = "default_vat")]
    pub vat_period_type: Value,
    pub payment: Option<Payment>,
}
#[derive(Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Payment {
    pub bank_name: Option<String>,
    pub registration_no: Option<String>,
    pub account_no: Option<String>,
    pub iban: Option<String>,
}
fn default_vat() -> Value {
    json!("quarter")
}
pub fn refused() -> Error {
    Error::new(
        "company_creation_refused",
        "company creation settings or fresh state refused",
    )
}
pub struct Settings {
    pub slug: String,
    pub name: String,
    pub month: u32,
    pub strategy: String,
    pub profile: serde_json::Map<String, Value>,
}
pub fn settings(i: &Input) -> Result<Settings, Error> {
    let name = i.name.trim();
    if name.is_empty() || name.len() > 1024 || name.chars().any(char::is_control) {
        return Err(refused());
    }
    let slug = i
        .slug
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| {
            let mut s = String::new();
            for c in name.to_lowercase().chars() {
                let v = match c {
                    'æ' => "ae".into(),
                    'ø' => "oe".into(),
                    'å' => "aa".into(),
                    c if c.is_ascii_alphanumeric() => c.to_string(),
                    _ => "-".into(),
                };
                s.push_str(&v);
            }
            s.split('-')
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join("-")
        });
    if slug.len() > 128
        || slug.is_empty()
        || !slug.as_bytes()[0].is_ascii_alphanumeric()
        || !slug
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
        || slug == "workspace"
    {
        return Err(refused());
    }
    let month = i
        .fiscal_year_start_month
        .as_deref()
        .unwrap_or("1")
        .parse::<u32>()
        .ok()
        .filter(|n| (1..=12).contains(n))
        .ok_or_else(refused)?;
    let strategy = i
        .fiscal_year_label_strategy
        .as_deref()
        .unwrap_or("end-year");
    if !["end-year", "start-year", "span"].contains(&strategy)
        || !(i.vat_period_type.is_null()
            || i.vat_period_type
                .as_str()
                .is_some_and(|s| ["month", "quarter", "half-year"].contains(&s)))
    {
        return Err(refused());
    }
    let cvr = i.cvr.as_deref().unwrap_or("").trim().to_ascii_uppercase();
    let digits = cvr.strip_prefix("DK").unwrap_or(&cvr);
    if !cvr.is_empty() && (digits.len() != 8 || !digits.bytes().all(|c| c.is_ascii_digit())) {
        return Err(refused());
    }
    let mut profile = serde_json::Map::new();
    profile.insert("cvr".into(), json!(cvr));
    profile.insert("vatPeriodType".into(), i.vat_period_type.clone());
    // Omitted/null values retain fresh defaults. Supplied text uses the same
    // trim/empty-to-null semantics as the canonical profile writer below.
    for (key, value) in [
        ("address", &i.address),
        ("postalCode", &i.postal_code),
        ("city", &i.city),
    ] {
        if let Some(value) = value {
            let text = value.trim();
            if text.len() > 1024 || text.chars().any(char::is_control) {
                return Err(refused());
            }
            profile.insert(key.into(), json!(text));
        }
    }
    if let Some(days) = i.payment_terms_days {
        if !(0..=365).contains(&days) {
            return Err(refused());
        }
        profile.insert("paymentTermsDays".into(), json!(days));
    }
    if let Some(p) = &i.payment {
        let mut payment = serde_json::Map::new();
        for (k, v) in [
            ("bankName", &p.bank_name),
            ("registrationNo", &p.registration_no),
            ("accountNo", &p.account_no),
            ("iban", &p.iban),
        ] {
            if let Some(v) = v {
                if v.len() > 1024 || v.chars().any(char::is_control) {
                    return Err(refused());
                }
                payment.insert(k.into(), json!(v.trim()));
            }
        }
        profile.insert("payment".into(), json!(payment));
    }
    Ok(Settings {
        slug,
        name: name.into(),
        month,
        strategy: strategy.into(),
        profile,
    })
}
pub fn initialize(company: &mut Connection, s: &Settings, subject: &str) -> Result<(), Error> {
    crate::fresh::initialize_company(company, &s.name)?;
    let tx = company
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|_| refused())?;
    tx.execute(
        "UPDATE companies SET fiscal_year_start_month=?1,fiscal_year_label_strategy=?2 WHERE id=1",
        params![s.month, s.strategy],
    )
    .map_err(|_| refused())?;
    crate::company_setup::update_profile(&tx, &s.profile, subject)?;
    tx.commit().map_err(|_| refused())
}
pub fn grant(tx: &Transaction<'_>, s: &Settings, subject: &str) -> Result<(), Error> {
    tx.execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor) VALUES(?1,?2,'grant','owner',?3)",params![subject,s.slug,format!("user:{subject}")]).map_err(|_|refused())?;
    tx.execute("INSERT INTO workspace_audit(event_type,entity_type,entity_id,actor) VALUES('company_create','company',?1,?2)",params![s.slug,format!("user:{subject}")]).map_err(|_|refused())?;
    // Canonical membership event is immutable creation/access evidence.
    Ok(())
}
