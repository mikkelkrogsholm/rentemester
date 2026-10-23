//! Original asset source DTOs. Server defaults and authority; no automatic write retries.
use crate::{
    ApiClient, Error, MAX_RESPONSE, company_path,
    domain::{body, object},
};
use reqwest::Method;
use serde_json::{Value, json};
const MAX_ID: i64 = 9_007_199_254_740_991;
const MAX_DKK: f64 = MAX_ID as f64 / 100.0;
const DEPR: &str = "DK-ASSET-DEPR-001";
const WRITEOFF: &str = "DK-ASSET-WRITEOFF-001";
fn id() -> Value {
    json!({"type":"integer","minimum":1,"maximum":MAX_ID})
}
fn count() -> Value {
    json!({"type":"integer","minimum":1,"maximum":12000})
}
fn text(max: usize) -> Value {
    json!({"type":"string","minLength":1,"maxLength":max})
}
fn date_schema() -> Value {
    json!({"type":"string","minLength":10,"maxLength":10})
}
fn money_schema() -> Value {
    json!({"type":"number","minimum":0,"maximum":MAX_DKK})
}
fn strings() -> Value {
    json!({"type":"array","maxItems":128,"items":text(512)})
}
/// Source field names, not native aliases; transport authority is separate.
pub fn original_asset_schema(operation: &str) -> Value {
    match operation {
        "register" => object(
            json!({"name":text(500),"category":text(500),"acquisitionDate":date_schema(),"cost":{"type":"number","exclusiveMinimum":0,"maximum":MAX_DKK},"usefulLifeMonths":count(),"documentId":id(),"assetAccount":text(128),"depreciationAccount":text(128),"accumulatedAccount":text(128),"note":{"type":"string","maxLength":4096}}),
            &[
                "name",
                "category",
                "acquisitionDate",
                "cost",
                "usefulLifeMonths",
                "documentId",
            ],
        ),
        "depreciate" => object(
            json!({"period":count(),"date":date_schema()}),
            &["period", "date"],
        ),
        "write-off" => object(
            json!({"name":text(500),"category":text(500),"acquisitionDate":date_schema(),"cost":{"type":"number","exclusiveMinimum":0,"maximum":MAX_DKK},"documentId":id(),"expenseAccount":text(128),"date":date_schema(),"thresholdRuleSource":text(1000),"confirmImmediateWriteOff":{"type":"boolean"},"paymentAccount":text(128),"note":{"type":"string","maxLength":4096}}),
            &[
                "name",
                "category",
                "acquisitionDate",
                "cost",
                "documentId",
                "expenseAccount",
                "date",
                "thresholdRuleSource",
                "confirmImmediateWriteOff",
            ],
        ),
        _ => object(json!({}), &[]),
    }
}
fn shape(v: &Value, s: &Value) -> bool {
    serde_json::to_vec(v).is_ok_and(|b| body(&b, s).is_ok())
}
fn date(v: &Value) -> bool {
    v.as_str().is_some_and(|s| {
        s.len() == 10
            && s.bytes().enumerate().all(|(i, b)| {
                if i == 4 || i == 7 {
                    b == b'-'
                } else {
                    b.is_ascii_digit()
                }
            })
            && s[..4]
                .parse::<i32>()
                .ok()
                .filter(|y| *y > 0)
                .zip(s[5..7].parse::<u8>().ok())
                .zip(s[8..].parse::<u8>().ok())
                .is_some_and(|((y, m), d)| {
                    time::Month::try_from(m)
                        .ok()
                        .is_some_and(|m| time::Date::from_calendar_date(y, m, d).is_ok())
                })
    })
}
fn cents(v: &Value) -> Option<i64> {
    let n = v.as_f64()?;
    let c = (n * 100.0).round();
    (n.is_finite() && n >= 0.0 && c <= MAX_ID as f64 && (n * 100.0 - c).abs() < 0.000001)
        .then_some(c as i64)
}
pub fn validate_original_asset(input: &Value, operation: &str) -> Result<(), Error> {
    if !matches!(operation, "register" | "depreciate" | "write-off") {
        return Err(Error::InvalidInput("unknown asset operation"));
    }
    let bytes = serde_json::to_vec(input).map_err(|_| Error::InvalidInput("invalid asset JSON"))?;
    if bytes.len() > 16384 {
        return Err(Error::InvalidInput("asset input exceeds16KiB"));
    }
    body(&bytes, &original_asset_schema(operation))?;
    for key in ["acquisitionDate", "date"] {
        if input.get(key).is_some_and(|v| !date(v)) {
            return Err(Error::InvalidInput("valid calendar date required"));
        }
    }
    for key in [
        "name",
        "category",
        "assetAccount",
        "depreciationAccount",
        "accumulatedAccount",
        "expenseAccount",
        "paymentAccount",
        "thresholdRuleSource",
    ] {
        if input.get(key).is_some_and(|v| {
            v.as_str()
                .is_none_or(|s| s.trim().is_empty() || s.chars().any(char::is_control))
        }) {
            return Err(Error::InvalidInput("nonblank asset text required"));
        }
    }
    if operation != "depreciate" && cents(&input["cost"]).is_none_or(|n| n <= 0) {
        return Err(Error::InvalidInput(
            "positive exact two-decimal DKK required",
        ));
    }
    Ok(())
}
pub fn validate_original_asset_actor(actor: Option<&str>) -> Result<(), Error> {
    if actor.is_some_and(|s| {
        s.trim().is_empty()
            || s.trim() != s
            || s.encode_utf16().count() > 160
            || s.chars().any(char::is_control)
    }) {
        return Err(Error::InvalidInput("invalid expected actor assertion"));
    }
    Ok(())
}
fn rules(v: &Value) -> bool {
    v.as_array().is_some_and(|a| {
        !a.is_empty()
            && a.len() <= 128
            && a.iter().all(|v| {
                v.as_str().is_some_and(|s| {
                    s.starts_with("DK-")
                        && s.len() <= 128
                        && s.bytes()
                            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'-')
                })
            })
    })
}
fn receipt(v: &Value, operation: &str, expected: &Value, asset_id: Option<i64>) -> bool {
    let register = operation == "register";
    let writeoff = operation == "write-off";
    let mut p = json!({"ok":{"const":true},"appliedRules":strings(),"errors":{"type":"array","maxItems":0,"items":text(512)}});
    let mut required = vec!["ok", "appliedRules", "errors"];
    if !register {
        p["entryId"] = id();
        p["entryNo"] = text(128);
        p["entryHash"] = text(64);
        required.extend(["entryId", "entryNo", "entryHash"]);
    }
    if writeoff {
        p["writeOffId"] = id();
        p["cost"] = money_schema();
        p["thresholdDkk"] = money_schema();
        required.extend(["writeOffId", "cost", "thresholdDkk"]);
    } else {
        p["assetId"] = id();
        p["periodAmount"] = money_schema();
        required.extend(["assetId", "periodAmount"]);
        if register {
            p["totalPeriods"] = count();
            required.push("totalPeriods");
        } else {
            p["periodIndex"] = count();
            required.push("periodIndex");
        }
    }
    if !shape(v, &object(p, &required))
        || !rules(&v["appliedRules"])
        || !v["appliedRules"]
            .as_array()
            .unwrap()
            .contains(&json!(if writeoff { WRITEOFF } else { DEPR }))
    {
        return false;
    }
    if !register
        && !v["entryHash"]
            .as_str()
            .is_some_and(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()))
    {
        return false;
    }
    if writeoff {
        cents(&v["cost"]).is_some_and(|n| n > 0)
            && cents(&v["cost"]) == cents(&expected["cost"])
            && cents(&v["thresholdDkk"]).is_some_and(|n| n > 0)
    } else if register {
        v["totalPeriods"] == expected["usefulLifeMonths"]
            && cents(&v["periodAmount"])
                == cents(&expected["cost"])
                    .map(|c| c / expected["usefulLifeMonths"].as_i64().unwrap())
    } else {
        v["assetId"].as_i64() == asset_id
            && v["periodIndex"] == expected["period"]
            && cents(&v["periodAmount"]).is_some_and(|n| n > 0)
    }
}
fn report(v: &Value) -> bool {
    let m = money_schema();
    let row = object(
        json!({"assetId":id(),"name":text(500),"category":text(500),"acquisitionDate":date_schema(),"cost":m,"usefulLifeMonths":count(),"postedPeriods":{"type":"integer","minimum":0,"maximum":12000},"accumulatedDepreciation":m,"netBookValue":m,"status":{"type":"string","enum":["active","fully-depreciated","disposed"]},"disposalDate":{"type":["string","null"],"minLength":10,"maxLength":10},"disposalId":{"type":["integer","null"],"minimum":1,"maximum":MAX_ID},"disposalJournalEntryId":{"type":["integer","null"],"minimum":1,"maximum":MAX_ID}}),
        &[
            "assetId",
            "name",
            "category",
            "acquisitionDate",
            "cost",
            "usefulLifeMonths",
            "postedPeriods",
            "accumulatedDepreciation",
            "netBookValue",
        ],
    );
    let totals = object(
        json!({"cost":m,"accumulatedDepreciation":m,"netBookValue":m,"activeCount":{"type":"integer","minimum":0,"maximum":MAX_ID},"fullyDepreciatedCount":{"type":"integer","minimum":0,"maximum":MAX_ID},"disposedCount":{"type":"integer","minimum":0,"maximum":MAX_ID}}),
        &["cost", "accumulatedDepreciation", "netBookValue"],
    );
    if !shape(
        v,
        &object(
            json!({"ok":{"const":true},"assets":{"type":"array","items":row},"totals":totals,"errors":{"type":"array","maxItems":0,"items":text(512)}}),
            &["ok", "assets", "totals", "errors"],
        ),
    ) {
        return false;
    }
    let rows = v["assets"].as_array().unwrap();
    if !rows
        .windows(2)
        .all(|p| p[0]["assetId"].as_i64() < p[1]["assetId"].as_i64())
    {
        return false;
    }
    let mut sums = [0i64; 3];
    let mut classifications = [0u64; 3];
    for r in rows {
        if !date(&r["acquisitionDate"])
            || r["postedPeriods"].as_u64() > r["usefulLifeMonths"].as_u64()
        {
            return false;
        }
        let mut amounts = [0i64; 3];
        for (i, k) in ["cost", "accumulatedDepreciation", "netBookValue"]
            .iter()
            .enumerate()
        {
            let Some(c) = cents(&r[*k]) else {
                return false;
            };
            amounts[i] = c;
            let Some(sum) = sums[i].checked_add(c) else {
                return false;
            };
            sums[i] = sum;
        }
        let disposed = r["status"] == "disposed";
        if amounts[0] <= 0 || amounts[1] > amounts[0] {
            return false;
        }
        if disposed {
            if amounts[2] != 0
                || !date(&r["disposalDate"])
                || r["disposalDate"].as_str() < r["acquisitionDate"].as_str()
                || !r["disposalId"].as_i64().is_some_and(|n| n > 0)
                || !r["disposalJournalEntryId"].as_i64().is_some_and(|n| n > 0)
            {
                return false;
            }
            classifications[2] += 1;
        } else {
            if amounts[1].checked_add(amounts[2]) != Some(amounts[0])
                || ["disposalDate", "disposalId", "disposalJournalEntryId"]
                    .iter()
                    .any(|k| r.get(*k).is_some_and(|v| !v.is_null()))
            {
                return false;
            }
            let fully = r["postedPeriods"] == r["usefulLifeMonths"];
            if r.get("status")
                .is_some_and(|v| v != if fully { "fully-depreciated" } else { "active" })
            {
                return false;
            }
            classifications[usize::from(fully)] += 1;
        }
    }
    let count_names = ["activeCount", "fullyDepreciatedCount", "disposedCount"];
    if count_names.iter().any(|k| v["totals"].get(*k).is_some())
        && !count_names
            .iter()
            .enumerate()
            .all(|(i, k)| v["totals"][*k].as_u64() == Some(classifications[i]))
    {
        return false;
    }
    ["cost", "accumulatedDepreciation", "netBookValue"]
        .iter()
        .enumerate()
        .all(|(i, k)| cents(&v["totals"][*k]) == Some(sums[i]))
}
/// Only bounded source business cores are accepted. Never expose backend metadata.
fn refusal(v: &Value) -> Option<Value> {
    if !shape(
        v,
        &object(
            json!({"ok":{"const":false},"appliedRules":strings(),"errors":strings()}),
            &["ok", "appliedRules", "errors"],
        ),
    ) || v["errors"].as_array()?.is_empty()
        || !rules(&v["appliedRules"])
    {
        return None;
    }
    // These routes return curated source messages, not raw storage errors.
    Some(v.clone())
}
impl ApiClient {
    pub async fn original_asset_register(
        &self,
        company: &str,
        input: &Value,
        confirmed: bool,
        expected_actor: Option<&str>,
    ) -> Result<Value, Error> {
        self.original_asset_write(
            company,
            "register",
            input,
            "register",
            None,
            confirmed,
            expected_actor,
        )
        .await
    }
    pub async fn original_asset_depreciate(
        &self,
        company: &str,
        asset_id: i64,
        input: &Value,
        confirmed: bool,
        expected_actor: Option<&str>,
    ) -> Result<Value, Error> {
        if !(1..=MAX_ID).contains(&asset_id) {
            return Err(Error::InvalidInput("positive safe asset id required"));
        }
        self.original_asset_write(
            company,
            &format!("{asset_id}/depreciate"),
            input,
            "depreciate",
            Some(asset_id),
            confirmed,
            expected_actor,
        )
        .await
    }
    pub async fn original_asset_write_off(
        &self,
        company: &str,
        input: &Value,
        confirmed: bool,
        expected_actor: Option<&str>,
    ) -> Result<Value, Error> {
        self.original_asset_write(
            company,
            "write-off",
            input,
            "write-off",
            None,
            confirmed,
            expected_actor,
        )
        .await
    }
    pub async fn original_asset_register_report(&self, company: &str) -> Result<Value, Error> {
        self.original_asset_request(company, "register-report", None, None, None)
            .await
    }
    #[allow(clippy::too_many_arguments)]
    async fn original_asset_write(
        &self,
        company: &str,
        suffix: &str,
        input: &Value,
        op: &str,
        id: Option<i64>,
        confirmed: bool,
        actor: Option<&str>,
    ) -> Result<Value, Error> {
        if !confirmed {
            return Err(Error::InvalidInput("confirmation required"));
        }
        validate_original_asset_actor(actor)?;
        validate_original_asset(input, op)?;
        let mut payload = input.clone();
        payload["confirm"] = json!(true);
        if let Some(a) = actor {
            payload["expectedActor"] = json!(a);
        }
        self.original_asset_request(company, suffix, Some(payload), Some((op, input)), id)
            .await
    }
    async fn original_asset_request(
        &self,
        company: &str,
        suffix: &str,
        payload: Option<Value>,
        expected: Option<(&str, &Value)>,
        id: Option<i64>,
    ) -> Result<Value, Error> {
        let write = payload.is_some();
        let unknown = || {
            if write {
                Error::OutcomeUnknown
            } else {
                Error::Protocol
            }
        };
        let transport = || {
            if write {
                Error::OutcomeUnknown
            } else {
                Error::Transport
            }
        };
        let bytes = payload
            .map(|p| serde_json::to_vec(&p).map_err(|_| Error::Protocol))
            .transpose()?;
        let mut response = self
            .send(
                if write { Method::POST } else { Method::GET },
                &company_path(company, &format!("asset/original/{suffix}"))?,
                &[],
                bytes,
                self.cookie.as_ref().map(|c| c.as_str()),
            )
            .await
            .map_err(|e| {
                if !write && e == Error::OutcomeUnknown {
                    Error::Transport
                } else {
                    e
                }
            })?;
        let status = response.status().as_u16();
        if status >= 500 || status == 408 {
            return Err(transport());
        }
        if status != 200 && status != 422 {
            return Err(Error::Refused { status });
        }
        let mut bytes = vec![];
        while let Some(chunk) = response.chunk().await.map_err(|_| transport())? {
            if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE {
                return Err(unknown());
            }
            bytes.extend_from_slice(&chunk);
        }
        let v: Value = serde_json::from_slice(&bytes).map_err(|_| {
            if status == 422 {
                Error::Refused { status }
            } else {
                unknown()
            }
        })?;
        if let Some(safe) = refusal(&v) {
            return Ok(safe);
        }
        if status == 422 {
            return Err(Error::Refused { status });
        }
        if expected.map_or_else(|| report(&v), |(op, input)| receipt(&v, op, input, id)) {
            Ok(v)
        } else {
            Err(unknown())
        }
    }
}

