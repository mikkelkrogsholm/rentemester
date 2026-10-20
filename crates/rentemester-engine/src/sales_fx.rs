//! Issue-time exact DKK valuation. Source currency amounts remain unchanged.
use crate::{
    Error,
    currency::Currency,
    decimal,
    fx::FxRate,
    money::Money,
    sales_invoice::{self, TaxLine, TaxProjection, refused, wire},
};
use serde_json::{Number, Value, json};

pub fn rate(currency: &str, input: Option<&Number>) -> Result<FxRate, Error> {
    Currency::parse(currency)?;
    let raw = input.map(ToString::to_string);
    let raw = raw
        .as_deref()
        .unwrap_or(if currency == "DKK" { "1" } else { "" });
    let rate = FxRate::parse(raw)?;
    if decimal::compare(raw, &rate.decimal())? != std::cmp::Ordering::Equal {
        return Err(refused("invoice FX rate must be exact at six decimals"));
    }
    if currency == "DKK" && rate != FxRate::parse("1")? {
        return Err(refused("DKK invoice FX rate must be one"));
    }
    Ok(rate)
}
fn amount(value: &Value) -> Result<Money, Error> {
    let n = value
        .as_number()
        .ok_or_else(|| refused("missing frozen DKK amount"))?
        .to_string();
    let m = Money::parse(&n)?;
    if decimal::compare(&n, &m.decimal())? != std::cmp::Ordering::Equal {
        return Err(refused("frozen DKK amount must be exact cents"));
    }
    Ok(m)
}
fn split(source: &[Money], rate: &FxRate, target: &Money) -> Result<Vec<Money>, Error> {
    let mut result: Vec<_> = source.iter().map(|v| rate.apply(v)).collect();
    let mut sum = Money::default();
    for v in &result {
        sum.add(v);
    }
    let residual = target.subtract(&sum);
    if residual != Money::default() {
        let largest = source
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| decimal::compare(&a.decimal(), &b.decimal()).unwrap())
            .map(|(i, _)| i)
            .ok_or_else(|| refused("empty FX allocation"))?;
        result[largest].add(&residual);
        if result[largest].is_negative() {
            return Err(refused("FX rounding would make source allocation negative"));
        }
    }
    Ok(result)
}
fn converted(source: &TaxProjection, rate: &FxRate) -> Result<TaxProjection, Error> {
    let net = rate.apply(&source.net);
    let gross = rate.apply(&source.gross);
    let vat = gross.subtract(&net);
    if gross == Money::default() || vat.is_negative() {
        return Err(refused(
            "foreign invoice DKK gross must be positive with nonnegative VAT",
        ));
    }
    let nets = split(
        &source
            .lines
            .iter()
            .map(|l| l.net.clone())
            .collect::<Vec<_>>(),
        rate,
        &net,
    )?;
    let vats = split(
        &source
            .lines
            .iter()
            .map(|l| l.vat.clone())
            .collect::<Vec<_>>(),
        rate,
        &vat,
    )?;
    Ok(TaxProjection {
        legacy: source.legacy,
        net,
        vat,
        gross,
        lines: source
            .lines
            .iter()
            .enumerate()
            .map(|(i, l)| TaxLine {
                index: l.index,
                net: nets[i].clone(),
                vat: vats[i].clone(),
                vat_code: l.vat_code,
            })
            .collect(),
    })
}
pub fn freeze(payload: &mut Value, input: Option<&Number>) -> Result<(), Error> {
    let currency = payload["currency"].as_str().unwrap_or("DKK");
    let rate = rate(currency, input)?;
    if currency == "DKK" {
        return Ok(());
    }
    payload["totals"]["fxRateToDkk"] =
        serde_json::from_str(&rate.decimal()).map_err(|_| refused("invalid rate wire"))?;
    let source = sales_invoice::project(payload)?;
    let dkk = converted(&source, &rate)?;
    payload["fxValuationVersion"] = json!(1);
    payload["totals"]["fxRateToDkk"] =
        serde_json::from_str(&rate.decimal()).map_err(|_| refused("invalid rate wire"))?;
    for (key, value) in [
        ("netAmountDkk", &dkk.net),
        ("vatAmountDkk", &dkk.vat),
        ("grossAmountDkk", &dkk.gross),
    ] {
        payload["totals"][key] = wire(value)?;
    }
    for line in &dkk.lines {
        payload["lines"][line.index]["vatBaseDkk"] = wire(&line.net)?;
        payload["lines"][line.index]["vatAmountDkk"] = wire(&line.vat)?;
    }
    Ok(())
}
/// Source TypeScript booking scales bases to the verified aggregate and puts
/// the cent residual on the final income line. These are ephemeral accounting
/// units, never additions to the immutable original or native version-one data.
fn legacy_split(source: &[Money], total: &Money, target: &Money) -> Result<Vec<Money>, Error> {
    if total == &Money::default() {
        if target != &Money::default() {
            return Err(refused("legacy FX zero base has a nonzero allocation"));
        }
        return Ok(vec![Money::default(); source.len()]);
    }
    let mut result = source
        .iter()
        .map(|value| decimal::round_div(&(&value.0 * &target.0), &total.0).map(Money))
        .collect::<Result<Vec<_>, _>>()?;
    let mut sum = Money::default();
    for value in &result {
        sum.add(value);
    }
    let residual = target.subtract(&sum);
    let last = result
        .last_mut()
        .ok_or_else(|| refused("legacy FX requires income lines"))?;
    last.add(&residual);
    if result.iter().any(Money::is_negative) {
        return Err(refused("legacy FX residual makes an allocation negative"));
    }
    Ok(result)
}
fn legacy_project(
    payload: &Value,
    source: TaxProjection,
    currency: &str,
) -> Result<TaxProjection, Error> {
    if payload["lines"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|line| line.get("vatBaseDkk").is_some() || line.get("vatAmountDkk").is_some())
    {
        return Err(refused(
            "unversioned FX cannot contain native DKK line allocations",
        ));
    }
    Currency::parse(currency)?;
    let raw = payload["totals"]["fxRateToDkk"]
        .as_number()
        .ok_or_else(|| refused("legacy foreign invoice requires a frozen quote"))?
        .to_string();
    // invoice.ts validates its six-decimal quote but retains the input bytes.
    let rate = FxRate::parse(&raw)?;
    let net = amount(&payload["totals"]["netAmountDkk"])?;
    let gross = amount(&payload["totals"]["grossAmountDkk"])?;
    let reverse = matches!(
        payload["vatTreatment"].as_str(),
        Some("domestic_reverse_charge" | "foreign_reverse_charge")
    );
    let vat = match payload["totals"].get("vatAmountDkk") {
        Some(value) => amount(value)?,
        None if reverse => Money::default(),
        None => return Err(refused("legacy standard invoice requires frozen VAT DKK")),
    };
    let mut conserved = net.clone();
    conserved.add(&vat);
    if net != rate.apply(&source.net)
        || vat != rate.apply(&source.vat)
        || gross != rate.apply(&source.gross)
        || gross != conserved
        || net.is_negative()
        || net == Money::default()
        || vat.is_negative()
    {
        return Err(refused(
            "legacy frozen DKK totals differ from source FX aggregates",
        ));
    }
    let nets = legacy_split(
        &source
            .lines
            .iter()
            .map(|line| line.net.clone())
            .collect::<Vec<_>>(),
        &source.net,
        &net,
    )?;
    // The source stores VAT as one aggregate journal counter. Derive only
    // ephemeral taxable counters; exempt lines must never absorb VAT rounding.
    let taxable: Vec<_> = source
        .lines
        .iter()
        .enumerate()
        .filter(|(_, line)| line.vat != Money::default())
        .collect();
    let mut vats = vec![Money::default(); source.lines.len()];
    if !taxable.is_empty() {
        let allocated = legacy_split(
            &taxable
                .iter()
                .map(|(_, line)| line.vat.clone())
                .collect::<Vec<_>>(),
            &source.vat,
            &vat,
        )?;
        for ((index, _), value) in taxable.into_iter().zip(allocated) {
            vats[index] = value;
        }
    }
    Ok(TaxProjection {
        legacy: source.legacy,
        net,
        vat,
        gross,
        lines: source
            .lines
            .into_iter()
            .enumerate()
            .map(|(index, line)| TaxLine {
                index: line.index,
                net: nets[index].clone(),
                vat: vats[index].clone(),
                vat_code: line.vat_code,
            })
            .collect(),
    })
}
/// Validate the complete immutable allocation, returning only DKK accounting units.
pub fn project(payload: &Value) -> Result<TaxProjection, Error> {
    let source = sales_invoice::project(payload)?;
    let currency = payload["currency"].as_str().unwrap_or("DKK");
    if currency == "DKK" {
        if payload.get("fxValuationVersion").is_some() {
            return Err(refused("unexpected DKK valuation version"));
        }
        if let Some(n) = payload["totals"].get("fxRateToDkk") {
            rate(
                currency,
                Some(n.as_number().ok_or_else(|| refused("invalid FX rate"))?),
            )?;
        }
        return Ok(source);
    }
    if payload.get("fxValuationVersion").is_none() {
        return legacy_project(payload, source, currency);
    }
    if payload["fxValuationVersion"] != 1 {
        return Err(refused(
            "foreign invoice requires frozen DKK valuation version",
        ));
    }
    let rate = rate(
        currency,
        Some(
            payload["totals"]["fxRateToDkk"]
                .as_number()
                .ok_or_else(|| refused("foreign invoice requires explicit FX rate"))?,
        ),
    )?;
    let dkk = converted(&source, &rate)?;
    for (key, value) in [
        ("netAmountDkk", &dkk.net),
        ("vatAmountDkk", &dkk.vat),
        ("grossAmountDkk", &dkk.gross),
    ] {
        if &amount(&payload["totals"][key])? != value {
            return Err(refused("frozen DKK total differs from exact FX projection"));
        }
    }
    for line in &dkk.lines {
        if amount(&payload["lines"][line.index]["vatBaseDkk"])? != line.net
            || amount(&payload["lines"][line.index]["vatAmountDkk"])? != line.vat
        {
            return Err(refused("frozen DKK line differs from exact FX allocation"));
        }
    }
    Ok(dkk)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn without_native_allocation(mut payload: Value) -> Value {
        payload
            .as_object_mut()
            .unwrap()
            .remove("fxValuationVersion");
        for line in payload["lines"].as_array_mut().unwrap() {
            line.as_object_mut().unwrap().remove("vatBaseDkk");
            line.as_object_mut().unwrap().remove("vatAmountDkk");
        }
        payload
    }
    fn reverse_invoice() -> Value {
        sales_invoice::compute(&serde_json::from_value(json!({"issueDate":"2025-08-01","currency":"EUR","fxRateToDkk":7.46,"vatTreatment":"foreign_reverse_charge","reverseChargeBasis":"EU_B2B_SERVICES","reverseChargeNote":"Synthetic reverse charge","lines":[{"description":"Synthetic A","quantity":1,"unitPriceExVat":0.01},{"description":"Synthetic B","quantity":1,"unitPriceExVat":0.01}]})).unwrap()).unwrap()
    }
    #[test]
    fn historical_aggregate_preserves_original_and_source_final_line_rounding() {
        let native = reverse_invoice();
        let native_projection = project(&native).unwrap();
        let mut old = without_native_allocation(native.clone());
        old["totals"]
            .as_object_mut()
            .unwrap()
            .remove("vatAmountDkk");
        let bytes = serde_json::to_vec(&old).unwrap();
        let projected = project(&old).unwrap();
        assert_eq!(projected.net.decimal(), "0.15");
        assert_eq!(projected.gross.decimal(), "0.15");
        assert_eq!(projected.vat, Money::default());
        assert_eq!(projected.lines[0].net.decimal(), "0.08");
        assert_eq!(projected.lines[1].net.decimal(), "0.07");
        assert_ne!(projected.lines[0].net, native_projection.lines[0].net);
        assert_eq!(serde_json::to_vec(&old).unwrap(), bytes);
        assert_eq!(
            project(&native).unwrap().lines[0].net,
            native_projection.lines[0].net
        );
        old["totals"]["fxRateToDkk"] = json!(7.4600001);
        assert_eq!(project(&old).unwrap().gross.decimal(), "0.15");
        old["totals"]["vatAmountDkk"] = json!(0);
        assert!(project(&old).is_ok());
    }
    #[test]
    fn historical_aggregate_rejects_partial_mixed_and_tampered_sources() {
        let old = without_native_allocation(reverse_invoice());
        for key in [
            "fxRateToDkk",
            "netAmountDkk",
            "grossAmountDkk",
            "vatAmountDkk",
        ] {
            let mut bad = old.clone();
            bad["totals"][key] = json!(1);
            assert!(project(&bad).is_err(), "{key}");
        }
        for key in ["fxRateToDkk", "netAmountDkk", "grossAmountDkk"] {
            let mut bad = old.clone();
            bad["totals"].as_object_mut().unwrap().remove(key);
            assert!(project(&bad).is_err(), "{key}");
        }
        for version in [json!(null), json!(1), json!(2)] {
            let mut bad = old.clone();
            bad["fxValuationVersion"] = version;
            assert!(project(&bad).is_err());
        }
        for key in ["vatBaseDkk", "vatAmountDkk"] {
            let mut bad = old.clone();
            bad["lines"][0][key] = json!(0);
            assert!(project(&bad).is_err());
        }
        let native = sales_invoice::compute(&serde_json::from_value(json!({"issueDate":"2025-08-01","currency":"EUR","fxRateToDkk":1.5,"lines":[{"description":"Synthetic standard","quantity":1,"unitPriceExVat":0.03}]})).unwrap()).unwrap();
        assert!(project(&native).is_ok());
        // Native v1 balances by gross minus net; historical source independently
        // converts VAT and requires the three aggregates to conserve exactly.
        let mut old = without_native_allocation(native);
        assert!(project(&old).is_err());
        old["totals"]["vatAmountDkk"] = json!(0.02);
        assert!(project(&old).is_err());
    }
    #[test]
    fn historical_standard_aggregate_keeps_verified_vat_without_invented_fields() {
        let mut old = without_native_allocation(sales_invoice::compute(&serde_json::from_value(json!({"issueDate":"2025-08-01","currency":"EUR","fxRateToDkk":1.5,"lines":[{"description":"Synthetic standard","quantity":1,"unitPriceExVat":100}]})).unwrap()).unwrap());
        // The source's older uniform invoices have no explicit classification.
        for key in ["taxClassification", "vatRate", "vatBase", "vatAmount"] {
            old["lines"][0].as_object_mut().unwrap().remove(key);
        }
        let before = old.clone();
        let projected = project(&old).unwrap();
        assert!(projected.legacy);
        assert_eq!(projected.net.decimal(), "150.00");
        assert_eq!(projected.vat.decimal(), "37.50");
        assert_eq!(projected.gross.decimal(), "187.50");
        assert_eq!(old, before);
        old["totals"]
            .as_object_mut()
            .unwrap()
            .remove("vatAmountDkk");
        assert!(project(&old).is_err());
    }
    #[test]
    fn historical_ordinary_exempt_source_keeps_zero_vat_and_exact_aggregate() {
        let native = sales_invoice::compute(&serde_json::from_value(json!({"issueDate":"2025-08-01","currency":"EUR","fxRateToDkk":7.46,"lines":[{"description":"Synthetic exempt sale","quantity":1,"unitPriceExVat":100,"taxClassification":"exempt"}]})).unwrap()).unwrap();
        let old = without_native_allocation(native);
        let original = serde_json::to_vec(&old).unwrap();
        let projected = project(&old).unwrap();
        assert_eq!(projected.net.decimal(), "746.00");
        assert_eq!(projected.gross, projected.net);
        assert_eq!(projected.vat, Money::default());
        assert_eq!(projected.lines[0].vat_code, "DK_SALE_EXEMPT");
        assert_eq!(serde_json::to_vec(&old).unwrap(), original);
        let mut bad = old;
        bad["totals"]["vatAmountDkk"] = json!(1);
        assert!(project(&bad).is_err());
    }
    #[test]
    fn foreign_service_allocation_is_exact_and_tampering_refuses() {
        let input=serde_json::from_value(json!({"issueDate":"2025-08-01","currency":"EUR","fxRateToDkk":7.46,"vatTreatment":"foreign_reverse_charge","reverseChargeBasis":"EU_B2B_SERVICES","reverseChargeNote":"Reverse charge - customer accounts for VAT","lines":[{"description":"Synthetic service A","quantity":1,"unitPriceExVat":0.01},{"description":"Synthetic service B","quantity":1,"unitPriceExVat":0.01}]})).unwrap();
        let value = sales_invoice::compute(&input).unwrap();
        let original = sales_invoice::project(&value).unwrap();
        let dkk = project(&value).unwrap();
        assert_eq!(original.gross.decimal(), "0.02");
        assert_eq!(dkk.gross.decimal(), "0.15");
        assert_eq!(dkk.lines[0].vat_code, "REVERSE_CHARGE_EXEMPT");
        let mut sum = Money::default();
        for l in &dkk.lines {
            sum.add(&l.net);
        }
        assert_eq!(sum, dkk.gross);
        for path in ["grossAmountDkk", "netAmountDkk", "fxRateToDkk"] {
            let mut bad = value.clone();
            bad["totals"][path] = json!(1);
            assert!(project(&bad).is_err());
        }
        let mut bad = value;
        bad["lines"][0]["vatBaseDkk"] = json!(1);
        assert!(project(&bad).is_err());
    }
    #[test]
    fn historical_mixed_vat_residual_stays_on_taxable_counters() {
        let native = sales_invoice::compute(&serde_json::from_value(json!({"issueDate":"2025-08-01","currency":"EUR","fxRateToDkk":1.5,"lines":[
            {"description":"Synthetic taxable A","quantity":1,"unitPriceExVat":0.02},
            {"description":"Synthetic taxable B","quantity":1,"unitPriceExVat":0.02},
            {"description":"Synthetic exempt","quantity":1,"unitPriceExVat":0.02,"taxClassification":"exempt"}]})).unwrap()).unwrap();
        let source = without_native_allocation(native);
        let dkk = project(&source).unwrap();
        assert_eq!(dkk.vat.decimal(), "0.03");
        assert_eq!(dkk.lines[0].vat.decimal(), "0.02");
        assert_eq!(dkk.lines[1].vat.decimal(), "0.01");
        assert_eq!(dkk.lines[2].vat, Money::default());
        assert_eq!(dkk.lines[2].vat_code, "DK_SALE_EXEMPT");
    }
    #[test]
    fn rate_requires_explicit_exact_positive_quote() {
        for (c, n) in [
            ("EUR", None),
            ("DKK", Some(json!(2))),
            ("EUR", Some(json!(0))),
            ("EUR", Some(json!(7.4600001))),
        ] {
            assert!(rate(c, n.as_ref().and_then(Value::as_number)).is_err());
        }
        assert!(rate("EUR", Some(json!(7.46).as_number().unwrap())).is_ok());
    }
}
