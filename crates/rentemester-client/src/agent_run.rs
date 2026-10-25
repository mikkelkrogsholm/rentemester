//! Original agent loop: one admitted POST, strict flat source report, never retries.
use crate::{ApiClient, Error, MAX_RESPONSE, company_path};
use reqwest::Method;
use serde_json::{Value, json};

fn object(properties: Value) -> Value {
    let required: Vec<String> = properties.as_object().unwrap().keys().cloned().collect();
    json!({"type":"object","properties":properties,"required":required,"additionalProperties":false})
}
fn text() -> Value {
    json!({"type":"string"})
}
fn strings() -> Value {
    json!({"type":"array","items":text()})
}
fn array(items: Value) -> Value {
    json!({"type":"array","items":items})
}
fn integer() -> Value {
    json!({"type":"integer"})
}
fn nullable() -> Value {
    json!({"type":["string","null"]})
}
fn shape(v: &Value, s: &Value) -> bool {
    if s.as_object().is_some_and(|o| o.is_empty()) {
        return true;
    }
    if let Some(a) = s["oneOf"].as_array() {
        return a.iter().filter(|s| shape(v, s)).count() == 1;
    }
    if let Some(a) = s["enum"].as_array() {
        return a.contains(v);
    }
    if let Some(o) = v.as_object() {
        if s["type"] != "object" {
            return false;
        }
        return s["required"]
            .as_array()
            .is_none_or(|a| a.iter().all(|k| o.contains_key(k.as_str().unwrap_or(""))))
            && o.iter()
                .all(|(k, v)| s["properties"].get(k).is_some_and(|s| shape(v, s)));
    }
    if let Some(a) = v.as_array() {
        return s["type"] == "array" && a.iter().all(|v| shape(v, &s["items"]));
    }
    crate::validate_original_mileage_shape(v, s)
}
/// Exact fifteen-field source shape, including required nullable nested fields.
pub fn report_schema() -> Value {
    let count = json!({"type":"integer","minimum":0});
    object(json!({
        "ok":{"type":"boolean"},"actor":text(),"asOf":text(),"company":text(),"phases":strings(),
        "documentsIngested":count,"documentsRejected":count,"bankTransactionsImported":count,
        "accrualRecognitionsDue":count,"summary":strings(),"errors":strings(),
        "expensesBooked":array(object(json!({"documentNo":text(),"documentId":integer(),"bankTransactionId":integer(),"supplier":text(),"amount":{"type":"number"},"currency":text(),"expenseAccount":text(),"vatTreatment":text(),"label":text(),"journalEntryNo":nullable()}))),
        "payablesMatched":array(object(json!({"payableId":integer(),"documentId":integer(),"bankTransactionId":integer(),"supplier":text(),"amount":{"type":"number"},"journalEntryNo":nullable()}))),
        "openExceptions":array(object(json!({"exceptionId":integer(),"type":text(),"severity":text(),"message":text(),"requiredAction":nullable()}))),
        "upcomingDeadlines":array(object(json!({"kind":{"enum":["vat_period","fiscal_year"]},"periodStart":text(),"periodEnd":text(),"dueDate":text(),"daysRemaining":integer(),"ready":{"type":"boolean"},"note":text()})))
    }))
}
pub fn validate_report(v: &Value) -> bool {
    if !shape(v, &report_schema()) {
        return false;
    }
    let phases = [
        "ingest",
        "book",
        "route",
        "payables",
        "reconcile",
        "deadlines",
        "report",
    ];
    let p = v["phases"].as_array().unwrap();
    p.len() <= phases.len()
        && p.iter().zip(phases).all(|(a, b)| a == b)
        && (v["ok"] != true || p.len() == phases.len())
        && (v["ok"] != false || !v["errors"].as_array().unwrap().is_empty())
        && v["actor"].as_str().is_some_and(|s| !s.is_empty())
        && v["company"]
            .as_str()
            .is_some_and(|s| company_path(s, "agent/original/run").is_ok())
}
impl ApiClient {
    /// `request` is the frozen engine::agent_run::Request serialized to JSON.
    /// expectedActor is an assertion only; all credentials use the common sender.
    pub async fn original_agent_run(&self, company: &str, request: &Value) -> Result<Value, Error> {
        let mut properties = json!({"asOf":text(),"confirm":{"const":true},"expectedActor":text(),"inbox":{},"bank":{}});
        // The engine owns metadata validation; any parsed JSON must reach intake.
        let document = object(json!({"fileName":text(),"fileBase64":text(),"metadata":{}}));
        properties["inbox"] = json!({"oneOf":[object(json!({"state":{"const":"files"},"items":array(document)})),object(json!({"state":{"const":"failed"},"error":text()}))]});
        properties["bank"] = json!({"oneOf":[object(json!({"state":{"const":"csv"},"fileName":text(),"csv":text()})),object(json!({"state":{"const":"failed"},"error":text()}))]});
        let schema = json!({"type":"object","properties":properties,"required":["asOf","confirm"],"additionalProperties":false});
        if !shape(request, &schema) {
            return Err(Error::InvalidInput("invalid original agent request"));
        }
        let body = serde_json::to_vec(request)
            .map_err(|_| Error::InvalidInput("invalid original agent JSON"))?;
        let mut response = self
            .send(
                Method::POST,
                &company_path(company, "agent/original/run")?,
                &[],
                Some(body),
                self.cookie.as_ref().map(|s| s.as_str()),
            )
            .await?;
        let status = response.status().as_u16();
        if status == 408 || status >= 500 {
            return Err(Error::OutcomeUnknown);
        }
        if status != 200 && status != 422 {
            return Err(Error::Refused { status });
        }
        let mut bytes = vec![];
        while let Some(chunk) = response.chunk().await.map_err(|_| Error::OutcomeUnknown)? {
            if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE {
                return Err(Error::OutcomeUnknown);
            }
            bytes.extend_from_slice(&chunk);
        }
        let report: Value = serde_json::from_slice(&bytes).map_err(|_| Error::OutcomeUnknown)?;
        if !validate_report(&report)
            || (status == 422 && report["ok"] != false)
            || report["asOf"] != request["asOf"]
            || report["company"] != company
            || request
                .get("expectedActor")
                .is_some_and(|actor| report["actor"] != *actor)
        {
            return Err(Error::OutcomeUnknown);
        }
        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_report_and_complete_success_only() {
        let mut v = json!({"ok":true,"actor":"user:test","asOf":"2026-02-01","company":"test","phases":["ingest","book","route","payables","reconcile","deadlines","report"],"documentsIngested":0,"documentsRejected":0,"bankTransactionsImported":0,"expensesBooked":[],"payablesMatched":[],"accrualRecognitionsDue":0,"openExceptions":[],"upcomingDeadlines":[],"summary":[],"errors":[]});
        assert!(validate_report(&v));
        v["phases"] = json!(["ingest"]);
        assert!(!validate_report(&v));
        v["ok"] = json!(false);
        v["errors"] = json!(["bank failed"]);
        assert!(validate_report(&v));
        v["extra"] = json!(1);
        assert!(!validate_report(&v));
        v.as_object_mut().unwrap().remove("extra");
        v.as_object_mut().unwrap().remove("payablesMatched");
        assert!(!validate_report(&v));
    }
}
