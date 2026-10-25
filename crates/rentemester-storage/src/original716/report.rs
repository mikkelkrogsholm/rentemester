//! Source report metrics from a single admitted read transaction.
use rentemester_engine::Error;
use rusqlite::Transaction;
use serde_json::{Value, json};
pub fn read(tx: &Transaction<'_>, slug: &str, date: &str) -> Result<Value, Error> {
    let mut data = crate::compliance::read(tx, slug, date)?;
    // Source verifyAuditChain checks ledger integrity, not unobserved physical originals.
    let ledger = crate::audit_ledger::verify(tx)?;
    data["audit"] =
        json!({"ok":ledger["ok"],"entries":ledger["checked"]["entries"],"errors":ledger["errors"]});
    let input = serde_json::from_value(json!({"asOf":date})).map_err(crate::invalid)?;
    // Reuse the original read-only GDPR event projection, not an audited subject export.
    data["gdprAudit"] = crate::gdpr::original717::audit_events(tx, &input, date)?;
    let periods = data["periods"]["periods"]
        .as_array()
        .ok_or_else(|| Error::new("report_invalid", "Periods unavailable"))?;
    let closed: Vec<_> = periods
        .iter()
        .filter(|p| p["rowStatus"] == "closed")
        .collect();
    let latest = closed.iter().max_by_key(|p| {
        (
            p["periodEnd"].as_str().unwrap_or(""),
            p["id"].as_i64().unwrap_or(0),
        )
    });
    let label = latest.map(|p| {
        format!(
            "{} {} → {}",
            p["kind"].as_str().unwrap_or(""),
            p["periodStart"].as_str().unwrap_or(""),
            p["periodEnd"].as_str().unwrap_or("")
        )
    });
    let closed_count = closed.len();
    data["sourcePeriods"] = json!({"closedCount":closed_count,"lastClosedLabel":label});
    Ok(data)
}
