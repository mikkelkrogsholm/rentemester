//! Original, offline invoice validation. Uses shared exact decimal and calendar primitives.
//! This accepts evidence for validation, independently of the stricter issuance contract.
use crate::{calendar::valid_iso_date, decimal};
use num_bigint::BigInt;
use serde_json::{Value, json};

fn text(v: &Value) -> bool {
    v.as_str().is_some_and(|s| !s.trim().is_empty())
}
fn number(v: &Value) -> bool {
    v.is_number()
}
fn nonnegative(v: &Value) -> bool {
    v.as_f64().is_some_and(|x| x >= 0.0)
}
fn positive(v: &Value) -> bool {
    v.as_f64().is_some_and(|x| x > 0.0)
}
fn numeric(v: &Value) -> String {
    if v.is_null() {
        "0".into()
    } else if let Some(s) = v.as_str() {
        s.to_owned()
    } else {
        v.to_string()
    }
}
fn scaled(v: &Value, scale: u8) -> BigInt {
    decimal::parse_scaled(&numeric(v), scale).unwrap_or_default()
}
fn cents(v: &Value) -> BigInt {
    scaled(v, 2)
}
fn display(v: &BigInt) -> String {
    let s = decimal::format_scaled(v, 2);
    s.trim_end_matches('0').trim_end_matches('.').to_owned()
}
fn product(a: &Value, b: &Value) -> BigInt {
    decimal::multiply(&numeric(a), &numeric(b), 2).unwrap_or_default()
}
fn date(v: &Value) -> bool {
    v.as_str().is_some_and(valid_iso_date)
}
fn ean(v: &Value) -> bool {
    v.as_str().is_some_and(|s| {
        let s: String = s.chars().filter(|c| !c.is_whitespace()).collect();
        s.len() == 13 && s.bytes().all(|b| b.is_ascii_digit())
    })
}

