//! Source voucher metadata validation. File evidence and mutation authority live outside the engine.
use crate::{Error, calendar, money::Money};
use serde::{Deserialize, Serialize};
use serde_json::Number;
#[derive(Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Party {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vat_or_cvr: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub country_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identifier_kind: Option<String>,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct VatLine {
    pub classification: String,
    pub net_amount: Number,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vat_amount: Option<Number>,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Metadata {
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub issue_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invoice_no: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delivery_description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount_inc_vat: Option<Number>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub currency: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sender: Option<Party>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recipient: Option<Party>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vat_amount: Option<Number>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub purchase_vat_lines: Option<Vec<VatLine>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reverse_charge_wording_confirmed: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub danish_simplified_purchase_invoice: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payment_details: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exemption_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_bank_transaction_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accounting_rationale: Option<String>,
}
pub struct Supplier {
    pub country: String,
    pub kind: String,
    pub identifier: Option<String>,
}
fn error(message: &str) -> Error {
    Error::new("invalid_document_metadata", message)
}
pub fn present(value: Option<&String>) -> bool {
    value.is_some_and(|s| !s.trim().is_empty())
}
pub fn money(value: &Number) -> Result<Money, Error> {
    Money::parse(&value.to_string())
}
const EU: &[&str] = &[
    "AT", "BE", "BG", "HR", "CY", "CZ", "DE", "DK", "EE", "EL", "ES", "FI", "FR", "GR", "HU", "IE",
    "IT", "LT", "LU", "LV", "MT", "NL", "PL", "PT", "RO", "SE", "SI", "SK",
];
pub fn supplier(party: &Party) -> Result<Supplier, Error> {
    let identifier = party
        .vat_or_cvr
        .as_deref()
        .unwrap_or("")
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .to_uppercase();
    let explicit = party.country_code.is_some() || party.identifier_kind.is_some();
    let country = party
        .country_code
        .as_deref()
        .map(|s| s.trim().to_uppercase())
        .unwrap_or_else(|| identifier.chars().take(2).collect());
    let kind = if explicit {
        party.identifier_kind.clone().unwrap_or_default()
    } else if identifier.starts_with("DK") {
        "dk_cvr".into()
    } else {
        "eu_vat".into()
    };
    if country.len() != 2 || !country.bytes().all(|b| b.is_ascii_uppercase()) {
        return Err(error(
            "supplier country and typed identifier require human resolution",
        ));
    }
    let normalized = match kind.as_str() {
        "dk_cvr" => {
            let digits = identifier.strip_prefix("DK").unwrap_or(&identifier);
            if country != "DK" || digits.len() != 8 || !digits.bytes().all(|b| b.is_ascii_digit()) {
                return Err(error(
                    "Danish supplier identity requires country DK and eight CVR digits",
                ));
            }
            Some(format!("DK{digits}"))
        }
        "eu_vat" => {
            if country == "DK"
                || !EU.contains(&country.as_str())
                || !identifier.starts_with(&country)
                || !(4..=16).contains(&identifier.len())
                || !identifier.bytes().all(|b| b.is_ascii_alphanumeric())
            {
                return Err(error(
                    "EU supplier identity requires matching country and VAT identifier",
                ));
            }
            Some(identifier)
        }
        "non_eu" => {
            if EU.contains(&country.as_str()) {
                return Err(error("non-EU supplier identity requires a non-EU country"));
            }
            (!identifier.is_empty()).then_some(identifier)
        }
        _ => return Err(error("supplier identity requires human resolution")),
    };
    Ok(Supplier {
        country,
        kind,
        identifier: normalized,
    })
}
impl Metadata {
    pub fn kind(&self) -> &str {
        self.document_type.as_deref().unwrap_or("purchase_sale")
    }
    pub fn currency(&self) -> String {
        self.currency
            .as_deref()
            .unwrap_or("DKK")
            .trim()
            .to_uppercase()
    }
    pub fn validate(&self) -> Result<Option<Supplier>, Error> {
        if self.source.trim().is_empty() || self.source.len() > 1024 {
            return Err(error("source is required"));
        }
        crate::currency::Currency::parse(&self.currency())?;
        if !["purchase_sale", "cash_register_receipt", "internal_voucher"].contains(&self.kind()) {
            return Err(error("unsupported uploaded document type"));
        }
        if self
            .issue_date
            .as_ref()
            .is_some_and(|s| !calendar::valid_iso_date(s))
        {
            return Err(error("issueDate must use YYYY-MM-DD"));
        }
        let gross = self.amount_inc_vat.as_ref().map(money).transpose()?;
        let vat = self.vat_amount.as_ref().map(money).transpose()?;
        if gross.as_ref().is_some_and(Money::is_negative)
            || vat.as_ref().is_some_and(Money::is_negative)
        {
            return Err(error("document amounts must not be negative"));
        }
        if self
            .exemption_code
            .as_deref()
            .is_some_and(|s| s != "FOREIGN_PHYSICAL_ONLY")
        {
            return Err(error("unsupported document exemption"));
        }
        if let Some(lines) = &self.purchase_vat_lines {
            if lines.is_empty() || lines.len() > 1000 {
                return Err(error("purchaseVatLines requires a bounded nonempty split"));
            }
            let mut total = Money::default();
            let mut total_vat = Money::default();
            for line in lines {
                let net = money(&line.net_amount)?;
                let tax = line
                    .vat_amount
                    .as_ref()
                    .map(money)
                    .transpose()?
                    .unwrap_or_default();
                if net.is_negative() || tax.is_negative() {
                    return Err(error("purchase VAT split must not be negative"));
                }
                match line.classification.as_str() {
                    "exempt" if tax == Money::default() => {}
                    "dk_purchase_25" => {
                        let difference = tax.subtract(&net.percent("25")?);
                        if difference != Money::default() {
                            return Err(error("purchase VAT split must match its 25% base"));
                        }
                    }
                    _ => return Err(error("unsupported purchase VAT split classification")),
                }
                total.add(&net);
                total.add(&tax);
                total_vat.add(&tax);
            }
            if gross.as_ref() != Some(&total) || vat.as_ref() != Some(&total_vat) {
                return Err(error(
                    "purchase VAT split must reconcile with document gross and VAT totals",
                ));
            }
        }
        if self.kind() == "internal_voucher"
            && (self.issue_date.is_none()
                || !present(self.delivery_description.as_ref())
                || gross.as_ref().is_none_or(|v| v == &Money::default())
                || vat.as_ref() != Some(&Money::default())
                || self.source_bank_transaction_id.is_none_or(|id| id <= 0)
                || !present(self.accounting_rationale.as_ref())
                || self.purchase_vat_lines.is_some()
                || self.reverse_charge_wording_confirmed.is_some()
                || self.exemption_code.is_some())
        {
            return Err(error(
                "internal voucher requires date, description, positive gross, zero VAT, bank evidence and accounting rationale",
            ));
        }
        let identity = if self.kind() == "purchase_sale" {
            self.sender.as_ref().and_then(|p| supplier(p).ok())
        } else {
            None
        };
        let simplified = self.danish_simplified_purchase_invoice == Some(true);
        if simplified {
            let gross = gross
                .as_ref()
                .ok_or_else(|| error("simplified invoice requires gross amount"))?;
            let tax = vat
                .as_ref()
                .ok_or_else(|| error("simplified invoice requires VAT"))?;
            if self.kind() != "purchase_sale"
                || self.currency() != "DKK"
                || self.issue_date.is_none()
                || !present(self.invoice_no.as_ref())
                || !present(self.delivery_description.as_ref())
                || identity.as_ref().is_none_or(|i| i.kind != "dk_cvr")
                || self
                    .sender
                    .as_ref()
                    .is_none_or(|p| !present(p.name.as_ref()) || !present(p.address.as_ref()))
                || gross == &Money::default()
                || !gross
                    .subtract(&Money::parse_canonical("3000.00")?)
                    .is_negative()
                    && gross != &Money::parse_canonical("3000.00")?
                || tax == &Money::default()
            {
                return Err(error(
                    "simplified invoice lacks required Danish invoice evidence",
                ));
            }
        }
        if simplified && self.purchase_vat_lines.is_none() {
            let gross = gross.as_ref().unwrap();
            let tax = vat.as_ref().unwrap();
            let net = gross.subtract(tax);
            let difference = tax.subtract(&net.percent("25")?);
            if net.is_negative()
                || net == Money::default()
                || (difference != Money::default()
                    && difference != Money::parse_canonical("0.01")?
                    && difference != Money::parse_canonical("-0.01")?)
            {
                return Err(error(
                    "simplified invoice VAT must match the Danish 25% rate",
                ));
            }
        }
        if self.kind() == "purchase_sale" && self.exemption_code.is_none() {
            if self.issue_date.is_none()
                || !present(self.delivery_description.as_ref())
                || gross.is_none()
                || vat.is_none()
                || identity.is_none()
                || self
                    .sender
                    .as_ref()
                    .is_none_or(|p| !present(p.name.as_ref()) || !present(p.address.as_ref()))
            {
                return Err(error(
                    "purchase invoice requires date, description, amounts and resolved supplier name/address/identity",
                ));
            }
            if !simplified
                && self.recipient.as_ref().is_none_or(|p| {
                    !present(p.name.as_ref())
                        || !present(p.address.as_ref())
                        || !present(p.vat_or_cvr.as_ref())
                })
            {
                return Err(error(
                    "purchase invoice requires recipient name, address and VAT/CVR",
                ));
            }
        }
        Ok(identity)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};
    fn validate(value: Value) -> Result<Option<Supplier>, Error> {
        serde_json::from_value::<Metadata>(value)
            .unwrap()
            .validate()
    }
    fn purchase() -> Value {
        json!({"source":"synthetic","documentType":"purchase_sale","issueDate":"2026-10-18","deliveryDescription":"Synthetic supplies","invoiceNo":"SYN-1","amountIncVat":125,"vatAmount":25,"sender":{"name":"Synthetic supplier","address":"Synthetic street 1","vatOrCvr":"DK12345678"},"recipient":{"name":"Synthetic buyer","address":"Synthetic street 2","vatOrCvr":"DK87654321"}})
    }
    #[test]
    fn ordinary_purchase_requires_identified_parties_and_valid_date() {
        let base = purchase();
        assert_eq!(validate(base.clone()).unwrap().unwrap().kind, "dk_cvr");
        for (field, value) in [
            ("recipient", Value::Null),
            ("issueDate", json!("2026-02-30")),
            ("amountIncVat", json!(-1)),
            (
                "sender",
                json!({"name":"Synthetic","address":"Somewhere","vatOrCvr":"arbitrary"}),
            ),
        ] {
            let mut input = base.clone();
            input[field] = value;
            assert!(validate(input).is_err(), "accepted invalid {field}");
        }
    }
    #[test]
    fn mixed_vat_totals_and_each_rate_must_reconcile() {
        let mut input = purchase();
        input["amountIncVat"] = json!(150);
        input["purchaseVatLines"] = json!([{"classification":"dk_purchase_25","netAmount":100,"vatAmount":25},{"classification":"exempt","netAmount":25,"vatAmount":0}]);
        assert!(validate(input.clone()).is_ok());
        input["purchaseVatLines"][0]["vatAmount"] = json!(24.99);
        assert!(validate(input).is_err());
    }
    #[test]
    fn simplified_invoice_checks_threshold_and_receipt_remains_a_receipt() {
        let mut input = purchase();
        input.as_object_mut().unwrap().remove("recipient");
        input["danishSimplifiedPurchaseInvoice"] = json!(true);
        assert!(validate(input.clone()).is_ok());
        input["amountIncVat"] = json!(3000.01);
        assert!(validate(input).is_err());
        assert!(
            validate(json!({"source":"Synthetic receipt","documentType":"cash_register_receipt"}))
                .is_ok()
        );
        assert!(validate(json!({"source":"Synthetic receipt","documentType":"cash_register_receipt","danishSimplifiedPurchaseInvoice":true})).is_err());
    }
    #[test]
    fn internal_voucher_requires_zero_vat_and_bound_bank_evidence() {
        let mut input = json!({"source":"synthetic","documentType":"internal_voucher","issueDate":"2026-10-18","deliveryDescription":"Synthetic bank fee","amountIncVat":25,"vatAmount":0,"sourceBankTransactionId":1,"accountingRationale":"Synthetic bank statement evidence"});
        assert!(validate(input.clone()).is_ok());
        input["vatAmount"] = json!(5);
        assert!(validate(input).is_err());
    }
}