#[cfg(test)]
mod asset_disposal_report_tests {
    use super::*;
    fn disposed() -> Value {
        json!({"ok":true,"assets":[{"assetId":4,"name":"Synthetic","category":"Equipment","acquisitionDate":"2026-01-01","cost":125,"usefulLifeMonths":3,"postedPeriods":1,"accumulatedDepreciation":25,"netBookValue":0,"status":"disposed","disposalDate":"2026-10-20","disposalId":2,"disposalJournalEntryId":7}],"totals":{"cost":125,"accumulatedDepreciation":25,"netBookValue":0,"activeCount":0,"disposedCount":1,"fullyDepreciatedCount":0},"errors":[]})
    }
    #[test]
    fn asset_disposal_report_accepts_retained_cost_and_zero_carrying_without_weakening_totals() {
        let v = disposed();
        assert!(report(&v), "valid server disposed report refused");
        for (field, bad) in [
            ("netBookValue", json!(100)),
            ("disposalId", Value::Null),
            ("disposalDate", json!("2025-12-31")),
            ("status", json!("invented")),
        ] {
            let mut bad_report = v.clone();
            bad_report["assets"][0][field] = bad;
            assert!(!report(&bad_report), "{field}");
        }
        let mut bad = v.clone();
        bad["totals"]["disposedCount"] = json!(0);
        assert!(!report(&bad));
        let mut bad = v;
        bad["totals"]["cost"] = json!(126);
        assert!(!report(&bad));
    }
    #[test]
    fn asset_disposal_report_preserves_active_and_legacy_report_compatibility() {
        let mut v = disposed();
        v["assets"][0]["status"] = json!("active");
        v["assets"][0]["netBookValue"] = json!(100);
        v["totals"]["netBookValue"] = json!(100);
        v["totals"]["activeCount"] = json!(1);
        v["totals"]["disposedCount"] = json!(0);
        for k in ["disposalDate", "disposalId", "disposalJournalEntryId"] {
            v["assets"][0][k] = Value::Null;
        }
        assert!(report(&v));
        for k in [
            "status",
            "disposalDate",
            "disposalId",
            "disposalJournalEntryId",
        ] {
            v["assets"][0].as_object_mut().unwrap().remove(k);
        }
        for k in ["activeCount", "fullyDepreciatedCount", "disposedCount"] {
            v["totals"].as_object_mut().unwrap().remove(k);
        }
        assert!(report(&v));
        v["assets"][0]["netBookValue"] = json!(0);
        assert!(!report(&v));
    }
}
