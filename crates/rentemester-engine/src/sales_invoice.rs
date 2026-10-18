//! Invoice arithmetic and bounded full domestic invoice inputs, shared by delivery surfaces.
use crate::{Error, money::Money};
use serde::Deserialize;
use serde_json::{Number, Value, json};
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Party {
    pub name: Option<String>,
    pub address: Option<String>,
    pub vat_or_cvr: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Line {
    pub description: String,
    pub quantity: Number,
    pub unit_price_ex_vat: Number,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Input {
    pub issue_date: String,
    pub lines: Vec<Line>,
    pub vat_rate_percent: Option<Number>,
    pub customer_id: Option<i64>,
    pub invoice_number: Option<String>,
    pub due_date: Option<String>,
    pub currency: Option<String>,
    pub seller: Option<Party>,
    pub buyer: Option<Party>,
}
pub fn refused(message: impl Into<String>) -> Error {
    Error::new("invoice_refused", message)
}
pub fn wire(money: &Money) -> Result<Value, Error> {
    let n: f64 = money
        .decimal()
        .parse()
        .map_err(|_| refused("invoice amount too large"))?;
    if !n.is_finite() || Money::parse(&n.to_string())? != *money {
        return Err(refused(
            "invoice amount exceeds exact numeric storage range",
        ));
    }
    Ok(json!(n))
}
pub fn text(value: &str, limit: usize) -> Result<String, Error> {
    let s = value.trim();
    if s.is_empty() || s.len() > limit || s.chars().any(|c| c.is_control() || u32::from(c) > 255) {
        return Err(refused(
            "invoice text must be nonempty, bounded and supported by the PDF character set",
        ));
    }
    Ok(s.into())
}
pub fn compute(input: &Input) -> Result<Value, Error> {
    if input.currency.as_deref().is_some_and(|s| s != "DKK")
        || input
            .vat_rate_percent
            .as_ref()
            .is_some_and(|n| n.as_f64() != Some(25.0))
    {
        return Err(refused(
            "native invoice issuance currently supports DKK and standard 25% VAT",
        ));
    }
    crate::fiscal_year::Date::parse(&input.issue_date)?;
    if input.lines.is_empty() || input.lines.len() > 200 {
        return Err(refused("invoice requires 1–200 lines"));
    }
    let mut net = Money::default();
    let mut lines = Vec::new();
    for line in &input.lines {
        let q = line
            .quantity
            .as_f64()
            .ok_or_else(|| refused("invalid quantity"))?;
        let p = line
            .unit_price_ex_vat
            .as_f64()
            .ok_or_else(|| refused("invalid unit price"))?;
        if q <= 0.0 || p < 0.0 {
            return Err(refused("quantity must be positive and price nonnegative"));
        }
        let total = Money::multiply_decimal(
            &line.quantity.to_string(),
            &line.unit_price_ex_vat.to_string(),
        )?;
        net.add(&total);
        lines.push(json!({"description":text(&line.description,1000)?,"quantity":line.quantity,"unitPriceExVat":line.unit_price_ex_vat,"lineTotalExVat":wire(&total)?}));
    }
    if net == Money::default() {
        return Err(refused("invoice total must be positive"));
    }
    let vat = net.percent("25")?;
    let mut gross = net.clone();
    gross.add(&vat);
    Ok(
        json!({"lines":lines,"totals":{"netAmount":wire(&net)?,"vatRate":0.25,"vatAmount":wire(&vat)?,"grossAmount":wire(&gross)?}}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn multiplication_rounds_once_per_line_and_rejects_invalid_inputs() {
        let mut input:Input=serde_json::from_value(json!({"issueDate":"2026-10-18","lines":[{"description":"Synthetic","quantity":3,"unitPriceExVat":0.335}]})).unwrap();
        let result = compute(&input).unwrap();
        assert_eq!(result["totals"]["netAmount"].as_f64(), Some(1.01));
        assert_eq!(result["totals"]["vatAmount"].as_f64(), Some(0.25));
        assert_eq!(result["totals"]["grossAmount"].as_f64(), Some(1.26));
        input.lines[0].quantity = 0.into();
        assert!(compute(&input).is_err());
        input.lines[0].quantity = 1.into();
        input.lines[0].description = "".into();
        assert!(compute(&input).is_err());
        input.lines[0].description = "Synthetic".into();
        input.vat_rate_percent = Some(0.into());
        assert!(compute(&input).is_err());
        assert!(text("Unsupported 📄", 100).is_err());
    }
}
