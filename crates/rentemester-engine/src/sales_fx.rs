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