/// Return the original validator's complete verdict without allocating a number or accessing storage.
pub fn validate(p: &Value) -> Value {
    let mut errors: Vec<String> = vec![];
    let kind = p["invoiceType"].as_str().unwrap_or("");
    let treatment = p["vatTreatment"].as_str().unwrap_or("standard");
    let standard = treatment == "standard";
    let reverse = matches!(
        treatment,
        "domestic_reverse_charge" | "foreign_reverse_charge"
    );
    let full = kind == "full";
    let simplified = kind == "simplified";
    let t = &p["totals"];
    let lines = p["lines"].as_array();
    let explicit = lines.is_some_and(|ls| ls.iter().any(|l| text(&l["taxClassification"])));
    let complete = lines.is_some_and(|ls| ls.iter().all(|l| number(&l["lineTotalExVat"])));
    let mut rules = vec![
        if simplified {
            "DK-INVOICE-SIMPLIFIED-001"
        } else {
            "DK-INVOICE-FULL-001"
        },
        "DK-INVOICE-ARITHMETIC-001",
    ];
    if !date(&p["issueDate"]) {
        errors.push("issueDate must be present in YYYY-MM-DD format".into());
    }
    if p.get("dueDate").is_some() && !date(&p["dueDate"]) {
        errors.push("dueDate must be YYYY-MM-DD when present".into());
    }
    if date(&p["issueDate"])
        && date(&p["dueDate"])
        && p["dueDate"].as_str() < p["issueDate"].as_str()
    {
        errors.push("dueDate cannot be earlier than issueDate".into());
    }
    rules.push("DK-INVOICE-DELIVERY-DATE-001");
    for key in ["deliveryDate", "deliveryPeriodStart", "deliveryPeriodEnd"] {
        if p.get(key).is_some() && !date(&p[key]) {
            errors.push(format!("{key} must be YYYY-MM-DD when present"));
        }
    }
    let start = p.get("deliveryPeriodStart").is_some();
    let end = p.get("deliveryPeriodEnd").is_some();
    if start != end {
        errors.push("deliveryPeriodStart and deliveryPeriodEnd must be provided together".into());
    }
    if p.get("deliveryDate").is_some() && (start || end) {
        errors.push(
            "use either deliveryDate or deliveryPeriodStart/deliveryPeriodEnd, not both".into(),
        );
    }
    if date(&p["deliveryPeriodStart"])
        && date(&p["deliveryPeriodEnd"])
        && p["deliveryPeriodEnd"].as_str() < p["deliveryPeriodStart"].as_str()
    {
        errors.push("deliveryPeriodEnd cannot be earlier than deliveryPeriodStart".into());
    }
    if p.get("invoiceNumber").is_some() && !text(&p["invoiceNumber"]) {
        errors.push("invoiceNumber must not be blank when present".into());
    }
    for key in ["name", "address", "vatOrCvr"] {
        if !text(&p["seller"][key]) {
            errors.push(format!("seller.{key} is required"));
        }
    }
    if !lines.is_some_and(|ls| !ls.is_empty() && ls.iter().all(|l| text(&l["description"]))) {
        errors.push("lines must contain at least one described good or service".into());
    }
    if !nonnegative(&t["grossAmount"]) {
        errors.push("totals.grossAmount is required".into());
    }
    if full {
        for key in ["name", "address"] {
            if !text(&p["buyer"][key]) {
                errors.push(format!("buyer.{key} is required for full invoices"));
            }
        }
        if !nonnegative(&t["netAmount"]) {
            errors.push("totals.netAmount is required for full invoices".into());
        }
    }
    let valid_ean = ean(&p["buyer"]["eanNumber"]);
    if p["buyer"]["publicRecipient"] == true || valid_ean {
        rules.push("DK-INVOICE-PUBLIC-RECIPIENT-001");
        if !full {
            errors.push("public-recipient invoices must use invoiceType full".into());
        }
        if !valid_ean {
            errors
                .push("public-recipient invoices must include buyer.eanNumber as 13 digits".into());
        }
    }
    if simplified {
        if t["grossAmount"].as_f64().unwrap_or(f64::INFINITY) > 3000.0 {
            errors.push("simplified invoices are only allowed up to DKK 3,000 gross".into());
        }
        if !positive(&t["vatAmount"]) && t["vatComputationBasis"] != "VAT_20_OF_GROSS" {
            errors.push(
                "simplified invoices must include vatAmount or VAT_20_OF_GROSS computation basis"
                    .into(),
            );
        }
    }
    if standard && !explicit {
        rules.push("DK-VAT-SEPARATE-AMOUNT-001");
        for key in ["vatRate", "vatAmount"] {
            if !positive(&t[key]) {
                errors.push(format!("standard VAT invoices must include totals.{key}"));
            }
        }
    }
    if reverse {
        rules.extend([
            "DK-INVOICE-REVERSE-CHARGE-001",
            "DK-INVOICE-REVERSE-CHARGE-BASIS-001",
        ]);
        if !text(&p["reverseChargeBasis"]) {
            errors.push("reverse-charge invoices must include reverseChargeBasis".into());
        }
        for key in ["vatRate", "vatAmount"] {
            if t.get(key).is_some() {
                errors.push(format!(
                    "reverse-charge invoices must not include totals.{key}"
                ));
            }
        }
        let foreign = treatment == "foreign_reverse_charge";
        if foreign && !text(&p["buyer"]["vatOrCvr"]) {
            errors.push("foreign reverse-charge invoices must include buyer.vatOrCvr".into());
        }
        let allowed: &[&str] = if foreign {
            &[
                "DK_MOMSLOVEN_§46_STK_1_NR_3",
                "EU_MOMSDIREKTIV_ART_196",
                "EU_MOMSDIREKTIV_ART_199",
            ]
        } else {
            &["DK_MOMSLOVEN_§46_STK_1_NR_6", "DK_MOMSLOVEN_§46_STK_1_NR_7"]
        };
        if let Some(basis) = p["reverseChargeBasis"]
            .as_str()
            .filter(|s| !s.trim().is_empty())
            && !allowed.contains(&basis)
        {
            errors.push(format!(
                "reverseChargeBasis {basis} is not valid for {} reverse-charge invoices",
                if foreign { "foreign" } else { "domestic" }
            ));
        }
    }
    if let Some(lines) = lines {
        for (i, l) in lines.iter().enumerate() {
            if ["quantity", "unitPriceExVat", "lineTotalExVat"]
                .iter()
                .all(|k| number(&l[k]))
            {
                let expected = product(&l["quantity"], &l["unitPriceExVat"]);
                if cents(&l["lineTotalExVat"]) != expected {
                    errors.push(format!(
                        "lines[{i}].lineTotalExVat must equal quantity * unitPriceExVat ({})",
                        display(&expected)
                    ));
                }
            }
        }
    }
    // Original evidence accepts arbitrary positive VAT rates and description-only lines.
    // Quantization/multiplication use the same decimal primitives as the native tax engine.
    let mut projected_net = BigInt::default();
    let mut projected_vat = BigInt::default();
    if let Some(lines) = lines {
        for (i, l) in lines.iter().enumerate() {
            let base = cents(&l["lineTotalExVat"]);
            let class = l["taxClassification"]
                .as_str()
                .filter(|s| !s.is_empty())
                .unwrap_or(if standard {
                    "taxable"
                } else {
                    "reverse_charge"
                });
            if base < BigInt::default() {
                errors.push(format!(
                    "lines[{i}].lineTotalExVat must be a non-negative number"
                ));
            }
            if !matches!(class, "taxable" | "exempt" | "reverse_charge") {
                errors.push(format!(
                    "lines[{i}].taxClassification must be taxable, exempt or reverse_charge"
                ));
            }
            let rate = l
                .get("vatRate")
                .filter(|v| !v.is_null())
                .unwrap_or(&t["vatRate"]);
            if class == "taxable" && !positive(rate) {
                errors.push(format!("lines[{i}].vatRate is required for taxable lines"));
            }
            if class == "reverse_charge"
                && !text(&l["reverseChargeBasis"])
                && text(&l["taxClassification"])
            {
                errors.push(format!(
                    "lines[{i}].reverseChargeBasis is required for reverse-charge lines"
                ));
            }
            if class == "taxable" {
                projected_vat +=
                    decimal::multiply(&decimal::format_scaled(&base, 2), &numeric(rate), 2)
                        .unwrap_or_default();
            }
            projected_net += base;
        }
    }
    let projected_gross = &projected_net + &projected_vat;
    let net = cents(&t["netAmount"]);
    let vat = cents(&t["vatAmount"]);
    let gross = cents(&t["grossAmount"]);
    let fx = scaled(&t["fxRateToDkk"], 6);
    let net_dkk = cents(&t["netAmountDkk"]);
    let vat_dkk = cents(&t["vatAmountDkk"]);
    let gross_dkk = cents(&t["grossAmountDkk"]);
    if full && complete && net != projected_net {
        errors.push(format!(
            "totals.netAmount must equal sum of lineTotalExVat ({})",
            display(&projected_net)
        ));
    }
    if standard && complete {
        if t.get("netAmount").is_some() && net != projected_net {
            errors.push(format!(
                "totals.netAmount must equal rounded VAT line bases ({})",
                display(&projected_net)
            ));
        }
        if vat != projected_vat {
            errors.push(format!(
                "totals.vatAmount must equal rounded VAT line amounts ({})",
                display(&projected_vat)
            ));
        }
        if gross != projected_gross {
            errors.push(format!(
                "totals.grossAmount must equal rounded VAT line totals ({})",
                display(&projected_gross)
            ));
        }
    } else if explicit {
        for (key, value, expected, noun) in [
            ("netAmount", &net, &projected_net, "bases"),
            ("vatAmount", &vat, &projected_vat, "amounts"),
            ("grossAmount", &gross, &projected_gross, "totals"),
        ] {
            if value != expected {
                errors.push(format!(
                    "totals.{key} must equal explicit VAT line {noun} ({})",
                    display(expected)
                ));
            }
        }
    }
    if reverse && complete {
        if net != projected_net {
            errors.push(format!(
                "totals.netAmount must equal rounded reverse-charge line bases ({})",
                display(&projected_net)
            ));
        }
        if projected_vat != BigInt::default() {
            errors.push("reverse-charge invoice lines must not include VAT".into());
        }
        if gross != projected_gross {
            errors.push(format!(
                "totals.grossAmount must equal rounded reverse-charge line totals ({})",
                display(&projected_gross)
            ));
        }
    }
    if standard && !explicit && (full || t.get("netAmount").is_some()) && gross != &net + &vat {
        errors.push(format!(
            "totals.grossAmount must equal totals.netAmount + totals.vatAmount ({})",
            display(&(&net + &vat))
        ));
    }
    if standard && simplified && t.get("netAmount").is_none() && nonnegative(&t["vatAmount"]) {
        let rate = scaled(&t["vatRate"], 6);
        if gross <= vat {
            errors.push("totals.vatAmount must be less than totals.grossAmount".into());
        } else if rate > BigInt::default() {
            let expected = decimal::round_div(&(&gross * &rate), &(BigInt::from(1_000_000) + rate))
                .unwrap_or_default();
            let delta = &vat - &expected;
            if delta > BigInt::from(1) || delta < BigInt::from(-1) {
                errors.push(format!("totals.vatAmount must be the VAT contained in totals.grossAmount at totals.vatRate (≈ {})", display(&expected)));
            }
        }
    }
    if reverse && t.get("netAmount").is_some() && gross != net {
        errors.push(format!(
            "reverse-charge invoices must have totals.grossAmount equal totals.netAmount ({})",
            display(&net)
        ));
    }
    let raw_currency = p["currency"].as_str().unwrap_or("").trim().to_uppercase();
    let currency = if raw_currency.is_empty() {
        "DKK"
    } else {
        &raw_currency
    };
    if currency.chars().count() != 3 {
        errors.push("currency must be a 3-letter ISO code when present".into());
    }
    if currency != "DKK" {
        if fx <= BigInt::default() {
            errors.push("non-DKK invoices must include totals.fxRateToDkk".into());
        }
        for key in ["netAmountDkk", "vatAmountDkk", "grossAmountDkk"] {
            if (key != "vatAmountDkk" || standard) && !nonnegative(&t[key]) {
                errors.push(format!(
                    "non-DKK invoices must include totals.{key}{}",
                    if key == "vatAmountDkk" {
                        " for standard VAT"
                    } else {
                        ""
                    }
                ));
            }
        }
        if fx > BigInt::default() {
            for (key, source_key, value, source) in [
                ("netAmountDkk", "netAmount", &net_dkk, &net),
                ("vatAmountDkk", "vatAmount", &vat_dkk, &vat),
                ("grossAmountDkk", "grossAmount", &gross_dkk, &gross),
            ] {
                let expected = decimal::round_div(&(source * &fx), &BigInt::from(1_000_000))
                    .unwrap_or_default();
                if t.get(key).is_some() && value != &expected {
                    errors.push(format!(
                        "totals.{key} must equal totals.{source_key} * totals.fxRateToDkk ({})",
                        display(&expected)
                    ));
                }
            }
        }
        if standard
            && ["netAmountDkk", "vatAmountDkk", "grossAmountDkk"]
                .iter()
                .any(|k| t.get(k).is_some())
            && gross_dkk != &net_dkk + &vat_dkk
        {
            errors.push(format!(
                "totals.grossAmountDkk must equal totals.netAmountDkk + totals.vatAmountDkk ({})",
                display(&(&net_dkk + &vat_dkk))
            ));
        }
        if reverse && t.get("netAmountDkk").is_some() && gross_dkk != net_dkk {
            errors.push(format!("reverse-charge invoices must have totals.grossAmountDkk equal totals.netAmountDkk ({})", display(&net_dkk)));
        }
    }
    let mut result = json!({"ok":errors.is_empty(),"vatTreatment":treatment,"appliedRules":rules,"errors":errors});
    if let Some(kind) = p.get("invoiceType") {
        result["invoiceType"] = kind.clone();
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_source_cases_match_complete_verdicts() {
        let cases: Vec<Value> =
            serde_json::from_str(include_str!("invoice_validation/original_cases.json")).unwrap();
        assert!(cases.len() >= 25);
        for (i, case) in cases.iter().enumerate() {
            assert_eq!(
                validate(&case["input"]),
                case["expected"],
                "source case {i}: {}",
                case["input"]
            );
        }
    }
}
