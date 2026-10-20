//! Workspace-scoped creation review. A minimal known receipt plus canonical reads is required for selection.
use serde_json::{Map, Value, json};
pub const FIELDS: &[(&str, &str)] = &[
    ("name", "Virksomhedsnavn"),
    ("slug", "Valgfrit virksomheds-id"),
    ("cvr", "Valgfrit CVR"),
    ("fiscalYearStartMonth", "Regnskabsårets startmåned, 1–12"),
    (
        "fiscalYearLabelStrategy",
        "Årsnavn: end-year, start-year eller span",
    ),
    (
        "vatPeriodType",
        "Moms: month, quarter, half-year eller none",
    ),
    ("bankName", "Valgfrit banknavn"),
    ("registrationNo", "Valgfrit registreringsnummer"),
    ("accountNo", "Valgfrit bankkontonummer"),
    ("iban", "Valgfrit IBAN"),
];
const LIMIT: usize = 16 * 1024;
fn fail() -> String {
    "Virksomhedsoprettelsen kunne ikke verificeres; læs den aktuelle virksomhedsoversigt før yderligere handling".into()
}
fn text(s: &str) -> bool {
    s.len() <= 1024 && !s.chars().any(char::is_control)
}
fn slug(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s != "workspace"
        && s.as_bytes()[0].is_ascii_alphanumeric()
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}
fn catalog(value: &Value) -> bool {
    value["ok"] == true
        && value["companies"].as_array().is_some_and(|rows| {
            let mut seen = std::collections::BTreeSet::new();
            rows.iter().all(|r| {
                r["slug"]
                    .as_str()
                    .is_some_and(|s| (s == "workspace" || slug(s)) && seen.insert(s))
                    && r["name"].as_str().is_some_and(|s| !s.is_empty() && text(s))
            })
        })
}
#[derive(Clone, Debug, PartialEq)]
pub struct Snapshot {
    pub principal: String,
    pub me: Value,
    pub catalog: Value,
}
impl Snapshot {
    pub fn parse(me: Value, catalog_value: Value) -> Result<Self, String> {
        let principal = me["user"]["id"]
            .as_str()
            .filter(|s| !s.is_empty() && !s.chars().any(char::is_control))
            .ok_or_else(fail)?
            .to_string();
        if me["ok"] != true || !catalog(&catalog_value) {
            return Err(fail());
        }
        Ok(Self {
            principal,
            me,
            catalog: catalog_value,
        })
    }
}
/// Human choices only. No copied chart, inferred company selector, actor or replay key.
pub fn body(values: &[(String, String)]) -> Result<Vec<u8>, String> {
    let mut seen = std::collections::BTreeSet::new();
    if values
        .iter()
        .any(|(k, v)| !FIELDS.iter().any(|(field, _)| field == k) || !seen.insert(k) || !text(v))
    {
        return Err("Ukendt, gentaget eller ugyldigt virksomhedsinput".into());
    }
    let get = |key: &str| {
        values
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.trim())
            .unwrap_or("")
    };
    let name = get("name");
    let month = get("fiscalYearStartMonth");
    let strategy = get("fiscalYearLabelStrategy");
    let vat = get("vatPeriodType");
    if name.is_empty()
        || month
            .parse::<u8>()
            .ok()
            .filter(|n| (1..=12).contains(n) && n.to_string() == month)
            .is_none()
        || !["end-year", "start-year", "span"].contains(&strategy)
        || !["none", "month", "quarter", "half-year"].contains(&vat)
    {
        return Err("Angiv navn og vælg eksplicit startmåned, årsnavn og momsperiode".into());
    }
    let mut value = json!({"name":name,"fiscalYearStartMonth":month,"fiscalYearLabelStrategy":strategy,"vatPeriodType":if vat=="none"{Value::Null}else{json!(vat)}});
    if !get("slug").is_empty() {
        if !slug(get("slug")) {
            return Err("Virksomheds-id skal være et gyldigt lille id uden mellemrum".into());
        }
        value["slug"] = json!(get("slug"));
    }
    if !get("cvr").is_empty() {
        let cvr = get("cvr").to_ascii_uppercase();
        let digits = cvr.strip_prefix("DK").unwrap_or(&cvr);
        if digits.len() != 8 || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return Err("CVR skal være otte cifre, eventuelt med DK foran".into());
        }
        value["cvr"] = json!(get("cvr"));
    }
    let mut payment = Map::new();
    for key in ["bankName", "registrationNo", "accountNo", "iban"] {
        if !get(key).is_empty() {
            payment.insert(key.into(), json!(get(key)));
        }
    }
    if !payment.is_empty() {
        value["payment"] = Value::Object(payment);
    }
    let bytes = serde_json::to_vec(&value).map_err(|_| fail())?;
    validate_body(&bytes)?;
    Ok(bytes)
}
pub(crate) fn validate_body(bytes: &[u8]) -> Result<Value, String> {
    if bytes.len() > LIMIT {
        return Err(fail());
    }
    let value: Value = serde_json::from_slice(bytes).map_err(|_| fail())?;
    let object = value.as_object().ok_or_else(fail)?;
    if object.keys().any(|k| {
        ![
            "name",
            "slug",
            "cvr",
            "fiscalYearStartMonth",
            "fiscalYearLabelStrategy",
            "vatPeriodType",
            "payment",
        ]
        .contains(&k.as_str())
    }) || value["name"]
        .as_str()
        .is_none_or(|s| s.trim().is_empty() || !text(s))
    {
        return Err(fail());
    }
    for key in [
        "slug",
        "cvr",
        "fiscalYearStartMonth",
        "fiscalYearLabelStrategy",
    ] {
        if let Some(v) = object.get(key)
            && !v.is_null()
            && !v.as_str().is_some_and(text)
        {
            return Err(fail());
        }
    }
    if let Some(v) = object.get("vatPeriodType")
        && !(v.is_null()
            || v.as_str()
                .is_some_and(|s| ["month", "quarter", "half-year"].contains(&s)))
    {
        return Err(fail());
    }
    if let Some(payment) = object.get("payment")
        && !payment.is_null()
    {
        let p = payment.as_object().ok_or_else(fail)?;
        if p.iter().any(|(k, v)| {
            !["bankName", "registrationNo", "accountNo", "iban"].contains(&k.as_str())
                || !(v.is_null() || v.as_str().is_some_and(text))
        }) {
            return Err(fail());
        }
    }
    Ok(value)
}
/// The receipt is intentionally minimal; it is never a company profile or proof inferred from a name.
pub(crate) fn receipt_slug(bytes: &[u8], receipt: &Value) -> Result<String, String> {
    let input = validate_body(bytes)?;
    let company = &receipt["company"];
    let result = company["slug"]
        .as_str()
        .filter(|s| slug(s))
        .ok_or_else(fail)?;
    if receipt["ok"] != true
        || company["name"] != input["name"].as_str().ok_or_else(fail)?.trim()
        || input["slug"]
            .as_str()
            .is_some_and(|s| !s.trim().is_empty() && s != result)
    {
        return Err(fail());
    }
    Ok(result.into())
}
#[derive(Clone, Debug)]
pub struct Review {
    pub before: Snapshot,
    pub bytes: Vec<u8>,
}
impl Review {
    pub fn new(before: Snapshot, bytes: Vec<u8>) -> Result<Self, String> {
        validate_body(&bytes)?;
        Ok(Self { before, bytes })
    }
    pub fn current(&self, now: &Snapshot) -> bool {
        self.before == *now
    }
    pub fn receipt(&self, value: &Value) -> Result<String, String> {
        receipt_slug(&self.bytes, value)
    }
    pub fn canonical(
        &self,
        receipt: &Value,
        catalog_value: &Value,
        profile: &Value,
    ) -> Result<String, String> {
        let slug = self.receipt(receipt)?;
        let input = validate_body(&self.bytes)?;
        let company = &profile["company"];
        if !catalog(catalog_value)
            || profile["ok"] != true
            || !company["id"].as_i64().is_some_and(|n| n > 0)
            || company["name"] != receipt["company"]["name"]
            || !catalog_value["companies"].as_array().is_some_and(|rows| {
                rows.iter()
                    .any(|r| r["slug"] == slug && r["name"] == company["name"])
            })
        {
            return Err(fail());
        }
        if let Some(month) = input["fiscalYearStartMonth"].as_str()
            && company["fiscalYearStartMonth"].as_u64() != month.parse::<u64>().ok()
        {
            return Err(fail());
        }
        if input["fiscalYearLabelStrategy"].is_string()
            && input["fiscalYearLabelStrategy"] != company["fiscalYearLabelStrategy"]
        {
            return Err(fail());
        }
        if input.get("vatPeriodType").is_some()
            && input["vatPeriodType"] != company["vatPeriodType"]
        {
            return Err(fail());
        }
        if let Some(cvr) = input["cvr"].as_str().filter(|s| !s.trim().is_empty()) {
            let upper = cvr.trim().to_ascii_uppercase();
            let digits = upper.strip_prefix("DK").unwrap_or(&upper);
            if company["cvr"] != format!("DK{digits}") {
                return Err(fail());
            }
        }
        if let Some(payment) = input["payment"].as_object() {
            for (k, v) in payment {
                if let Some(s) = v.as_str().filter(|s| !s.trim().is_empty())
                    && company["payment"][k] != s.trim()
                {
                    return Err(fail());
                }
            }
        }
        Ok(slug)
    }
}
#[cfg(test)]
pub(crate) mod tests;
