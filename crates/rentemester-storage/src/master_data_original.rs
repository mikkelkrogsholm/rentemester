//! Source-only projections over the ordinary contact tables and native writers.
use crate::{idempotency::PrincipalKind, invalid};
use rentemester_engine::{Error, sales_invoice::refused};
use rusqlite::Transaction;
use serde::Deserialize;
use serde_json::{Value, json};
#[derive(Clone, Copy)]
pub enum Kind {
    Customer,
    Vendor,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Create {
    pub input: Value,
    pub from_cvr: Option<String>,
    pub confirm: bool,
}
impl Create {
    pub fn validate(&self, kind: Kind) -> Result<(), Error> {
        if !self.confirm {
            return Err(refused("confirmation required"));
        }
        let mut input = self.input.clone();
        if !input.is_object() {
            return Err(refused("contact input object required"));
        }
        if input.get("name").is_none() {
            input["name"] = json!("");
        }
        match kind {
            Kind::Customer => {
                serde_json::from_value::<crate::sales::Customer>(input)
                    .map_err(|_| refused("invalid customer input"))?;
            }
            Kind::Vendor => {
                serde_json::from_value::<crate::vendors::Vendor>(input)
                    .map_err(|_| refused("invalid vendor input"))?;
            }
        }
        Ok(())
    }
}
pub fn list(tx: &Transaction<'_>, kind: Kind, archived: bool) -> Result<Value, Error> {
    crate::journal_preview::require_current(tx)?;
    let (table, fields): (_, &[(&str, &str)]) = match kind {
        Kind::Customer => (
            "customers",
            &[
                ("id", "id"),
                ("name", "name"),
                ("address", "address"),
                ("vat_or_cvr", "vatOrCvr"),
                ("email", "email"),
                ("phone", "phone"),
                ("website", "website"),
                ("ean_number", "eanNumber"),
                ("payment_terms_days", "paymentTermsDays"),
                ("default_currency", "defaultCurrency"),
                ("notes", "notes"),
                ("archived", "archived"),
                ("created_at", "createdAt"),
            ],
        ),
        Kind::Vendor => (
            "vendors",
            &[
                ("id", "id"),
                ("name", "name"),
                ("address", "address"),
                ("vat_or_cvr", "vatOrCvr"),
                ("country_code", "countryCode"),
                ("identifier_kind", "identifierKind"),
                ("identity_status", "identityStatus"),
                ("email", "email"),
                ("phone", "phone"),
                ("website", "website"),
                ("default_expense_account", "defaultExpenseAccount"),
                ("default_vat_treatment", "defaultVatTreatment"),
                ("notes", "notes"),
                ("archived", "archived"),
                ("created_at", "createdAt"),
            ],
        ),
    };
    let sql = format!(
        "SELECT {} FROM {table} WHERE (?1 OR archived=0) ORDER BY lower(name) ASC,id ASC",
        fields.iter().map(|f| f.0).collect::<Vec<_>>().join(",")
    );
    let mut query = tx.prepare(&sql).map_err(invalid)?;
    let mut result = query.query([archived]).map_err(invalid)?;
    let mut rows = Vec::new();
    while let Some(row) = result.next().map_err(invalid)? {
        let mut value = json!({});
        for (i, (_, key)) in fields.iter().enumerate() {
            value[*key] = match *key {
                "archived" => json!(row.get::<_, bool>(i).map_err(invalid)?),
                "id" | "paymentTermsDays" => json!(row.get::<_, Option<i64>>(i).map_err(invalid)?),
                _ => json!(row.get::<_, Option<String>>(i).map_err(invalid)?),
            };
        }
        crate::gdpr::overlay_contact(tx, table, &mut value)?;
        // Preserve redaction without adding a field to the source DTO.
        value.as_object_mut().unwrap().remove("gdprErased");
        rows.push(value);
    }
    // Privacy overlays can change the visible name. Keep source ASCII lower-name
    // ordering on that projection, without disclosing the hidden original name.
    rows.sort_by(|a, b| {
        a["name"]
            .as_str()
            .unwrap()
            .to_ascii_lowercase()
            .cmp(&b["name"].as_str().unwrap().to_ascii_lowercase())
            .then_with(|| a["id"].as_i64().cmp(&b["id"].as_i64()))
    });
    Ok(json!({"ok":true,"count":rows.len(),"rows":rows,"errors":[]}))
}
/// Only the six source CVR fill fields are composed; explicit nonblank values win.
pub fn compose(input: &Value, company: Option<&Value>) -> Result<Value, Error> {
    let mut input = input.clone();
    let fields = input
        .as_object_mut()
        .ok_or_else(|| refused("contact input object required"))?;
    for value in fields.values_mut() {
        if let Some(s) = value.as_str() {
            let s = s.trim();
            *value = if s.is_empty() { Value::Null } else { json!(s) };
        }
    }
    if let Some(company) = company {
        let city = ["postalCode", "city"]
            .iter()
            .filter_map(|k| company[*k].as_str())
            .filter(|s| !s.trim().is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        let address = [company["address"].as_str().unwrap_or(""), &city]
            .into_iter()
            .filter(|s| !s.trim().is_empty())
            .collect::<Vec<_>>()
            .join(", ");
        for (key, value) in [
            ("name", company["name"].clone()),
            ("address", json!(address)),
            (
                "vatOrCvr",
                json!(format!("DK{}", company["cvr"].as_str().unwrap_or(""))),
            ),
            ("email", company["email"].clone()),
            ("phone", company["phone"].clone()),
            ("website", company["website"].clone()),
        ] {
            if fields.get(key).is_none_or(Value::is_null) {
                fields.insert(key.into(), value);
            }
        }
    }
    if fields.get("name").is_none_or(Value::is_null) {
        fields.insert("name".into(), json!(""));
    }
    Ok(input)
}
pub fn create(
    tx: &Transaction<'_>,
    kind: Kind,
    input: &Value,
    subject: &str,
    principal: PrincipalKind,
) -> Result<Value, Error> {
    let mut input = compose(input, None)?;
    let (key, rule, result) = match kind {
        Kind::Customer => {
            input["defaultCurrency"] = json!(
                input["defaultCurrency"]
                    .as_str()
                    .unwrap_or("DKK")
                    .to_ascii_uppercase()
            );
            let input =
                serde_json::from_value(input).map_err(|_| refused("invalid customer input"))?;
            (
                "customerId",
                "DK-MASTER-DATA-CUSTOMER-001",
                crate::sales::create_customer_with_principal(tx, &input, subject, principal)?,
            )
        }
        Kind::Vendor => {
            let input =
                serde_json::from_value(input).map_err(|_| refused("invalid vendor input"))?;
            (
                "vendorId",
                "DK-MASTER-DATA-VENDOR-001",
                crate::vendors::create_with_principal(tx, &input, subject, principal)?,
            )
        }
    };
    Ok(json!({"ok":true,key:result["id"],"appliedRules":[rule],"errors":[]}))
}
#[cfg(test)]
mod tests;
