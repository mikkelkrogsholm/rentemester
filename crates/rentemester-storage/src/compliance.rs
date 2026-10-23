//! Read-only company facts; no GDPR export or audit-event insertion.
use rentemester_engine::{
    Error,
    fiscal_year::{self, Date, LabelStrategy, StartMonth},
};
use rusqlite::Transaction;
use serde_json::{Value, json};
pub fn read(tx: &Transaction<'_>, slug: &str, as_of: &str) -> Result<Value, Error> {
    let date = Date::parse(as_of)?;
    let retention = crate::retention::read(tx, slug, as_of)?;
    let profile = &retention["company"];
    let year = fiscal_year::for_date(
        date,
        StartMonth::new(
            profile["fiscalYearStartMonth"]
                .as_i64()
                .ok_or_else(|| Error::new("compliance_unavailable", "fiscal start unavailable"))?
                as i32,
        )?,
        LabelStrategy::try_from(profile["fiscalYearLabelStrategy"].as_str().unwrap_or(""))?,
    )?;
    let periods = crate::periods::list(tx, slug)?;
    let audit = crate::audit_verify::report(
        tx,
        slug,
        Err(Error::new(
            "originals_not_observed",
            "Report does not read original files; full native audit verify is required for physical evidence",
        )),
    )?;
    let count: i64 = tx
        .query_row(
            "SELECT count(*) FROM audit_log WHERE substr(event_type,1,5)='gdpr_'",
            [],
            |r| r.get(0),
        )
        .map_err(crate::invalid)?;
    Ok(
        json!({"format":"rentemester-company-compliance/1","slug":slug,"asOf":as_of,"stateNotice":"Current company state; asOf selects fiscal/retention calculations, not historical database reconstruction.","company":{"name":profile["name"],"cvr":profile["cvr"],"country":profile["country"],"currency":profile["currency"]},"fiscalYear":year,"audit":audit["audit"],"periods":periods,"retention":retention["report"],"gdpr":{"eventCount":count,"scope":"Literal gdpr_ event count only; no subject details, export, signature or audit mutation."},"backup":{"available":false,"detailsAvailable":false,"reason":"Backup governance details require separate CompanyAdmin authorization and retained artifact/configuration observation; not collected by this read-only CompanyRead report.","remoteSyncVerified":false},"coverage":rentemester_engine::regulatory::bundled_coverage()?}),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn read_only_actual_company_and_literal_events() {
        let mut c = rusqlite::Connection::open_in_memory().unwrap();
        let mut l = rusqlite::Connection::open_in_memory().unwrap();
        crate::fresh::initialize_empty_pair(&mut c, &mut l, "Synthetic Compliance").unwrap();
        l.execute_batch("INSERT INTO audit_log(event_type,entity_type,entity_id,actor,message)VALUES('gdpr_export','customer','1','user:synthetic','Synthetic event'),('gdprXexport','customer','1','user:synthetic','Synthetic unrelated event');").unwrap();
        let before = l.total_changes();
        let tx = l.transaction().unwrap();
        let data = read(&tx, "synthetic", "2026-01-01").unwrap();
        assert_eq!(data["company"]["name"], "Synthetic Compliance");
        assert_eq!(data["gdpr"]["eventCount"], 1);
        assert_eq!(data["audit"]["available"], false);
        assert_eq!(data["backup"]["available"], false);
        assert!(read(&tx, "synthetic", "2026-02-30").is_err());
        tx.commit().unwrap();
        assert_eq!(l.total_changes(), before);
    }
}
