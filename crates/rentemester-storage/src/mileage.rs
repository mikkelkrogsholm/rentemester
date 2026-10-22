//! Append-only mileage documentation. Never writes journal entries or tax treatment.
//! Every Err requires the owning transaction to roll back.
use crate::{idempotency::PrincipalKind, invalid, legacy_money};
use rentemester_engine::{
    Error,
    fiscal_year::{self, Date, LabelStrategy, StartMonth},
    mileage,
    money::Money,
    sales_invoice::{refused, wire},
};
use rusqlite::{Transaction, params};
use serde::Deserialize;
use serde_json::{Number, Value, json};
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Register {
    pub trip_date: String,
    pub purpose: String,
    pub from_location: String,
    pub to_location: String,
    pub kilometers: Number,
    pub vehicle: String,
    pub driver: String,
    pub rate_per_km: Number,
    pub rate_basis: String,
    pub rate_source: Option<String>,
    pub notes: Option<String>,
    pub confirm: bool,
}
fn optional(s: &Option<String>) -> Option<&str> {
    s.as_deref().map(str::trim).filter(|v| !v.is_empty())
}
pub fn register(tx: &Transaction<'_>, input: &Register, actor: &str) -> Result<Value, Error> {
    register_with_principal(tx, input, actor, PrincipalKind::User)
}
/// Shared insertion path; identity kind is supplied by current authenticated admission.
pub fn register_with_principal(
    tx: &Transaction<'_>,
    input: &Register,
    actor: &str,
    kind: PrincipalKind,
) -> Result<Value, Error> {
    crate::journal_post::require_current(tx)?;
    if !input.confirm
        || [
            &input.purpose,
            &input.from_location,
            &input.to_location,
            &input.vehicle,
            &input.driver,
            &input.rate_basis,
        ]
        .iter()
        .any(|s| s.trim().is_empty() || s.len() > 4096)
        || [&input.rate_source, &input.notes]
            .iter()
            .any(|s| s.as_ref().is_some_and(|v| v.len() > 4096))
    {
        return Err(refused(
            "trip purpose, route, vehicle, driver, rate basis and confirmation required",
        ));
    }
    let date = input.trip_date.trim();
    Date::parse(date)?;
    let rate = input.rate_per_km.to_string();
    let (km, amount) = mileage::calculate(&input.kilometers.to_string(), &rate)?;
    // Legacy NUMERIC storage must retain the decimal rate used in the calculation.
    let persisted_rate = input
        .rate_per_km
        .as_f64()
        .filter(|v| v.is_finite() && *v > 0.0)
        .ok_or_else(|| refused("rate cannot be persisted"))?;
    let rate_back = ryu_js::Buffer::new()
        .format_finite(persisted_rate)
        .to_owned();
    if !mileage::same_rate(&rate, &rate_back)? {
        return Err(refused("rate loses precision in legacy numeric storage"));
    }
    let km_number = crate::journal_post::money(&km)?;
    let amount_number = crate::journal_post::money(&amount)?;
    let (company_id, cvr): (i64, Option<String>) = tx
        .query_row("SELECT id,cvr FROM companies WHERE id=1", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .map_err(invalid)?;
    let year = &date[..4];
    let scope = crate::sequences::company_scope(cvr.as_deref(), company_id, year);
    let floor:i64=tx.query_row("SELECT COALESCE(MAX(CAST(substr(entry_no,-6) AS INTEGER)),0) FROM mileage_entries WHERE entry_no GLOB ?1",[format!("MIL-{year}-[0-9][0-9][0-9][0-9][0-9][0-9]")],|r|r.get(0)).map_err(invalid)?;
    let n = crate::sequences::next(tx, "mileage_entry", &scope, floor)?;
    if n > 999999 {
        return Err(refused("mileage entry sequence exhausted"));
    }
    let entry_no = format!("MIL-{year}-{n:06}");

    let id:i64=tx.query_row("INSERT INTO mileage_entries(entry_no,trip_date,purpose,from_location,to_location,kilometers,vehicle,driver,rate_per_km,amount_basis,rate_basis,rate_source,notes) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13) RETURNING id",params![entry_no,date,input.purpose.trim(),input.from_location.trim(),input.to_location.trim(),km_number,input.vehicle.trim(),input.driver.trim(),persisted_rate,amount_number,input.rate_basis.trim(),optional(&input.rate_source),optional(&input.notes)],|r|r.get(0)).map_err(invalid)?;
    tx.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor) VALUES('mileage_entry_create','mileage_entry',?1,?2,?3)",params![id.to_string(),format!("Recorded mileage entry {entry_no} ({date}, {} km, {} -> {})",km.decimal(),input.from_location.trim(),input.to_location.trim()),format!("{}:{actor}", match kind { PrincipalKind::User => "user", PrincipalKind::ServiceAccount => "service_account" })]).map_err(invalid)?;
    Ok(
        json!({"mileageEntryId":id,"entryNo":entry_no,"amountBasis":wire(&amount)?,"warnings":mileage::warnings(&rate)?,"appliedRules":[mileage::RULE]}),
    )
}
pub fn read(
    tx: &Transaction<'_>,
    slug: &str,
    today: &str,
    selected: Option<&str>,
) -> Result<Value, Error> {
    let company = crate::company_setup::profile(tx)?;
    let start = StartMonth::new(
        company["fiscalYearStartMonth"]
            .as_i64()
            .ok_or_else(|| refused("invalid fiscal calendar"))? as i32,
    )?;
    let strategy = LabelStrategy::try_from(
        company["fiscalYearLabelStrategy"]
            .as_str()
            .ok_or_else(|| refused("invalid fiscal calendar"))?,
    )?;
    let current = fiscal_year::for_date(Date::parse(today)?, start, strategy)?;
    let current_label = current.display_label.clone();
    let mut years = std::collections::BTreeMap::new();
    years.insert(current_label.clone(), current);
    let mut q=tx.prepare("SELECT DISTINCT trip_date FROM mileage_entries UNION SELECT DISTINCT transaction_date FROM journal_entries").map_err(invalid)?;
    for date in q
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(invalid)?
    {
        let y = fiscal_year::for_date(Date::parse(&date.map_err(invalid)?)?, start, strategy)?;
        years.insert(y.display_label.clone(), y);
    }
    let label = selected.unwrap_or(&current_label);
    let year = years
        .get(label)
        .ok_or_else(|| Error::new("invalid_year", "unknown live fiscal year"))?;
    let from = year.start.to_string();
    let to = year.end.to_string();
    let mut q=tx.prepare("SELECT id,entry_no,trip_date,purpose,from_location,to_location,kilometers,vehicle,driver,rate_per_km,amount_basis,rate_basis,rate_source,notes,created_at FROM mileage_entries WHERE trip_date BETWEEN ?1 AND ?2 ORDER BY trip_date DESC,id DESC").map_err(invalid)?;
    let mut rs = q.query(params![from, to]).map_err(invalid)?;
    let mut entries = vec![];
    let mut total_km = Money::default();
    let mut total_amount = Money::default();
    let mut month_totals = vec![(0usize, Money::default(), Money::default()); 12];
    while let Some(r) = rs.next().map_err(invalid)? {
        let date: String = r.get(2).map_err(invalid)?;
        let km = legacy_money(r.get_ref(6).map_err(invalid)?)?;
        let amount = legacy_money(r.get_ref(10).map_err(invalid)?)?;
        let month: usize = date[5..7].parse().map_err(invalid)?;
        let m = &mut month_totals[month - 1];
        m.0 += 1;
        m.1.add(&km);
        m.2.add(&amount);
        total_km.add(&km);
        total_amount.add(&amount);
        entries.push(json!({"id":r.get::<_,i64>(0).map_err(invalid)?,"entryNo":r.get::<_,String>(1).map_err(invalid)?,"tripDate":date,"purpose":r.get::<_,String>(3).map_err(invalid)?,"fromLocation":r.get::<_,String>(4).map_err(invalid)?,"toLocation":r.get::<_,String>(5).map_err(invalid)?,"kilometers":wire(&km)?,"vehicle":r.get::<_,String>(7).map_err(invalid)?,"driver":r.get::<_,String>(8).map_err(invalid)?,"ratePerKm":r.get::<_,f64>(9).map_err(invalid)?,"amountBasis":wire(&amount)?,"rateBasis":r.get::<_,String>(11).map_err(invalid)?,"rateSource":r.get::<_,Option<String>>(12).map_err(invalid)?,"notes":r.get::<_,Option<String>>(13).map_err(invalid)?,"createdAt":r.get::<_,String>(14).map_err(invalid)?}));
    }
    let names = [
        "jan", "feb", "mar", "apr", "maj", "jun", "jul", "aug", "sep", "okt", "nov", "dec",
    ];
    let first: usize = from[5..7].parse::<usize>().map_err(invalid)? - 1;
    let months=(0..12).map(|offset|{let i=(first+offset)%12;let (count,km,amount)=&month_totals[i];Ok(json!({"month":i+1,"label":names[i],"tripCount":count,"kilometers":wire(km)?,"amountBasis":wire(amount)?}))}).collect::<Result<Vec<_>,Error>>()?;
    let fiscal_years:Vec<_>=years.values().map(|y|json!({"label":y.display_label,"start":y.start.to_string(),"end":y.end.to_string(),"source":"live"})).collect();
    Ok(
        json!({"slug":slug,"company":company,"selectedYear":label,"archived":false,"periodStart":from,"periodEnd":to,"fiscalYears":fiscal_years,"entries":entries,"tripCount":entries.len(),"totalKilometers":wire(&total_km)?,"totalAmountBasis":wire(&total_amount)?,"months":months}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn input(date: &str, km: Value, rate: Value) -> Register {
        serde_json::from_value(json!({"tripDate":date,"purpose":"Synthetic customer visit","fromLocation":"Synthetic origin","toLocation":"Synthetic destination","kilometers":km,"vehicle":"Synthetic private vehicle","driver":"Synthetic driver","ratePerKm":rate,"rateBasis":"Synthetic user-confirmed basis","rateSource":"https://example.invalid/rate","notes":"Synthetic audit note","confirm":true})).unwrap()
    }
    #[test]
    fn append_only_sequences_exact_totals_fiscal_boundaries_and_restart() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("company.sqlite");
        let mut db = rusqlite::Connection::open(&file).unwrap();
        let mut control = rusqlite::Connection::open_in_memory().unwrap();
        crate::fresh::initialize_empty_pair(&mut control, &mut db, "Synthetic Mileage").unwrap();
        db.execute("UPDATE companies SET fiscal_year_start_month=7,fiscal_year_label_strategy='span' WHERE id=1",[]).unwrap();
        let tx = db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        let first = register(
            &tx,
            &input("2026-07-01", json!(12.345), json!(3.791)),
            "synthetic-author",
        )
        .unwrap();
        assert_eq!(first["entryNo"], "MIL-2026-000001");
        assert_eq!(first["amountBasis"], 46.82);
        assert_eq!(
            register(
                &tx,
                &input("2027-06-30", json!(1), json!(1.005)),
                "synthetic-author"
            )
            .unwrap()["entryNo"],
            "MIL-2027-000001"
        );
        register(
            &tx,
            &input("2026-06-30", json!(10), json!(2)),
            "synthetic-author",
        )
        .unwrap();
        register(
            &tx,
            &input("2027-07-01", json!(20), json!(2)),
            "synthetic-author",
        )
        .unwrap();
        let listed = read(&tx, "synthetic", "2026-10-19", Some("2026/27")).unwrap();
        assert_eq!(listed["periodStart"], "2026-07-01");
        assert_eq!(listed["periodEnd"], "2027-06-30");
        assert_eq!(listed["tripCount"], 2);
        assert_eq!(listed["totalKilometers"], 13.35);
        assert_eq!(listed["totalAmountBasis"], 47.83);
        assert_eq!(listed["months"][0]["month"], 7);
        assert_eq!(listed["months"][11]["month"], 6);
        assert_eq!(listed["months"][0]["amountBasis"], 46.82);
        assert_eq!(listed["months"][11]["amountBasis"], 1.01);
        assert_eq!(listed["entries"][0]["tripDate"], "2027-06-30");
        assert!(read(&tx, "synthetic", "2026-10-19", Some("missing")).is_err());
        tx.commit().unwrap();
        drop(db);
        let mut db = rusqlite::Connection::open(&file).unwrap();
        let tx = db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        assert_eq!(
            read(&tx, "synthetic", "2026-10-19", Some("2026/27")).unwrap(),
            listed
        );
        assert_eq!(
            register(
                &tx,
                &input("2026-10-19", json!(1), json!(2)),
                "synthetic-author"
            )
            .unwrap()["entryNo"],
            "MIL-2026-000003"
        );
        assert_eq!(
            tx.query_row("SELECT count(*) FROM journal_entries", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert_eq!(tx.query_row("SELECT count(*) FROM audit_log WHERE event_type='mileage_entry_create' AND actor='user:synthetic-author'",[],|r|r.get::<_,i64>(0)).unwrap(),5);
        for sql in [
            "UPDATE mileage_entries SET purpose='tampered'",
            "DELETE FROM mileage_entries",
            "UPDATE audit_log SET message='tampered'",
            "DELETE FROM audit_log",
        ] {
            assert!(tx.execute(sql, []).is_err(), "{sql}");
        }
        tx.rollback().unwrap();
    }
    #[test]
    fn invalid_inputs_do_not_allocate_or_write_and_rollback_restores_sequence() {
        let mut db = rusqlite::Connection::open_in_memory().unwrap();
        let mut control = rusqlite::Connection::open_in_memory().unwrap();
        crate::fresh::initialize_empty_pair(&mut control, &mut db, "Synthetic Mileage").unwrap();
        let tx = db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        for (date, km, rate) in [
            ("2026-02-30", json!(1), json!(2)),
            ("2026-01-01", json!(0.001), json!(2)),
            ("2026-01-01", json!(-1), json!(2)),
            ("2026-01-01", json!(1), json!(0)),
        ] {
            assert!(register(&tx, &input(date, km, rate), "synthetic").is_err());
        }
        let mut i = input("2026-01-01", json!(1), json!(2));
        i.confirm = false;
        assert!(register(&tx, &i, "synthetic").is_err());
        i.confirm = true;
        i.rate_basis = " ".into();
        assert!(register(&tx, &i, "synthetic").is_err());
        assert_eq!(
            tx.query_row("SELECT count(*) FROM mileage_entries", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert_eq!(
            tx.query_row(
                "SELECT count(*) FROM sequences WHERE kind='mileage_entry'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
        register(&tx, &input("2026-01-01", json!(1), json!(9)), "synthetic").unwrap();
        tx.rollback().unwrap();
        let tx = db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        let created = register(&tx, &input("2026-01-01", json!(1), json!(9)), "synthetic").unwrap();
        assert_eq!(created["entryNo"], "MIL-2026-000001");
        assert_eq!(created["warnings"].as_array().unwrap().len(), 1);
        assert_eq!(created["amountBasis"], 9.0);
    }
}
