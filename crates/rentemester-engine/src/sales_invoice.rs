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
    pub ean_number: Option<String>,
    pub public_recipient: Option<bool>,
    pub buyer_reference: Option<String>,
    pub order_reference: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Line {
    pub description: String,
    pub quantity: Number,
    pub unit_price_ex_vat: Number,
    pub tax_classification: Option<String>,
    pub vat_rate: Option<Number>,
    pub reverse_charge_basis: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Input {
    pub issue_date: String,
    pub lines: Vec<Line>,
    pub vat_rate_percent: Option<Number>,
    pub vat_treatment: Option<String>,
    pub reverse_charge_basis: Option<String>,
    pub reverse_charge_note: Option<String>,
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
/// Recipient routing is explicit evidence, not inferred from names or VAT IDs.
pub fn recipient_ean(value: Option<&str>) -> Result<Option<String>, Error> {
    let Some(value) = value.filter(|s| !s.trim().is_empty()) else {
        return Ok(None);
    };
    if value.len() > 1024 {
        return Err(refused("buyer.eanNumber exceeds bound"));
    }
    let digits = value
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>();
    if digits.len() != 13 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return Err(refused("buyer.eanNumber must be 13 digits"));
    }
    Ok(Some(digits))
}
pub fn recipient_reference(value: Option<&str>) -> Result<Option<String>, Error> {
    value
        .map(|value| {
            let value = value.trim();
            if value.is_empty() || value.len() > 1024 || value.chars().any(char::is_control) {
                return Err(refused(
                    "buyer reference must be nonempty, bounded text without controls",
                ));
            }
            Ok(value.to_owned())
        })
        .transpose()
}
/// Validated immutable accounting allocations. Historical uniform invoices retain
/// their aggregate counter; explicit lines retain their individual rounded VAT.
#[derive(Debug)]
pub struct TaxProjection {
    pub legacy: bool,
    pub lines: Vec<TaxLine>,
    pub net: Money,
    pub vat: Money,
    pub gross: Money,
}
#[derive(Debug)]
pub struct TaxLine {
    pub index: usize,
    pub net: Money,
    pub vat: Money,
    pub vat_code: &'static str,
}
fn treatment(value: Option<&str>) -> Result<&str, Error> {
    match value.unwrap_or("standard") {
        value @ ("standard" | "domestic_reverse_charge") => Ok(value),
        _ => Err(refused(
            "native invoices currently support standard or domestic_reverse_charge VAT",
        )),
    }
}
fn basis(value: &str) -> Result<&str, Error> {
    match value {
        "DK_MOMSLOVEN_§46_STK_1_NR_6" | "DK_MOMSLOVEN_§46_STK_1_NR_7" => Ok(value),
        _ => Err(refused(
            "a supported explicit domestic reverseChargeBasis is required",
        )),
    }
}
fn classification<'a>(value: Option<&'a str>, domestic: bool) -> Result<&'a str, Error> {
    let value = value.unwrap_or(if domestic {
        "reverse_charge"
    } else {
        "taxable"
    });
    match value {
        "taxable" if domestic => Err(refused(
            "domestic reverse-charge invoices cannot contain taxable lines",
        )),
        "taxable" | "exempt" | "reverse_charge" => Ok(value),
        _ => Err(refused(
            "taxClassification must be taxable, exempt or reverse_charge",
        )),
    }
}
fn number(value: &Value) -> Result<String, Error> {
    value
        .as_number()
        .map(ToString::to_string)
        .ok_or_else(|| refused("invoice amounts must be finite numbers"))
}
fn money(value: &Value) -> Result<Money, Error> {
    let raw = number(value)?;
    let value = Money::parse(&raw)?;
    if crate::decimal::compare(&raw, &value.decimal())? != std::cmp::Ordering::Equal {
        return Err(refused("invoice amount must be rounded to cents"));
    }
    Ok(value)
}
fn taxable_rate(value: &Value) -> Result<(), Error> {
    if crate::decimal::compare(&number(value)?, "0.25")? != std::cmp::Ordering::Equal {
        return Err(refused(
            "native taxable invoice lines currently require vatRate 0.25",
        ));
    }
    Ok(())
}
fn line_base(quantity: &Value, price: &Value) -> Result<Money, Error> {
    let quantity = number(quantity)?;
    let price = number(price)?;
    if !crate::decimal::positive(&quantity)?
        || crate::decimal::compare(&price, "0")? == std::cmp::Ordering::Less
    {
        return Err(refused("quantity must be positive and price nonnegative"));
    }
    Money::multiply_decimal(&quantity, &price)
}
/// New issuance always freezes the effective line tax decision. It never
/// changes an already-issued payload or rerenders an already-saved PDF.
pub fn compute(input: &Input) -> Result<Value, Error> {
    if input.currency.as_deref().is_some_and(|s| s != "DKK") {
        return Err(refused("native invoice issuance currently supports DKK"));
    }
    if input
        .vat_rate_percent
        .as_ref()
        .is_some_and(|n| n.as_f64() != Some(25.0))
    {
        return Err(refused("native taxable invoices currently support 25% VAT"));
    }
    crate::fiscal_year::Date::parse(&input.issue_date)?;
    if input.lines.is_empty() || input.lines.len() > 200 {
        return Err(refused("invoice requires 1–200 lines"));
    }
    let treatment = treatment(input.vat_treatment.as_deref())?;
    let domestic = treatment == "domestic_reverse_charge";
    if domestic && input.vat_rate_percent.is_some() {
        return Err(refused(
            "domestic reverse-charge input must omit vatRatePercent",
        ));
    }
    let doc_basis = input
        .reverse_charge_basis
        .as_deref()
        .map(basis)
        .transpose()?;
    let note = input
        .reverse_charge_note
        .as_deref()
        .map(|s| text(s, 2000))
        .transpose()?;
    if domestic && (doc_basis.is_none() || note.is_none()) {
        return Err(refused(
            "domestic reverse-charge invoices require reverseChargeBasis and reverseChargeNote",
        ));
    }
    let mut net = Money::default();
    let mut vat = Money::default();
    let mut lines = Vec::new();
    let mut uniform = true;
    for line in &input.lines {
        let total = line_base(&json!(line.quantity), &json!(line.unit_price_ex_vat))?;
        let class = classification(line.tax_classification.as_deref(), domestic)?;
        let line_basis = line
            .reverse_charge_basis
            .as_deref()
            .map(basis)
            .transpose()?;
        let effective_basis = if class == "reverse_charge" {
            // An explicit line decision needs its own basis, as in vat-lines.ts.
            Some(
                line_basis
                    .or(if line.tax_classification.is_none() {
                        doc_basis
                    } else {
                        None
                    })
                    .ok_or_else(|| refused("reverse-charge lines require reverseChargeBasis"))?,
            )
        } else {
            None
        };
        let line_vat = if class == "taxable" {
            taxable_rate(&line.vat_rate.as_ref().map_or(json!(0.25), |n| json!(n)))?;
            total.percent("25")?
        } else {
            Money::default()
        };
        uniform &= class == "taxable";
        net.add(&total);
        vat.add(&line_vat);
        let mut result = json!({"description":text(&line.description,1000)?,"quantity":line.quantity,
            "unitPriceExVat":line.unit_price_ex_vat,"lineTotalExVat":wire(&total)?,
            "taxClassification":class,"vatRate":if class == "taxable" {0.25} else {0.0},
            "vatBase":wire(&total)?,"vatAmount":wire(&line_vat)?});
        if let Some(value) = effective_basis {
            result["reverseChargeBasis"] = json!(value);
        }
        lines.push(result);
    }
    if net == Money::default() {
        return Err(refused("invoice total must be positive"));
    }
    let mut gross = net.clone();
    gross.add(&vat);
    let mut totals = json!({"netAmount":wire(&net)?,"grossAmount":wire(&gross)?});
    if !domestic {
        totals["vatAmount"] = wire(&vat)?;
        if uniform {
            totals["vatRate"] = json!(0.25);
        }
    }
    let mut result = json!({"vatTreatment":treatment,"lines":lines,"totals":totals});
    if let Some(value) = doc_basis {
        result["reverseChargeBasis"] = json!(value);
    }
    if let Some(value) = note {
        result["reverseChargeNote"] = json!(value);
    }
    project(&result)?;
    Ok(result)
}
/// Verify saved source amounts before posting, crediting or accounting reads.
/// Missing classifications identify the historical aggregate-25 contract only.
pub fn project(payload: &Value) -> Result<TaxProjection, Error> {
    if payload
        .get("currency")
        .is_some_and(|v| v.as_str() != Some("DKK"))
    {
        return Err(refused("native tax projection currently supports DKK"));
    }
    let treatment = treatment(
        payload
            .get("vatTreatment")
            .map(|v| v.as_str().ok_or_else(|| refused("invalid vatTreatment")))
            .transpose()?,
    )?;
    let domestic = treatment == "domestic_reverse_charge";
    let doc_basis = payload
        .get("reverseChargeBasis")
        .map(|v| basis(v.as_str().unwrap_or("")))
        .transpose()?;
    if domestic {
        if doc_basis.is_none() {
            return Err(refused("domestic reverse-charge basis missing"));
        }
        text(payload["reverseChargeNote"].as_str().unwrap_or(""), 2000)?;
    }
    let source = payload["lines"]
        .as_array()
        .filter(|v| !v.is_empty() && v.len() <= 200)
        .ok_or_else(|| refused("invoice requires 1–200 lines"))?;
    let legacy = !domestic
        && source
            .iter()
            .all(|line| line.get("taxClassification").is_none());
    let mut net = Money::default();
    let mut vat = Money::default();
    let mut lines = Vec::new();
    for (index, line) in source.iter().enumerate() {
        let base = money(&line["lineTotalExVat"])?;
        if base.is_negative() || line_base(&line["quantity"], &line["unitPriceExVat"])? != base {
            return Err(refused(
                "lineTotalExVat must equal rounded quantity times unitPriceExVat",
            ));
        }
        let explicit = line.get("taxClassification");
        let class = classification(
            explicit
                .map(|v| {
                    v.as_str()
                        .ok_or_else(|| refused("invalid taxClassification"))
                })
                .transpose()?,
            domestic,
        )?;
        let (line_vat, code) = match class {
            "taxable" => {
                taxable_rate(line.get("vatRate").unwrap_or(&payload["totals"]["vatRate"]))?;
                (base.percent("25")?, "DK_SALE_25")
            }
            "exempt" => (Money::default(), "DK_SALE_EXEMPT"),
            _ => {
                let value = line
                    .get("reverseChargeBasis")
                    .and_then(Value::as_str)
                    .or(if explicit.is_none() { doc_basis } else { None })
                    .ok_or_else(|| refused("reverse-charge line basis missing"))?;
                basis(value)?;
                (Money::default(), "DOMESTIC_REVERSE_CHARGE_EXEMPT")
            }
        };
        if let Some(value) = line.get("vatBase")
            && money(value)? != base
        {
            return Err(refused("line vatBase differs from lineTotalExVat"));
        }
        if let Some(value) = line.get("vatAmount")
            && money(value)? != line_vat
        {
            return Err(refused(
                "line vatAmount differs from rounded tax projection",
            ));
        }
        net.add(&base);
        vat.add(&line_vat);
        lines.push(TaxLine {
            index,
            net: base,
            vat: line_vat,
            vat_code: code,
        });
    }
    if net == Money::default() {
        return Err(refused("invoice total must be positive"));
    }
    if legacy {
        taxable_rate(&payload["totals"]["vatRate"])?;
        vat = net.percent("25")?;
        lines = vec![TaxLine {
            index: 0,
            net: net.clone(),
            vat: vat.clone(),
            vat_code: "DK_SALE_25",
        }];
    }
    let mut gross = net.clone();
    gross.add(&vat);
    if money(&payload["totals"]["netAmount"])? != net
        || money(&payload["totals"]["grossAmount"])? != gross
    {
        return Err(refused("invoice totals differ from immutable line amounts"));
    }
    if domestic {
        if payload["totals"].get("vatRate").is_some()
            || payload["totals"].get("vatAmount").is_some()
            || vat != Money::default()
        {
            return Err(refused(
                "domestic reverse-charge totals must omit VAT fields",
            ));
        }
    } else if money(&payload["totals"]["vatAmount"])? != vat {
        return Err(refused(
            "invoice VAT differs from immutable line tax projection",
        ));
    }
    Ok(TaxProjection {
        legacy,
        lines,
        net,
        vat,
        gross,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recipient_routing_and_reference_bounds_preserve_explicit_evidence() {
        assert_eq!(
            recipient_ean(Some("5790 0000 00000")).unwrap().as_deref(),
            Some("5790000000000")
        );
        assert_eq!(recipient_ean(Some("  ")).unwrap(), None);
        for bad in ["579000000000", "57900000000000", "579000000000x"] {
            assert!(recipient_ean(Some(bad)).is_err());
        }
        assert_eq!(
            recipient_reference(Some(" Reviewed 漢字 "))
                .unwrap()
                .as_deref(),
            Some("Reviewed 漢字")
        );
        assert!(recipient_reference(Some("\n")).is_err());
        assert!(recipient_reference(Some(&"x".repeat(1025))).is_err());
        assert!(serde_json::from_value::<Party>(json!({"publicRecipient":"true"})).is_err());
    }
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
    fn computed(value: Value) -> Value {
        compute(&serde_json::from_value(value).unwrap()).unwrap()
    }
    #[test]
    fn mixed_lines_round_individually_and_keep_exact_tax_allocations() {
        let value = computed(json!({"issueDate":"2026-10-20","lines":[
            {"description":"A","quantity":1,"unitPriceExVat":0.02},
            {"description":"B","quantity":1,"unitPriceExVat":0.02},
            {"description":"Exempt","quantity":1,"unitPriceExVat":10,"taxClassification":"exempt"},
            {"description":"Reverse","quantity":1,"unitPriceExVat":20,"taxClassification":"reverse_charge","reverseChargeBasis":"DK_MOMSLOVEN_§46_STK_1_NR_6"}]}));
        let projection = project(&value).unwrap();
        assert!(!projection.legacy);
        assert_eq!(projection.net.decimal(), "30.04");
        assert_eq!(projection.vat.decimal(), "0.02");
        assert_eq!(projection.gross.decimal(), "30.06");
        assert!(value["totals"].get("vatRate").is_none());
        assert_eq!(
            projection
                .lines
                .iter()
                .map(|v| v.vat_code)
                .collect::<Vec<_>>(),
            vec![
                "DK_SALE_25",
                "DK_SALE_25",
                "DK_SALE_EXEMPT",
                "DOMESTIC_REVERSE_CHARGE_EXEMPT"
            ]
        );
        for index in 0..4 {
            let mut bad = value.clone();
            bad["lines"][index]["vatAmount"] = json!(1);
            assert!(project(&bad).is_err());
        }
        let mut bad = value.clone();
        bad["totals"]["vatAmount"] = json!(0.01);
        assert!(project(&bad).is_err());
        let mut bad = value.clone();
        bad["lines"][0]["quantity"] = json!(2);
        assert!(project(&bad).is_err());
        let mut bad = value;
        bad["lines"][2]["vatBase"] = json!(9);
        assert!(project(&bad).is_err());
    }
    #[test]
    fn domestic_reverse_requires_explicit_basis_note_and_omits_total_vat() {
        for basis in ["DK_MOMSLOVEN_§46_STK_1_NR_6", "DK_MOMSLOVEN_§46_STK_1_NR_7"] {
            let mut input = json!({"issueDate":"2026-10-20","vatTreatment":"domestic_reverse_charge",
                "reverseChargeBasis":basis,"reverseChargeNote":"Omvendt betalingspligt",
                "lines":[{"description":"Service","quantity":1,"unitPriceExVat":100}]});
            let value = computed(input.clone());
            assert!(value["totals"].get("vatAmount").is_none());
            assert!(value["totals"].get("vatRate").is_none());
            assert_eq!(value["lines"][0]["reverseChargeBasis"], basis);
            assert_eq!(project(&value).unwrap().vat.decimal(), "0.00");
            input["vatRatePercent"] = json!(25);
            assert!(compute(&serde_json::from_value(input.clone()).unwrap()).is_err());
            input.as_object_mut().unwrap().remove("vatRatePercent");
            input["reverseChargeNote"] = json!(" ");
            assert!(compute(&serde_json::from_value(input).unwrap()).is_err());
            let mut bad = value;
            bad["totals"]["vatAmount"] = json!(0);
            assert!(project(&bad).is_err());
        }
    }
    #[test]
    fn legacy_aggregate_vat_is_not_silently_rewritten_or_reallocated() {
        let legacy = json!({"lines":[
            {"quantity":1,"unitPriceExVat":0.02,"lineTotalExVat":0.02},
            {"quantity":1,"unitPriceExVat":0.02,"lineTotalExVat":0.02}],
            "totals":{"netAmount":0.04,"vatRate":0.25,"vatAmount":0.01,"grossAmount":0.05}});
        let projection = project(&legacy).unwrap();
        assert!(projection.legacy);
        assert_eq!(projection.lines.len(), 1);
        assert_eq!(projection.lines[0].vat.decimal(), "0.01");
        let mut invalid = legacy.clone();
        invalid["totals"]["vatAmount"] = json!(0.02);
        assert!(project(&invalid).is_err());
        let mut explicit = legacy;
        for line in explicit["lines"].as_array_mut().unwrap() {
            line["taxClassification"] = json!("taxable");
        }
        assert!(project(&explicit).is_err());
    }
    #[test]
    fn unsupported_rates_bases_and_zero_rate_inference_refuse() {
        for extra in [
            json!({"vatRate":0}),
            json!({"vatRate":0.2}),
            json!({"taxClassification":"reverse_charge"}),
            json!({"taxClassification":"reverse_charge","reverseChargeBasis":"EU_196"}),
            json!({"taxClassification":"unknown"}),
        ] {
            let mut line = json!({"description":"Service","quantity":1,"unitPriceExVat":100});
            line.as_object_mut()
                .unwrap()
                .extend(extra.as_object().unwrap().clone());
            let input: Input =
                serde_json::from_value(json!({"issueDate":"2026-10-20","lines":[line]})).unwrap();
            assert!(compute(&input).is_err());
        }
        let input:Input=serde_json::from_value(json!({"issueDate":"2026-10-20","vatTreatment":"foreign_reverse_charge","lines":[{"description":"Service","quantity":1,"unitPriceExVat":100}]})).unwrap();
        assert!(compute(&input).is_err());
        let exempt = computed(
            json!({"issueDate":"2026-10-20","lines":[{"description":"Service","quantity":1,"unitPriceExVat":100,"taxClassification":"exempt"}]}),
        );
        assert_eq!(project(&exempt).unwrap().vat.decimal(), "0.00");
        assert_eq!(exempt["totals"]["vatAmount"], 0.0);
    }
}
