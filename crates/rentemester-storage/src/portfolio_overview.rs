//! Original per-company portfolio overview, projected from one admitted dashboard.
//! No company discovery, financial arithmetic or cross-company aggregation.
use rentemester_engine::Error;
use serde_json::{Value, json};
fn invalid() -> Error {
    Error::new(
        "portfolio_overview_source_unavailable",
        "Dashboard projection is incomplete",
    )
}
/// Complete the current ledger integrity barrier before any row refusal is allowed.
pub fn require_integrity(tx: &rusqlite::Transaction<'_>) -> Result<(), Error> {
    crate::journal_preview::require_current(tx)?;
    if !crate::journal_hash::verify(tx)?.ok {
        return Err(Error::new(
            "dashboard_source_unavailable",
            "Current journal hash integrity differs",
        ));
    }
    Ok(())
}
pub fn row(dashboard: &Value) -> Result<Value, Error> {
    let d = dashboard;
    let slug = d["slug"].as_str().ok_or_else(invalid)?;
    let name = d["company"]["name"].as_str().ok_or_else(invalid)?;
    let vat = &d["vat"];
    let invoices = &d["invoices"];
    let backup = &d["backup"];
    let audit = &d["audit"];
    let errors = vat["errors"].as_array().ok_or_else(invalid)?;
    let exceptions = d["exceptions"]["rows"].as_array().ok_or_else(invalid)?;
    for object in [vat, invoices, backup, audit] {
        if !object.is_object() {
            return Err(invalid());
        }
    }
    if !vat["available"].is_boolean()
        || !vat["registered"].is_boolean()
        || !backup["available"].is_boolean()
        || !audit["ok"].is_boolean()
    {
        return Err(invalid());
    }
    let registered = vat["registered"] == true;
    let can_compute = registered && vat["available"] == true && errors.is_empty();
    let period_field = |key: &str| {
        if registered {
            vat[key].clone()
        } else {
            Value::Null
        }
    };
    Ok(
        json!({"slug":slug,"name":name,"cvr":d["company"]["cvr"],"ok":true,
        "vat":{"vatRegistered":vat["registered"],"periodStart":period_field("periodStart"),"periodEnd":period_field("periodEnd"),"daysUntilDue":period_field("daysRemaining"),"netVatPayable":if can_compute {vat["netVatPayable"].clone()}else{Value::Null},"canCompute":can_compute,"periodStatus":period_field("periodStatus"),"deadline":period_field("deadline"),"errors":errors,"basis":vat["basis"]},
        "openReceivables":{"count":invoices["count"],"totalOpenBalance":invoices["openTotal"],"overdueCount":d["overdueInvoices"]["count"],"currency":"DKK","basis":{"principalAsOf":d["asOf"],"available":invoices["available"],"source":invoices["basis"],"reason":invoices["reason"]}},
        "backup":{"backupDue":backup["backupDue"],"daysSinceLatestBackup":backup["daysSinceLatestBackup"],"hasActivitySinceBackup":backup["hasActivitySinceBackup"],"backupsFound":backup["backupsFound"],"available":backup["available"],"basis":backup["basis"],"scope":backup["scope"],"errors":backup["errors"],"requiredPermission":backup["requiredPermission"]},
        "audit":{"ok":audit["ok"],"entries":audit["entryCount"],"firstError":audit["firstError"],"scope":audit["scope"]},
        "openExceptions":{"count":d["exceptions"]["count"],"highSeverityCount":exceptions.iter().filter(|r|r["severity"]=="high").count(),"basis":"current_state"},"basis":d["basis"]}),
    )
}
#[cfg(test)]
mod tests;
