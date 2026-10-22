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
    pub unit_code: Option<String>,
    pub quantity: Option<Number>,
    pub unit_price_ex_vat: Option<Number>,
    pub tax_classification: Option<String>,
    pub vat_rate: Option<Number>,
    pub reverse_charge_basis: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Input {
    pub invoice_type: Option<String>,
    pub totals: Option<SimplifiedTotals>,
    pub issue_date: String,
    pub delivery_date: Option<String>,
    pub delivery_period_start: Option<String>,
    pub delivery_period_end: Option<String>,
    pub unit_code: Option<String>,
    pub lines: Vec<Line>,
    pub vat_rate_percent: Option<Number>,
    pub vat_treatment: Option<String>,
    pub reverse_charge_basis: Option<String>,
    pub reverse_charge_note: Option<String>,
    pub customer_id: Option<i64>,
    pub invoice_number: Option<String>,
    pub due_date: Option<String>,
    pub currency: Option<String>,
    pub fx_rate_to_dkk: Option<Number>,
    pub seller: Option<Party>,
    pub buyer: Option<Party>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SimplifiedTotals {
    pub gross_amount: Number,
    pub vat_rate: Number,
    pub vat_computation_basis: Option<String>,
    pub vat_amount: Option<Number>,
    pub net_amount: Option<Number>,
}
pub fn invoice_type(value: Option<&str>) -> Result<&str, Error> {
    match value.unwrap_or("full") {
        kind @ ("full" | "simplified") => Ok(kind),
        _ => Err(refused("invoiceType must be full or simplified")),
    }
}
fn simplified_limit(payload: &Value, gross: &Money) -> Result<(), Error> {
    let rate = crate::sales_fx::rate(
        payload["currency"].as_str().unwrap_or("DKK"),
        payload["totals"]["fxRateToDkk"].as_number(),
    )?;
    if Money::parse("3000")?
        .subtract(&rate.apply(gross))
        .is_negative()
    {
        return Err(refused("simplified invoice gross exceeds DKK 3000"));
    }
    Ok(())
}
fn reduced_project(payload: &Value, canonical: bool) -> Result<TaxProjection, Error> {
    if payload["vatTreatment"].as_str().unwrap_or("standard") != "standard"
        || payload.get("reverseChargeBasis").is_some()
        || payload.get("reverseChargeNote").is_some()
        || payload["buyer"]["publicRecipient"] == true
        || recipient_ean(payload["buyer"]["eanNumber"].as_str())?.is_some()
    {
        return Err(refused(
            "simplified invoices require standard VAT and a non-public buyer",
        ));
    }
    let source = payload["lines"]
        .as_array()
        .filter(|v| !v.is_empty() && v.len() <= 200)
        .ok_or_else(|| refused("invoice requires 1–200 lines"))?;
    for line in source {
        text(line["description"].as_str().unwrap_or(""), 1000)?;
        if [
            "quantity",
            "unitPriceExVat",
            "lineTotalExVat",
            "taxClassification",
            "vatRate",
            "vatBase",
            "vatAmount",
            "reverseChargeBasis",
        ]
        .iter()
        .any(|key| line.get(key).is_some())
        {
            return Err(refused(
                "reduced simplified lines must contain descriptions without quantitative or tax fields",
            ));
        }
    }
    let totals = &payload["totals"];
    if crate::decimal::compare(&number(&totals["vatRate"])?, "0.25")? != std::cmp::Ordering::Equal {
        return Err(refused("simplified invoice requires contained 25% VAT"));
    }
    let gross = money(&totals["grossAmount"])?;
    if gross.is_negative() || gross == Money::default() {
        return Err(refused("simplified gross must be positive"));
    }
    simplified_limit(payload, &gross)?;
    let expected = gross.percent("20")?;
    let basis = totals.get("vatComputationBasis");
    if basis.is_some_and(|v| v != "VAT_20_OF_GROSS") {
        return Err(refused("unsupported simplified VAT computation basis"));
    }
    let vat = match totals.get("vatAmount") {
        Some(value) => money(value)?,
        None if basis.is_some() => expected.clone(),
        _ => {
            return Err(refused(
                "simplified invoice requires contained VAT amount or VAT_20_OF_GROSS",
            ));
        }
    };
    let diff = if vat.subtract(&expected).is_negative() {
        expected.subtract(&vat)
    } else {
        vat.subtract(&expected)
    };
    if vat.is_negative()
        || vat == Money::default()
        || gross == vat
        || gross.subtract(&vat).is_negative()
        || (canonical && vat != expected)
        || Money::parse("0.01")?.subtract(&diff).is_negative()
    {
        return Err(refused(
            "simplified VAT must reconcile to contained 25% VAT",
        ));
    }
    let net = gross.subtract(&vat);
    if let Some(value) = totals.get("netAmount")
        && money(value)? != net
    {
        return Err(refused(
            "simplified net must equal gross minus contained VAT",
        ));
    }
    Ok(TaxProjection {
        legacy: false,
        lines: vec![TaxLine {
            index: 0,
            net: net.clone(),
            vat: vat.clone(),
            vat_code: Some("DK_SALE_25"),
        }],
        net,
        vat,
        gross,
    })
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
    pub vat_code: Option<&'static str>,
}
fn treatment(value: Option<&str>) -> Result<&str, Error> {
    match value.unwrap_or("standard") {
        value @ ("standard" | "domestic_reverse_charge" | "foreign_reverse_charge") => Ok(value),
        _ => Err(refused("unsupported invoice VAT treatment")),
    }
}
/// Explicit source-supported foreign bases; identifiers never infer treatment.
pub fn foreign_reverse_charge_basis(value: &str) -> bool {
    matches!(
        value,
        "EU_B2B_SERVICES"
            | "DK_MOMSLOVEN_§46_STK_1_NR_3"
            | "EU_MOMSDIREKTIV_ART_196"
            | "EU_MOMSDIREKTIV_ART_199"
    )
}
fn basis(value: &str) -> Result<&str, Error> {
    if foreign_reverse_charge_basis(value)
        || matches!(
            value,
            "DK_MOMSLOVEN_§46_STK_1_NR_6" | "DK_MOMSLOVEN_§46_STK_1_NR_7"
        )
    {
        Ok(value)
    } else {
        Err(refused(
            "a supported explicit reverseChargeBasis is required",
        ))
    }
}
fn check_basis(foreign: bool, _domestic: bool, value: Option<&str>) -> Result<(), Error> {
    if (foreign && !value.is_some_and(foreign_reverse_charge_basis))
        || (!foreign && value.is_some_and(foreign_reverse_charge_basis))
    {
        return Err(refused(
            "reverse-charge basis must match the explicit invoice treatment",
        ));
    }
    Ok(())
}
fn classification(value: Option<&str>, domestic: bool) -> Result<&str, Error> {
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
fn taxable_rate(value: &Value) -> Result<String, Error> {
    let rate = number(value)?;
    if !crate::decimal::positive(&rate)? {
        return Err(refused("taxable invoice rate must be positive"));
    }
    Ok(rate)
}
fn taxable_code(rate: &str) -> Result<Option<&'static str>, Error> {
    Ok(
        (crate::decimal::compare(rate, "0.25")? == std::cmp::Ordering::Equal)
            .then_some("DK_SALE_25"),
    )
}
fn percentage_fraction(percent: &Number) -> Result<Value, Error> {
    let raw = crate::decimal::shift(&percent.to_string(), -2)?;
    let number: Number = raw.parse().map_err(|_| refused("invalid VAT percentage"))?;
    if crate::decimal::compare(&number.to_string(), &raw)? != std::cmp::Ordering::Equal {
        return Err(refused(
            "VAT percentage exceeds exact numeric storage range",
        ));
    }
    let value = json!(number);
    taxable_rate(&value)?;
    Ok(value)
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
/// Optional delivery evidence has no effect on monetary or tax decisions.
fn validate_metadata(payload: &Value) -> Result<(), Error> {
    let date = |key: &str| -> Result<Option<&str>, Error> {
        payload
            .get(key)
            .map(|value| {
                let value = value
                    .as_str()
                    .ok_or_else(|| refused("delivery dates must be ISO dates"))?;
                crate::fiscal_year::Date::parse(value)?;
                Ok(value)
            })
            .transpose()
    };
    let single = date("deliveryDate")?;
    let start = date("deliveryPeriodStart")?;
    let end = date("deliveryPeriodEnd")?;
    if start.is_some() != end.is_some() || (single.is_some() && start.is_some()) {
        return Err(refused("use a delivery date or a complete delivery period"));
    }
    if start.zip(end).is_some_and(|(start, end)| start > end) {
        return Err(refused("delivery period end must not precede its start"));
    }
    for value in std::iter::once(payload).chain(payload["lines"].as_array().into_iter().flatten()) {
        if let Some(unit) = value.get("unitCode") {
            text(
                unit.as_str()
                    .ok_or_else(|| refused("unitCode must be text"))?,
                32,
            )?;
        }
    }
    Ok(())
}
fn retain_metadata(input: &Input, payload: &mut Value) -> Result<(), Error> {
    for (key, value) in [
        ("deliveryDate", &input.delivery_date),
        ("deliveryPeriodStart", &input.delivery_period_start),
        ("deliveryPeriodEnd", &input.delivery_period_end),
        ("unitCode", &input.unit_code),
    ] {
        if let Some(value) = value {
            payload[key] = json!(if key == "unitCode" {
                text(value, 32)?
            } else {
                value.clone()
            });
        }
    }
    for (source, line) in input
        .lines
        .iter()
        .zip(payload["lines"].as_array_mut().unwrap())
    {
        if let Some(value) = &source.unit_code {
            line["unitCode"] = json!(text(value, 32)?);
        }
    }
    validate_metadata(payload)
}
/// New issuance always freezes the effective line tax decision. It never
/// changes an already-issued payload or rerenders an already-saved PDF.
pub fn compute(input: &Input) -> Result<Value, Error> {
    crate::sales_fx::rate(
        input.currency.as_deref().unwrap_or("DKK"),
        input.fx_rate_to_dkk.as_ref(),
    )?;
    let default_rate = input
        .vat_rate_percent
        .as_ref()
        .map(percentage_fraction)
        .transpose()?
        .unwrap_or(json!(0.25));
    crate::fiscal_year::Date::parse(&input.issue_date)?;
    if input.lines.is_empty() || input.lines.len() > 200 {
        return Err(refused("invoice requires 1–200 lines"));
    }
    let kind = invoice_type(input.invoice_type.as_deref())?;
    let treatment = treatment(input.vat_treatment.as_deref())?;
    if kind == "simplified"
        && (treatment != "standard"
            || input.reverse_charge_basis.is_some()
            || input.reverse_charge_note.is_some()
            || input.buyer.as_ref().is_some_and(|p| {
                p.public_recipient == Some(true)
                    || p.ean_number
                        .as_deref()
                        .is_some_and(|v| !v.trim().is_empty())
            }))
    {
        return Err(refused(
            "simplified invoices require standard VAT and a non-public buyer",
        ));
    }
    let reduced = input
        .lines
        .iter()
        .all(|line| line.quantity.is_none() && line.unit_price_ex_vat.is_none());
    if reduced {
        if crate::decimal::compare(&number(&default_rate)?, "0.25")? != std::cmp::Ordering::Equal {
            return Err(refused("simplified invoice requires contained 25% VAT"));
        }
        if kind != "simplified" {
            return Err(refused("full invoices require quantity and unitPriceExVat"));
        }
        let totals = input
            .totals
            .as_ref()
            .ok_or_else(|| refused("reduced simplified invoice requires totals"))?;
        let mut result = json!({"invoiceType":kind,"vatTreatment":treatment,"currency":input.currency.as_deref().unwrap_or("DKK"),"lines":input.lines.iter().map(|line| {
            let mut v = json!({"description":line.description});
            if let Some(value) = &line.tax_classification { v["taxClassification"] = json!(value); }
            if let Some(value) = &line.vat_rate { v["vatRate"] = json!(value); }
            if let Some(value) = &line.reverse_charge_basis { v["reverseChargeBasis"] = json!(value); }
            v
        }).collect::<Vec<_>>(),"totals":{"grossAmount":totals.gross_amount,"vatRate":totals.vat_rate}});
        for (key, value) in [
            ("vatAmount", &totals.vat_amount),
            ("netAmount", &totals.net_amount),
        ] {
            if let Some(value) = value {
                result["totals"][key] = json!(value);
            }
        }
        if let Some(value) = &totals.vat_computation_basis {
            result["totals"]["vatComputationBasis"] = json!(value);
        }
        if let Some(value) = &input.fx_rate_to_dkk {
            result["totals"]["fxRateToDkk"] = json!(value);
        }
        retain_metadata(input, &mut result)?;
        let projected = reduced_project(&result, true)?;
        result["totals"]["netAmount"] = wire(&projected.net)?;
        result["totals"]["vatAmount"] = wire(&projected.vat)?;
        result["totals"]["vatComputationBasis"] = json!("VAT_20_OF_GROSS");
        crate::sales_fx::freeze(&mut result, input.fx_rate_to_dkk.as_ref())?;
        return Ok(result);
    }
    if input.totals.is_some() {
        return Err(refused(
            "totals input belongs to reduced simplified invoices only",
        ));
    }
    if input
        .lines
        .iter()
        .any(|line| line.quantity.is_none() || line.unit_price_ex_vat.is_none())
    {
        return Err(refused(
            "invoice lines cannot mix complete and reduced amounts",
        ));
    }
    let foreign = treatment == "foreign_reverse_charge";
    let domestic = treatment == "domestic_reverse_charge";
    let reverse = domestic || foreign;
    if reverse && input.vat_rate_percent.is_some() {
        return Err(refused("reverse-charge input must omit vatRatePercent"));
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
    if reverse && doc_basis.is_none() {
        return Err(refused(
            "reverse-charge invoices require reverseChargeBasis",
        ));
    }
    let mut net = Money::default();
    let mut vat = Money::default();
    let mut lines = Vec::new();
    check_basis(foreign, domestic, doc_basis)?;
    let mut uniform = true;
    let mut uniform_rate: Option<String> = None;
    for line in &input.lines {
        let total = line_base(&json!(line.quantity), &json!(line.unit_price_ex_vat))?;
        let class = classification(line.tax_classification.as_deref(), reverse)?;
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
        if foreign && class != "reverse_charge" {
            return Err(refused(
                "EU services invoice requires reverse-charge service lines",
            ));
        }
        check_basis(foreign, domestic, effective_basis)?;
        let effective_rate = line
            .vat_rate
            .as_ref()
            .map_or_else(|| default_rate.clone(), |n| json!(n));
        let line_vat = if class == "taxable" {
            let rate = taxable_rate(&effective_rate)?;
            if let Some(previous) = &uniform_rate {
                uniform &= crate::decimal::compare(previous, &rate)? == std::cmp::Ordering::Equal;
            } else {
                uniform_rate = Some(rate.clone());
            }
            Money::multiply_decimal(&total.decimal(), &rate)?
        } else {
            Money::default()
        };
        if class != "taxable"
            && line.vat_rate.as_ref().is_some_and(|v| {
                crate::decimal::compare(&v.to_string(), "0").ok() != Some(std::cmp::Ordering::Equal)
            })
        {
            return Err(refused("non-taxable lines require zero VAT rate"));
        }
        uniform &= class == "taxable";
        net.add(&total);
        vat.add(&line_vat);
        let mut result = json!({"description":text(&line.description,1000)?,"quantity":line.quantity,
            "unitPriceExVat":line.unit_price_ex_vat,"lineTotalExVat":wire(&total)?,
            "taxClassification":class,"vatRate":if class == "taxable" {effective_rate} else {json!(0.0)},
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
            totals["vatRate"] = lines[0]["vatRate"].clone();
        }
    }
    let mut result =
        json!({"invoiceType":kind,"vatTreatment":treatment,"lines":lines,"totals":totals});
    if let Some(value) = doc_basis {
        result["reverseChargeBasis"] = json!(value);
    }
    if let Some(value) = note {
        result["reverseChargeNote"] = json!(value);
    }
    result["currency"] = json!(input.currency.as_deref().unwrap_or("DKK"));
    crate::sales_fx::freeze(&mut result, input.fx_rate_to_dkk.as_ref())?;
    if kind == "simplified" {
        simplified_limit(&result, &gross)?;
    }
    retain_metadata(input, &mut result)?;
    project(&result)?;
    Ok(result)
}
/// Verify saved source amounts before posting, crediting or accounting reads.
/// Uniform missing classifications retain historical aggregate rounding.
pub fn project(payload: &Value) -> Result<TaxProjection, Error> {
    validate_metadata(payload)?;
    crate::currency::Currency::parse(payload["currency"].as_str().unwrap_or("DKK"))?;
    let kind = invoice_type(
        payload
            .get("invoiceType")
            .map(|v| v.as_str().ok_or_else(|| refused("invalid invoiceType")))
            .transpose()?,
    )?;
    if kind == "simplified" {
        if payload["vatTreatment"].as_str().unwrap_or("standard") != "standard"
            || payload.get("reverseChargeBasis").is_some()
            || payload.get("reverseChargeNote").is_some()
            || payload["buyer"]["publicRecipient"] == true
            || recipient_ean(payload["buyer"]["eanNumber"].as_str())?.is_some()
        {
            return Err(refused(
                "simplified invoices require standard VAT and a non-public buyer",
            ));
        }
        if payload["lines"].as_array().is_some_and(|v| {
            v.iter()
                .any(|l| l.get("quantity").is_none() || l.get("unitPriceExVat").is_none())
        }) {
            return reduced_project(payload, false);
        }
    }
    let treatment = treatment(
        payload
            .get("vatTreatment")
            .map(|v| v.as_str().ok_or_else(|| refused("invalid vatTreatment")))
            .transpose()?,
    )?;
    let foreign = treatment == "foreign_reverse_charge";
    let domestic = treatment == "domestic_reverse_charge";
    let reverse = domestic || foreign;
    let doc_basis = payload
        .get("reverseChargeBasis")
        .map(|v| basis(v.as_str().unwrap_or("")))
        .transpose()?;
    check_basis(foreign, domestic, doc_basis)?;
    if reverse && doc_basis.is_none() {
        return Err(refused("reverse-charge basis missing"));
    }
    if let Some(note) = payload.get("reverseChargeNote") {
        text(note.as_str().unwrap_or(""), 2000)?;
    }
    let source = payload["lines"]
        .as_array()
        .filter(|v| !v.is_empty() && v.len() <= 200)
        .ok_or_else(|| refused("invoice requires 1–200 lines"))?;
    let mut legacy = !reverse
        && source
            .iter()
            .all(|line| line.get("taxClassification").is_none());
    let mut net = Money::default();
    let mut vat = Money::default();
    let mut lines = Vec::new();
    let mut uniform_rate: Option<String> = None;
    let mut uniform = true;
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
            reverse,
        )?;
        if foreign && class != "reverse_charge" {
            return Err(refused(
                "EU services invoice requires reverse-charge service lines",
            ));
        }
        let (line_vat, code) = match class {
            "taxable" => {
                let rate =
                    taxable_rate(line.get("vatRate").unwrap_or(&payload["totals"]["vatRate"]))?;
                if let Some(previous) = &uniform_rate {
                    uniform &=
                        crate::decimal::compare(previous, &rate)? == std::cmp::Ordering::Equal;
                } else {
                    uniform_rate = Some(rate.clone());
                }
                (
                    Money::multiply_decimal(&base.decimal(), &rate)?,
                    taxable_code(&rate)?,
                )
            }
            "exempt" => (Money::default(), Some("DK_SALE_EXEMPT")),
            _ => {
                let value = line
                    .get("reverseChargeBasis")
                    .and_then(Value::as_str)
                    .or(if explicit.is_none() { doc_basis } else { None })
                    .ok_or_else(|| refused("reverse-charge line basis missing"))?;
                basis(value)?;
                check_basis(foreign, domestic, Some(value))?;
                (
                    Money::default(),
                    Some(if foreign {
                        "REVERSE_CHARGE_EXEMPT"
                    } else {
                        "DOMESTIC_REVERSE_CHARGE_EXEMPT"
                    }),
                )
            }
        };
        if class != "taxable"
            && line.get("vatRate").is_some_and(|v| {
                number(v)
                    .and_then(|n| crate::decimal::compare(&n, "0"))
                    .ok()
                    != Some(std::cmp::Ordering::Equal)
            })
        {
            return Err(refused("non-taxable source line requires zero VAT rate"));
        }
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
        uniform &= class == "taxable";
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
    legacy &= uniform;
    if let Some(declared) = payload["totals"].get("vatRate") {
        let declared = taxable_rate(declared)?;
        // Original aggregate rate is a fallback: explicit line rates override it.
        // Only the matching historical uniform contract rounds in aggregate.
        legacy &= uniform_rate.as_ref().is_some_and(|rate| {
            crate::decimal::compare(rate, &declared).ok() == Some(std::cmp::Ordering::Equal)
        });
    } else {
        legacy = false;
    }
    if legacy {
        let rate = uniform_rate
            .as_ref()
            .ok_or_else(|| refused("missing taxable rate"))?;
        vat = Money::multiply_decimal(&net.decimal(), rate)?;
        lines = vec![TaxLine {
            index: 0,
            net: net.clone(),
            vat: vat.clone(),
            vat_code: taxable_code(rate)?,
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
    } else if foreign && payload["totals"].get("vatAmount").is_none() {
        // Historical foreign reverse-charge originals omit both VAT fields.
        // Keep this evidence unchanged; native issuance retains explicit zero.
        if payload["totals"].get("vatRate").is_some() || vat != Money::default() {
            return Err(refused(
                "omitted foreign VAT requires zero tax and no VAT rate",
            ));
        }
    } else if money(&payload["totals"]["vatAmount"])? != vat {
        return Err(refused(
            "invoice VAT differs from immutable line tax projection",
        ));
    }
    if kind == "simplified" {
        simplified_limit(payload, &gross)?;
        if lines.iter().any(|line| line.vat_code != Some("DK_SALE_25")) {
            return Err(refused(
                "simplified invoice requires standard 25% taxable lines",
            ));
        }
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
    fn full_rates_freeze_exact_fractions_and_never_invent_posting_codes() {
        let native = computed(json!({"issueDate":"2026-10-20","vatRatePercent":20,
            "lines":[{"description":"Synthetic","quantity":1,"unitPriceExVat":100}]}));
        assert_eq!(native["lines"][0]["vatRate"], 0.2);
        assert_eq!(native["totals"]["vatRate"], 0.2);
        assert_eq!(native["totals"]["vatAmount"], 20.0);
        assert_eq!(project(&native).unwrap().lines[0].vat_code, None);
        let precise = computed(json!({"issueDate":"2026-10-20","vatRatePercent":20.000006,
            "lines":[{"description":"Synthetic precise","quantity":1,"unitPriceExVat":100000}]}));
        assert_eq!(precise["lines"][0]["vatRate"], 0.20000006);
        assert_eq!(precise["totals"]["vatAmount"], 20000.01);
        let mixed = computed(json!({"issueDate":"2026-10-20","vatRatePercent":20,
            "lines":[{"description":"Fallback","quantity":1,"unitPriceExVat":100},
            {"description":"Override","quantity":1,"unitPriceExVat":100,"vatRate":0.25}]}));
        assert!(mixed["totals"].get("vatRate").is_none());
        assert_eq!(mixed["totals"]["vatAmount"], 45.0);
        let projection = project(&mixed).unwrap();
        assert_eq!(projection.lines[0].vat_code, None);
        assert_eq!(projection.lines[1].vat_code, Some("DK_SALE_25"));
        for (key, value) in [("vatAmount", json!(50)), ("grossAmount", json!(250))] {
            let mut forged = mixed.clone();
            forged["totals"][key] = value;
            assert!(project(&forged).is_err(), "{key}");
        }
        for rate in [json!(0), json!(-0.2), json!("0.2"), Value::Null] {
            let mut forged = native.clone();
            forged["lines"][0]["vatRate"] = rate;
            assert!(project(&forged).is_err());
        }
    }
    #[test]
    fn historical_non25_aggregate_and_mixed_line_rates_preserve_source_rounding() {
        let original = json!({"lines":[
            {"quantity":1,"unitPriceExVat":0.02,"lineTotalExVat":0.02},
            {"quantity":1,"unitPriceExVat":0.02,"lineTotalExVat":0.02}],
            "totals":{"netAmount":0.04,"vatRate":0.2,"vatAmount":0.01,"grossAmount":0.05}});
        let bytes = serde_json::to_vec(&original).unwrap();
        let projected = project(&original).unwrap();
        assert!(projected.legacy);
        assert_eq!(projected.vat.decimal(), "0.01");
        assert_eq!(projected.lines[0].vat_code, None);
        assert_eq!(serde_json::to_vec(&original).unwrap(), bytes);
        let mixed = json!({"lines":[
            {"quantity":1,"unitPriceExVat":100,"lineTotalExVat":100,"vatRate":0.2},
            {"quantity":1,"unitPriceExVat":100,"lineTotalExVat":100,"vatRate":0.25}],
            "totals":{"netAmount":200,"vatAmount":45,"grossAmount":245}});
        let projected = project(&mixed).unwrap();
        assert!(!projected.legacy);
        assert_eq!(projected.lines.len(), 2);
        assert_eq!(projected.vat.decimal(), "45.00");
        let mut fallback = mixed;
        fallback["totals"]["vatRate"] = json!(0.3);
        assert_eq!(project(&fallback).unwrap().vat.decimal(), "45.00");
    }
    #[test]
    fn reverse_custom_note_is_optional_and_supplied_text_stays_validated() {
        for (treatment, basis) in [
            ("domestic_reverse_charge", "DK_MOMSLOVEN_§46_STK_1_NR_6"),
            ("foreign_reverse_charge", "EU_MOMSDIREKTIV_ART_196"),
        ] {
            let input = json!({"issueDate":"2026-10-20","vatTreatment":treatment,
                "reverseChargeBasis":basis,"lines":[{"description":"Synthetic","quantity":1,"unitPriceExVat":100}]});
            let native = computed(input.clone());
            assert!(native.get("reverseChargeNote").is_none());
            assert_eq!(project(&native).unwrap().vat, Money::default());
            for bad in [json!(" "), json!("Unsupported 📄"), json!(42)] {
                let mut saved = native.clone();
                saved["reverseChargeNote"] = bad;
                assert!(project(&saved).is_err());
            }
            let mut custom = input;
            custom["reverseChargeNote"] = json!("Custom synthetic note");
            assert_eq!(
                computed(custom)["reverseChargeNote"],
                "Custom synthetic note"
            );
        }
    }
    #[test]
    fn supported_foreign_bases_preserve_native_and_omitted_zero_vat_originals() {
        for basis in [
            "EU_B2B_SERVICES",
            "DK_MOMSLOVEN_§46_STK_1_NR_3",
            "EU_MOMSDIREKTIV_ART_196",
            "EU_MOMSDIREKTIV_ART_199",
        ] {
            let source = json!({"issueDate":"2026-10-20","vatTreatment":"foreign_reverse_charge","reverseChargeBasis":basis,"reverseChargeNote":"Synthetic reverse charge","lines":[{"description":"Synthetic service","quantity":1,"unitPriceExVat":100}]});
            let native = computed(source);
            assert_eq!(native["reverseChargeBasis"], basis);
            assert_eq!(native["lines"][0]["reverseChargeBasis"], basis);
            assert_eq!(native["totals"]["vatAmount"], 0.0);
            assert_eq!(
                project(&native).unwrap().lines[0].vat_code,
                Some("REVERSE_CHARGE_EXEMPT")
            );
            let mut historical = native.clone();
            historical["totals"]
                .as_object_mut()
                .unwrap()
                .remove("vatAmount");
            for key in [
                "taxClassification",
                "reverseChargeBasis",
                "vatRate",
                "vatBase",
                "vatAmount",
            ] {
                historical["lines"][0].as_object_mut().unwrap().remove(key);
            }
            let original = serde_json::to_vec(&historical).unwrap();
            let projection = project(&historical).unwrap();
            assert_eq!(projection.gross.decimal(), "100.00");
            assert_eq!(projection.vat, Money::default());
            assert_eq!(projection.lines[0].vat_code, Some("REVERSE_CHARGE_EXEMPT"));
            assert_eq!(serde_json::to_vec(&historical).unwrap(), original);
            historical["totals"]["vatRate"] = json!(0);
            assert!(project(&historical).is_err());
            historical["totals"]
                .as_object_mut()
                .unwrap()
                .remove("vatRate");
            historical["totals"]["vatAmount"] = json!(1);
            assert!(project(&historical).is_err());
        }
    }
    #[test]
    fn foreign_basis_does_not_weaken_domestic_or_line_admission() {
        for (treatment, basis) in [
            ("foreign_reverse_charge", "UNKNOWN"),
            ("foreign_reverse_charge", "DK_MOMSLOVEN_§46_STK_1_NR_6"),
            ("foreign_reverse_charge", "DK_MOMSLOVEN_§46_STK_1_NR_7"),
            ("domestic_reverse_charge", "EU_MOMSDIREKTIV_ART_196"),
            ("standard", "EU_MOMSDIREKTIV_ART_199"),
        ] {
            let source = json!({"issueDate":"2026-10-20","vatTreatment":treatment,"reverseChargeBasis":basis,"reverseChargeNote":"Synthetic reverse charge","lines":[{"description":"Synthetic","quantity":1,"unitPriceExVat":100}]});
            assert!(compute(&serde_json::from_value(source).unwrap()).is_err());
        }
        let mut source = json!({"issueDate":"2026-10-20","vatTreatment":"foreign_reverse_charge","reverseChargeBasis":"EU_MOMSDIREKTIV_ART_196","reverseChargeNote":"Synthetic reverse charge","lines":[{"description":"Synthetic","quantity":1,"unitPriceExVat":100,"taxClassification":"reverse_charge","reverseChargeBasis":"UNKNOWN"}]});
        assert!(compute(&serde_json::from_value(source.clone()).unwrap()).is_err());
        source["lines"][0]["reverseChargeBasis"] = json!("DK_MOMSLOVEN_§46_STK_1_NR_6");
        assert!(compute(&serde_json::from_value(source.clone()).unwrap()).is_err());
        source["lines"][0]["reverseChargeBasis"] = json!("EU_MOMSDIREKTIV_ART_196");
        source["lines"][0]["taxClassification"] = json!("taxable");
        assert!(compute(&serde_json::from_value(source).unwrap()).is_err());
    }
    #[test]
    fn delivery_and_units_preserve_money_and_are_absent_by_default() {
        let source = json!({"issueDate":"2026-10-20","lines":[{"description":"Synthetic service","quantity":2,"unitPriceExVat":100} ]});
        let ordinary = computed(source.clone());
        for key in [
            "deliveryDate",
            "deliveryPeriodStart",
            "deliveryPeriodEnd",
            "unitCode",
        ] {
            assert!(ordinary.get(key).is_none());
        }
        assert!(ordinary["lines"][0].get("unitCode").is_none());
        let mut source = source;
        source["deliveryPeriodStart"] = json!("2026-10-01");
        source["deliveryPeriodEnd"] = json!("2026-10-31");
        source["unitCode"] = json!(" HUR ");
        source["lines"][0]["unitCode"] = json!("H87");
        let retained = computed(source);
        assert_eq!(retained["totals"], ordinary["totals"]);
        assert_eq!(retained["deliveryPeriodStart"], "2026-10-01");
        assert_eq!(retained["deliveryPeriodEnd"], "2026-10-31");
        assert_eq!(retained["unitCode"], "HUR");
        assert_eq!(retained["lines"][0]["unitCode"], "H87");
        assert_eq!(
            project(&retained).unwrap().gross,
            project(&ordinary).unwrap().gross
        );
        let reduced = computed(
            json!({"invoiceType":"simplified","issueDate":"2026-10-20","deliveryDate":"2026-10-19","unitCode":"HUR","lines":[{"description":"Synthetic service","unitCode":"H87"}],"totals":{"grossAmount":100.07,"vatRate":0.25,"vatComputationBasis":"VAT_20_OF_GROSS"}}),
        );
        assert_eq!(reduced["deliveryDate"], "2026-10-19");
        assert_eq!(reduced["lines"][0]["unitCode"], "H87");
        assert!(reduced["lines"][0].get("quantity").is_none());
        assert!(reduced["lines"][0].get("unitPriceExVat").is_none());
        assert_eq!(project(&reduced).unwrap().gross.decimal(), "100.07");
    }
    #[test]
    fn malformed_delivery_evidence_and_units_fail_closed_on_input_and_saved_source() {
        let base = json!({"issueDate":"2026-10-20","lines":[{"description":"Synthetic service","quantity":1,"unitPriceExVat":100}]});
        for extra in [
            json!({"deliveryDate":"2026-02-29"}),
            json!({"deliveryPeriodStart":"2026-10-01"}),
            json!({"deliveryPeriodEnd":"2026-10-31"}),
            json!({"deliveryPeriodStart":"2026-10-31","deliveryPeriodEnd":"2026-10-01"}),
            json!({"deliveryDate":"2026-10-20","deliveryPeriodStart":"2026-10-01","deliveryPeriodEnd":"2026-10-31"}),
            json!({"unitCode":" "}),
            json!({"unitCode":"x".repeat(33)}),
            json!({"unitCode":"H\nUR"}),
            json!({"unitCode":"📦"}),
        ] {
            let mut input = base.clone();
            input
                .as_object_mut()
                .unwrap()
                .extend(extra.as_object().unwrap().clone());
            assert!(
                compute(&serde_json::from_value(input).unwrap()).is_err(),
                "{extra}"
            );
            let mut saved = computed(base.clone());
            saved
                .as_object_mut()
                .unwrap()
                .extend(extra.as_object().unwrap().clone());
            assert!(project(&saved).is_err(), "{extra}");
        }
        let mut input = base;
        input["lines"][0]["unitCode"] = json!("x".repeat(33));
        assert!(compute(&serde_json::from_value(input).unwrap()).is_err());
        let mut saved = computed(
            json!({"issueDate":"2026-10-20","lines":[{"description":"Synthetic","quantity":1,"unitPriceExVat":100}]}),
        );
        saved["lines"][0]["unitCode"] = json!(42);
        assert!(project(&saved).is_err());
    }
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
        input.lines[0].quantity = Some(0.into());
        assert!(compute(&input).is_err());
        input.lines[0].quantity = Some(1.into());
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
                Some("DK_SALE_25"),
                Some("DK_SALE_25"),
                Some("DK_SALE_EXEMPT"),
                Some("DOMESTIC_REVERSE_CHARGE_EXEMPT")
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
    fn domestic_reverse_requires_explicit_basis_valid_note_and_omits_total_vat() {
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
            json!({"vatRate":-0.2}),
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

#[cfg(test)]
mod private_tests {
    use super::*;
    fn input(gross: Value) -> Value {
        json!({"issueDate":"2026-10-20","invoiceType":"simplified","lines":[{"description":"Synthetic reduced sale"}],"totals":{"grossAmount":gross,"vatRate":0.25,"vatComputationBasis":"VAT_20_OF_GROSS"}})
    }
    fn compute_value(value: Value) -> Result<Value, Error> {
        compute(&serde_json::from_value(value).unwrap())
    }
    #[test]
    fn gross_based_native_cents_are_canonical_without_invented_line_amounts() {
        let payload = compute_value(input(json!(100.07))).unwrap();
        assert_eq!(
            payload["lines"],
            json!([{"description":"Synthetic reduced sale"}])
        );
        assert_eq!(
            payload["totals"],
            json!({"grossAmount":100.07,"vatRate":0.25,"vatComputationBasis":"VAT_20_OF_GROSS","netAmount":80.06,"vatAmount":20.01})
        );
        let projection = project(&payload).unwrap();
        assert!(!projection.legacy);
        assert_eq!(projection.lines.len(), 1);
        assert_eq!(projection.lines[0].net.decimal(), "80.06");
        assert_eq!(projection.lines[0].vat.decimal(), "20.01");
        for gross in [json!(0.03), json!(0.04), json!(3000)] {
            assert!(compute_value(input(gross)).is_ok());
        }
        for gross in [
            json!(0),
            json!(-1),
            json!(0.01),
            json!(3000.01),
            json!(100.071),
        ] {
            assert!(compute_value(input(gross)).is_err());
        }
    }
    #[test]
    fn native_totals_are_derived_and_malformed_mixed_reverse_and_public_inputs_refuse() {
        let base = input(json!(100.07));
        let mut cases = vec![];
        for (key, value) in [
            ("vatAmount", json!(20.02)),
            ("netAmount", json!(80.05)),
            ("vatRate", json!(0.20)),
            ("vatComputationBasis", json!("OTHER")),
        ] {
            let mut bad = base.clone();
            bad["totals"][key] = value;
            cases.push(bad);
        }
        for (key, value) in [
            ("quantity", json!(1)),
            ("unitPriceExVat", json!(100)),
            ("taxClassification", json!("exempt")),
            ("reverseChargeBasis", json!("DK_MOMSLOVEN_§46_STK_1_NR_6")),
        ] {
            let mut bad = base.clone();
            bad["lines"][0][key] = value;
            cases.push(bad);
        }
        for (key, value) in [
            ("invoiceType", json!("other")),
            ("invoiceType", json!("full")),
            ("vatTreatment", json!("domestic_reverse_charge")),
            ("vatTreatment", json!("foreign_reverse_charge")),
            ("reverseChargeNote", json!("note")),
            ("buyer", json!({"publicRecipient":true})),
            (
                "buyer",
                json!({"eanNumber":"5790000000000","publicRecipient":false}),
            ),
        ] {
            let mut bad = base.clone();
            bad[key] = value;
            cases.push(bad);
        }
        for bad in cases {
            assert!(compute_value(bad.clone()).is_err(), "{bad}");
        }
        let mut exact = base;
        exact["totals"]["vatAmount"] = json!(20.01);
        exact["totals"]["netAmount"] = json!(80.06);
        assert!(compute_value(exact).is_ok());
        let full = json!({"issueDate":"2026-10-20","lines":[{"description":"Synthetic full","quantity":1,"unitPriceExVat":100}]});
        assert!(compute_value(full).is_ok());
    }
    #[test]
    fn historical_reduced_source_preserves_valid_contained_vat_with_one_cent_tolerance() {
        let mut payload = input(json!(100.07));
        payload["totals"]
            .as_object_mut()
            .unwrap()
            .remove("vatComputationBasis");
        payload["totals"]["vatAmount"] = json!(20.02);
        let unchanged = payload.clone();
        let projected = project(&payload).unwrap();
        assert_eq!(projected.vat.decimal(), "20.02");
        assert_eq!(projected.net.decimal(), "80.05");
        assert_eq!(payload, unchanged);
        payload["totals"]["vatAmount"] = json!(20.03);
        assert!(project(&payload).is_err());
        payload["totals"]
            .as_object_mut()
            .unwrap()
            .remove("vatAmount");
        assert!(project(&payload).is_err());
        payload["totals"]["vatComputationBasis"] = json!("VAT_20_OF_GROSS");
        assert_eq!(project(&payload).unwrap().vat.decimal(), "20.01");
    }
    #[test]
    fn simplified_foreign_limit_uses_frozen_dkk_gross_not_original_numeric_value() {
        let mut payload = input(json!(400));
        payload["currency"] = json!("EUR");
        payload["fxRateToDkk"] = json!(7.5);
        let result = compute_value(payload.clone()).unwrap();
        assert_eq!(
            crate::sales_fx::project(&result).unwrap().gross.decimal(),
            "3000.00"
        );
        payload["totals"]["grossAmount"] = json!(400.01);
        assert!(compute_value(payload).is_err());
        let mut payload = input(json!(3000.01));
        payload["currency"] = json!("EUR");
        payload["fxRateToDkk"] = json!(0.5);
        assert!(compute_value(payload).is_ok());
        let complete = json!({"invoiceType":"simplified","issueDate":"2026-10-20","currency":"EUR","fxRateToDkk":7.5,"lines":[{"description":"Synthetic full quantitative","quantity":1,"unitPriceExVat":320}]});
        assert!(compute_value(complete.clone()).is_ok());
        let mut too_large = complete;
        too_large["lines"][0]["unitPriceExVat"] = json!(320.01);
        assert!(compute_value(too_large).is_err());
    }
}
