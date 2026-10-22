//! Original mileage projections over the shared native writer. Caller owns rollback.
//! Source correction: positive distances rounding to zero remain refused; no ledger effects.
use crate::{idempotency::PrincipalKind, invalid, legacy_money, mileage};
use rentemester_engine::{
    Error, fiscal_year::Date, mileage::RULE, money::Money, sales_invoice::wire,
};
use rusqlite::{Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::{Number, Value, json};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Log {
    pub trip_date: String,
    pub purpose: String,
    pub from_location: String,
    pub to_location: String,
    pub kilometers: Option<Number>,
    pub vehicle: String,
    pub driver: String,
    pub rate_per_km: Option<Number>,
    pub rate_basis: String,
    pub rate_source: Option<String>,
    pub notes: Option<String>,
}
fn positive(n: &Option<Number>) -> bool {
    n.as_ref()
        .and_then(Number::as_f64)
        .is_some_and(|n| n.is_finite() && n > 0.0)
}
pub fn log(
    tx: &Transaction<'_>,
    input: &Log,
    subject: &str,
    kind: PrincipalKind,
) -> Result<Value, Error> {
    let mut errors = Vec::new();
    if input.trip_date.trim().is_empty() {
        errors.push("tripDate is required");
    } else if Date::parse(input.trip_date.trim()).is_err() {
        errors.push("tripDate must be a valid YYYY-MM-DD date");
    }
    for (text, error) in [
        (&input.purpose, "purpose is required"),
        (&input.from_location, "fromLocation is required"),
        (&input.to_location, "toLocation is required"),
    ] {
        if text.trim().is_empty() {
            errors.push(error);
        }
    }
    if !positive(&input.kilometers) {
        errors.push("kilometers must be a positive number");
    }
    if input.vehicle.trim().is_empty() {
        errors.push("vehicle is required");
    }
    if input.driver.trim().is_empty() {
        errors.push("driver is required");
    }
    if !positive(&input.rate_per_km) {
        errors.push("ratePerKm must be a positive number");
    }
    if input.rate_basis.trim().is_empty() {
        errors.push("rateBasis is required (user-supplied / source-backed)");
    }
    if !errors.is_empty() {
        return Ok(json!({"ok":false,"appliedRules":[RULE],"warnings":[],"errors":errors}));
    }
    let native = mileage::Register {
        trip_date: input.trip_date.clone(),
        purpose: input.purpose.clone(),
        from_location: input.from_location.clone(),
        to_location: input.to_location.clone(),
        kilometers: input.kilometers.clone().unwrap(),
        vehicle: input.vehicle.clone(),
        driver: input.driver.clone(),
        rate_per_km: input.rate_per_km.clone().unwrap(),
        rate_basis: input.rate_basis.clone(),
        rate_source: input.rate_source.clone(),
        notes: input.notes.clone(),
        confirm: true,
    };
    let mut result = mileage::register_with_principal(tx, &native, subject, kind)?;
    // Preserve native advisory unchanged, adapting only its displayed rate to JS da-DK.
    let rate = native.rate_per_km.as_f64().unwrap();
    if rate > 3.94 {
        let display = danish_rate(rate)?;
        let warnings = rentemester_engine::mileage::warnings(&native.rate_per_km.to_string())?;
        result["warnings"] = json!(
            warnings
                .into_iter()
                .map(|w| w.replacen(
                    &format!("sats {} kr/km", native.rate_per_km),
                    &format!("sats {display} kr/km"),
                    1
                ))
                .collect::<Vec<_>>()
        );
    }
    result["ok"] = json!(true);
    result["errors"] = json!([]);
    Ok(result)
}
fn danish_rate(rate: f64) -> Result<String, Error> {
    // Decimal half-up at the source locale's default maximum three fraction digits.
    let rounded = Money::multiply_decimal(ryu_js::Buffer::new().format_finite(rate), "10")?;
    let digits = rounded.decimal().replace('.', "");
    let padded = format!("{digits:0>4}");
    let (whole, frac) = padded.split_at(padded.len() - 3);
    let mut grouped = String::new();
    for (i, c) in whole.chars().enumerate() {
        if i > 0 && (whole.len() - i).is_multiple_of(3) {
            grouped.push('.');
        }
        grouped.push(c);
    }
    let frac = frac.trim_end_matches('0');
    if !frac.is_empty() {
        grouped.push(',');
        grouped.push_str(frac);
    }
    Ok(grouped)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Row {
    id: i64,
    entry_no: String,
    trip_date: String,
    purpose: String,
    from_location: String,
    to_location: String,
    #[serde(serialize_with = "js_number")]
    kilometers: f64,
    vehicle: String,
    driver: String,
    #[serde(serialize_with = "js_number")]
    rate_per_km: f64,
    #[serde(serialize_with = "js_number")]
    amount_basis: f64,
    rate_basis: String,
    rate_source: Option<String>,
    notes: Option<String>,
    created_at: String,
}
const COLUMNS: &str = "id,entry_no,trip_date,purpose,from_location,to_location,kilometers,vehicle,driver,rate_per_km,amount_basis,rate_basis,rate_source,notes,created_at";
fn rows(
    tx: &Transaction<'_>,
    range: Option<(&str, &str)>,
) -> Result<(Vec<Row>, Money, Money), Error> {
    let suffix = if range.is_some() {
        "WHERE trip_date BETWEEN ?1 AND ?2 ORDER BY trip_date ASC,id ASC"
    } else {
        "ORDER BY trip_date DESC,id DESC"
    };
    let mut q = tx
        .prepare(&format!("SELECT {COLUMNS} FROM mileage_entries {suffix}"))
        .map_err(invalid)?;
    let mut rs = match range {
        Some((from, to)) => q.query(params![from, to]),
        None => q.query([]),
    }
    .map_err(invalid)?;
    let mut entries = Vec::new();
    let mut km = Money::default();
    let mut amount = Money::default();
    while let Some(r) = rs.next().map_err(invalid)? {
        let k = legacy_money(r.get_ref(6).map_err(invalid)?)?;
        let a = legacy_money(r.get_ref(10).map_err(invalid)?)?;
        km.add(&k);
        amount.add(&a);
        let rate: f64 = r.get(9).map_err(invalid)?;
        if !rate.is_finite() || rate <= 0.0 {
            return Err(invalid("invalid persisted mileage rate"));
        }
        entries.push(Row {
            id: crate::safe_entry_id(r.get(0).map_err(invalid)?)?,
            entry_no: r.get(1).map_err(invalid)?,
            trip_date: r.get(2).map_err(invalid)?,
            purpose: r.get(3).map_err(invalid)?,
            from_location: r.get(4).map_err(invalid)?,
            to_location: r.get(5).map_err(invalid)?,
            kilometers: crate::journal_post::money(&k)?,
            vehicle: r.get(7).map_err(invalid)?,
            driver: r.get(8).map_err(invalid)?,
            rate_per_km: rate,
            amount_basis: crate::journal_post::money(&a)?,
            rate_basis: r.get(11).map_err(invalid)?,
            rate_source: r.get(12).map_err(invalid)?,
            notes: r.get(13).map_err(invalid)?,
            created_at: r.get(14).map_err(invalid)?,
        });
    }
    Ok((entries, km, amount))
}
pub fn list(tx: &Transaction<'_>) -> Result<Value, Error> {
    let (rows, _, _) = rows(tx, None)?;
    Ok(json!({"ok":true,"count":rows.len(),"rows":rows,"appliedRules":[RULE],"errors":[]}))
}
fn range_errors(from: &str, to: &str) -> Vec<&'static str> {
    let mut errors = vec![];
    if Date::parse(from).is_err() {
        errors.push("from must be a valid YYYY-MM-DD date");
    }
    if Date::parse(to).is_err() {
        errors.push("to must be a valid YYYY-MM-DD date");
    }
    if errors.is_empty() && from > to {
        errors.push("from cannot be after to");
    }
    errors
}
pub fn report(tx: &Transaction<'_>, from: &str, to: &str) -> Result<Value, Error> {
    let (from, to) = (from.trim(), to.trim());
    let errors = range_errors(from, to);
    if !errors.is_empty() {
        return Ok(
            json!({"ok":false,"from":from,"to":to,"entryCount":0,"totalKilometers":0,"totalAmountBasis":0,"entries":[],"appliedRules":[RULE],"errors":errors}),
        );
    }
    let (entries, km, amount) = rows(tx, Some((from, to)))?;
    Ok(
        json!({"ok":true,"from":from,"to":to,"entryCount":entries.len(),"totalKilometers":wire(&km)?,"totalAmountBasis":wire(&amount)?,"entries":entries,"appliedRules":[RULE],"errors":[]}),
    )
}
pub enum ExportOutcome {
    Refused(Value),
    Ready(ExportArtifacts),
}
pub struct ExportArtifacts {
    pub summary: Value,
    pub json_bytes: Vec<u8>,
    pub csv_bytes: Vec<u8>,
}
const NOTE: &str = "Mileage log only. Rentemester records the trips; tax treatment is the user/advisor's responsibility.";
const HEADER: &str = "entry_no,trip_date,purpose,from_location,to_location,kilometers,vehicle,driver,rate_per_km,amount_basis,rate_basis,rate_source";
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Artifact<'a> {
    applied_rules: [&'a str; 1],
    from: &'a str,
    to: &'a str,
    entry_count: usize,
    #[serde(serialize_with = "js_number")]
    total_kilometers: f64,
    #[serde(serialize_with = "js_number")]
    total_amount_basis: f64,
    note: &'a str,
    entries: &'a [Row],
}
fn cell(text: &str) -> String {
    if text.contains([',', '"', '\r', '\n']) {
        format!("\"{}\"", text.replace('"', "\"\""))
    } else {
        text.into()
    }
}
fn number(n: f64) -> String {
    ryu_js::Buffer::new().format_finite(n).into()
}
fn artifacts(
    from: &str,
    to: &str,
    entries: &[Row],
    km: &Money,
    amount: &Money,
) -> Result<(Vec<u8>, Vec<u8>), Error> {
    let payload = Artifact {
        applied_rules: [RULE],
        from,
        to,
        entry_count: entries.len(),
        total_kilometers: crate::journal_post::money(km)?,
        total_amount_basis: crate::journal_post::money(amount)?,
        note: NOTE,
        entries,
    };
    let mut json_bytes = Vec::new();
    let mut serializer = serde_json::Serializer::with_formatter(
        &mut json_bytes,
        serde_json::ser::PrettyFormatter::new(),
    );
    payload.serialize(&mut serializer).map_err(invalid)?;
    json_bytes.push(b'\n');
    let mut csv = format!("{HEADER}\n");
    for e in entries {
        csv.push_str(
            &[
                e.entry_no.clone(),
                e.trip_date.clone(),
                e.purpose.clone(),
                e.from_location.clone(),
                e.to_location.clone(),
                number(e.kilometers),
                e.vehicle.clone(),
                e.driver.clone(),
                number(e.rate_per_km),
                number(e.amount_basis),
                e.rate_basis.clone(),
                e.rate_source.clone().unwrap_or_default(),
            ]
            .iter()
            .map(|s| cell(s))
            .collect::<Vec<_>>()
            .join(","),
        );
        csv.push('\n');
    }
    Ok((json_bytes, csv.into_bytes()))
}

fn js_number<S: serde::Serializer>(n: &f64, s: S) -> Result<S::Ok, S::Error> {
    let value: Number = number(*n).parse().map_err(serde::ser::Error::custom)?;
    value.serialize(s)
}
/// The bounded HTTP packet is measured before recording prepared-delivery audit.
pub fn export(
    tx: &Transaction<'_>,
    from: &str,
    to: &str,
    subject: &str,
    kind: PrincipalKind,
    today: Date,
) -> Result<ExportOutcome, Error> {
    let (from, to) = (from.trim(), to.trim());
    let errors = range_errors(from, to);
    if !errors.is_empty() {
        return Ok(ExportOutcome::Refused(
            json!({"ok":false,"appliedRules":[RULE],"errors":errors}),
        ));
    }
    crate::journal_post::require_current(tx)?;
    let (entries, km, amount) = rows(tx, Some((from, to)))?;
    let (json_bytes, csv_bytes) = artifacts(from, to, &entries, &km, &amount)?;
    let summary = json!({"ok":true,"from":from,"to":to,"entryCount":entries.len(),"totalKilometers":wire(&km)?,"totalAmountBasis":wire(&amount)?,"appliedRules":[RULE],"errors":[]});
    let mut packet = summary.clone();
    packet["artifacts"] = json!({"json":{"filename":"mileage-log.json","contentType":"application/json; charset=utf-8","bytes":json_bytes},"csv":{"filename":"mileage-log.csv","contentType":"text/csv; charset=utf-8","bytes":csv_bytes}});
    if serde_json::to_vec(&packet).map_err(invalid)?.len() > 16 * 1024 * 1024 {
        return Ok(ExportOutcome::Refused(
            json!({"ok":false,"appliedRules":[RULE],"errors":["mileage export exceeds the 16 MiB response limit"]}),
        ));
    }
    let prefix = match kind {
        PrincipalKind::User => "user",
        PrincipalKind::ServiceAccount => "service_account",
    };
    tx.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor) VALUES('mileage_log_export','mileage_log',NULL,?1,?2)",params![format!("Prepared mileage log export for {from}..{to} ({} entries, {today}); local publication is not attested",entries.len()),format!("{prefix}:{subject}")]).map_err(invalid)?;
    Ok(ExportOutcome::Ready(ExportArtifacts {
        summary,
        json_bytes,
        csv_bytes,
    }))
}
#[cfg(test)]
mod tests;
