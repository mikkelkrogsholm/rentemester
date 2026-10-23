//! Source projection over the existing native source-bound commitment engine.
//! Native hashes/schedules intentionally differ from the pure TS proposal: active local
//! vendor, immutable document provenance, previous revision, and nextDate+90 inclusive.
use crate::{invalid, supplier_commitments as native};
use rentemester_engine::Error;
use rusqlite::Transaction;
use serde::{Deserialize, Serialize};
use serde_json::{Number, Value, json};
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Input {
    pub vendor_snapshot: Option<String>,
    pub status: Option<String>,
    pub commitment_id: String,
    pub vendor_party_id: String,
    pub r#type: String,
    pub description: String,
    pub business_purpose: String,
    pub amount: Number,
    pub currency: String,
    pub frequency: String,
    pub next_date: String,
    pub start_date: Option<String>,
    pub renewal_date: Option<String>,
    pub notice_date: Option<String>,
    pub end_date: Option<String>,
    pub evidence_refs: Vec<String>,
    pub vat_proposal: Option<String>,
}

impl Input {
    pub fn native(&self) -> Result<native::Input, Error> {
        if self.status.as_deref().is_some_and(|s| s != "active") {
            return Err(Error::new(
                "supplier_commitment_refused",
                "initial status must be active",
            ));
        }
        let mut v = serde_json::to_value(self).map_err(invalid)?;
        let m = v.as_object_mut().ok_or_else(|| invalid("input"))?;
        m.remove("vendorSnapshot");
        m.remove("status");
        serde_json::from_value(v).map_err(invalid)
    }
    pub fn assert_snapshot(&self, tx: &Transaction<'_>) -> Result<(), Error> {
        if let Some(expected) = &self.vendor_snapshot {
            let id = self
                .vendor_party_id
                .strip_prefix("vendor:")
                .and_then(|s| s.parse::<i64>().ok())
                .filter(|n| *n > 0)
                .ok_or_else(|| {
                    Error::new(
                        "supplier_commitment_refused",
                        "active local vendor required",
                    )
                })?;
            let current: String = tx
                .query_row(
                    "SELECT name FROM vendors WHERE id=?1 AND archived=0",
                    [id],
                    |r| r.get(0),
                )
                .map_err(invalid)?;
            if current != *expected {
                return Err(Error::new(
                    "supplier_commitment_refused",
                    "vendorSnapshot must equal current vendor name",
                ));
            }
        }
        Ok(())
    }
    pub fn proposal(&self, tx: &Transaction<'_>) -> Result<Value, Error> {
        self.assert_snapshot(tx)?;
        native::plan(tx, &self.native()?)
    }
}
pub fn list(tx: &Transaction<'_>) -> Result<Vec<Value>, Error> {
    let mut q = tx.prepare("SELECT commitment_id,payload_json,payload_hash,created_at FROM current_supplier_commitments ORDER BY commitment_id").map_err(invalid)?;
    q.query_map([], |r| Ok(json!({"commitment_id":r.get::<_,String>(0)?,"payload_json":r.get::<_,String>(1)?,"payload_hash":r.get::<_,String>(2)?,"created_at":r.get::<_,String>(3)?}))).map_err(invalid)?.collect::<Result<_,_>>().map_err(invalid)
}
pub fn matches(tx: &Transaction<'_>, id: Option<&str>) -> Result<Vec<Value>, Error> {
    Ok(native::matches(tx)?
        .into_iter()
        .filter(|r| id.is_none_or(|id| r["commitmentId"] == id))
        .collect())
}
pub fn alerts(tx: &Transaction<'_>, as_of: &str) -> Result<Vec<Value>, Error> {
    if !rentemester_engine::calendar::valid_iso_date(as_of) {
        return Ok(vec![]);
    }
    let end = rentemester_engine::vat_period::date(as_of)?
        .checked_add(time::Duration::days(30))
        .ok_or_else(|| invalid("date horizon"))?
        .to_string();
    let mut result = vec![];
    for row in list(tx)? {
        let p: Value = serde_json::from_str(
            row["payload_json"]
                .as_str()
                .ok_or_else(|| invalid("payload"))?,
        )
        .map_err(invalid)?;
        for (kind, key) in [("notice", "noticeDate"), ("renewal", "renewalDate")] {
            if let Some(date) = p[key].as_str()
                && date >= as_of
                && date <= end.as_str()
            {
                result.push(json!({"commitmentId":row["commitment_id"],"kind":kind,"date":date,"severity":if date==as_of {"due"}else{"upcoming"}}));
            }
        }
    }
    result.sort_by(|a, b| {
        a["date"]
            .as_str()
            .cmp(&b["date"].as_str())
            .then_with(|| a["commitmentId"].as_str().cmp(&b["commitmentId"].as_str()))
    });
    Ok(result)
}
