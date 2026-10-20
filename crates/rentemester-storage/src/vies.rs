//! Audited authoritative VIES cache. Host owns provider I/O, current authorization and trusted clock.
use crate::invalid;
use rentemester_engine::{Error, vies};
use rusqlite::{OptionalExtension, Transaction, params};
use serde_json::{Value, json};
pub fn require(tx: &Transaction<'_>, vat: &str, now: &str) -> Result<Value, Error> {
    let parsed = vies::foreign(vat)?;
    let record:Option<Value>=tx.query_row("SELECT valid,name,address,validated_at,expires_at,raw_response FROM vies_validations WHERE country_code=?1 AND vat_number=?2",params![parsed.country_code,parsed.vat_number],|r|Ok(json!({"countryCode":parsed.country_code,"vatNumber":parsed.vat_number,"normalized":parsed.normalized,"valid":match r.get::<_,i64>(0)? {0=>Some(false),1=>Some(true),_=>None},"name":r.get::<_,Option<String>>(1)?,"address":r.get::<_,Option<String>>(2)?,"validatedAt":r.get::<_,String>(3)?,"expiresAt":r.get::<_,String>(4)?,"rawResponse":r.get::<_,Option<String>>(5)?}))).optional().map_err(invalid)?;
    vies::require_record(
        &record.ok_or_else(|| {
            vies::refused("VIES lookup required before issuing foreign EU invoice")
        })?,
        vat,
        now,
    )
}
pub fn store(tx: &Transaction<'_>, record: &Value, actor: &str) -> Result<Value, Error> {
    if actor.is_empty() || actor.len() > 256 || actor.chars().any(char::is_control) {
        return Err(vies::refused("VIES audit actor required"));
    }
    let value = vies::validate_record(record)?;
    tx.execute("INSERT INTO vies_validations(country_code,vat_number,valid,name,address,validated_at,expires_at,raw_response)VALUES(?1,?2,?3,?4,?5,?6,?7,?8) ON CONFLICT(country_code,vat_number) DO UPDATE SET valid=excluded.valid,name=excluded.name,address=excluded.address,validated_at=excluded.validated_at,expires_at=excluded.expires_at,raw_response=excluded.raw_response",params![value["countryCode"].as_str(),value["vatNumber"].as_str(),i64::from(value["valid"].as_bool().unwrap()),value["name"].as_str(),value["address"].as_str(),value["validatedAt"].as_str(),value["expiresAt"].as_str(),value["rawResponse"].as_str()]).map_err(invalid)?;
    tx.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor,created_at)VALUES('native_vies_validate','vies_validation',?1,?2,?3,?4)",params![value["normalized"].as_str(),serde_json::to_string(&value).map_err(invalid)?,actor,value["validatedAt"].as_str()]).map_err(invalid)?;
    Ok(value)
}
#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::{Connection, TransactionBehavior};
    #[test]
    fn observations_are_audited_and_negative_cache_never_authorizes() {
        let mut control = Connection::open_in_memory().unwrap();
        let mut db = Connection::open_in_memory().unwrap();
        crate::fresh::initialize_empty_pair(&mut control, &mut db, "Synthetic VIES").unwrap();
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        let mut observation = json!({"countryCode":"DE","vatNumber":"123456789","normalized":"DE123456789","valid":true,"name":null,"address":null,"rawResponse":"{}"});
        let record = vies::record(&observation, "2026-01-01T12:00:00Z").unwrap();
        store(&tx, &record, "user:synthetic").unwrap();
        assert!(require(&tx, "DE123456789", "2026-01-02T12:00:00Z").is_ok());
        assert!(require(&tx, "DE123456789", "2026-05-01T12:00:00Z").is_err());
        observation["valid"] = json!(false);
        store(
            &tx,
            &vies::record(&observation, "2026-01-02T12:00:00Z").unwrap(),
            "user:synthetic",
        )
        .unwrap();
        assert!(require(&tx, "DE123456789", "2026-01-02T12:00:00Z").is_err());
        assert_eq!(
            tx.query_row(
                "SELECT count(*) FROM audit_log WHERE event_type='native_vies_validate'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            2
        );
    }
}
