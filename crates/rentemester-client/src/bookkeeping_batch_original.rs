//! Six source operations over the unchanged native atomic batch transport.
//! State includes immutable revisions, approvals, attempts, receipts, final checks
//! and inspect-only legacy history. Receipts and hashes are never synthesized.
use crate::{
    ApiClient, BookkeepingBatchMutation, Error, bookkeeping_batch_body_schema,
    bookkeeping_batch_scope_schema, domain::body,
};
use serde_json::{Value, json};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OriginalBatchOperation {
    Plan,
    Persist,
    DryRun,
    Approve,
    Apply,
    Status,
}
impl OriginalBatchOperation {
    pub fn write(self) -> bool {
        !matches!(self, Self::Plan | Self::Status)
    }
}
pub fn bookkeeping_batch_original_schema(op: OriginalBatchOperation) -> Value {
    use OriginalBatchOperation::*;
    match op {
        Plan => bookkeeping_batch_scope_schema(),
        Persist | DryRun => bookkeeping_batch_body_schema(BookkeepingBatchMutation::Persist),
        Approve => bookkeeping_batch_body_schema(BookkeepingBatchMutation::Approve),
        Apply => bookkeeping_batch_body_schema(BookkeepingBatchMutation::Apply),
        Status => {
            json!({"type":"object","properties":{"runId":{"type":"integer","minimum":1,"maximum":9007199254740991u64}},"required":["runId"],"additionalProperties":false})
        }
    }
}
fn hash(v: &Value) -> bool {
    v.as_str().is_some_and(|s| {
        s.len() == 64
            && s.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}
fn id(v: &Value) -> bool {
    v.as_u64()
        .is_some_and(|n| (1..=9007199254740991).contains(&n))
}
pub fn validate_bookkeeping_batch_original(
    op: OriginalBatchOperation,
    input: &Value,
) -> Result<(), Error> {
    let bytes =
        serde_json::to_vec(input).map_err(|_| Error::InvalidInput("batch JSON required"))?;
    if bytes.len() > crate::MAX_BOOKKEEPING_BATCH_BODY {
        return Err(Error::InvalidInput("batch exceeds32KiB"));
    }
    body(&bytes, &bookkeeping_batch_original_schema(op))?;
    use OriginalBatchOperation::*;
    if matches!(op, Plan | Persist | DryRun) {
        let format = time::format_description::parse_borrowed::<2>("[year]-[month]-[day]").unwrap();
        for key in ["accountingFrom", "accountingTo", "bankFrom", "bankTo"] {
            if !input[key]
                .as_str()
                .is_some_and(|s| s.len() == 10 && time::Date::parse(s, &format).is_ok())
            {
                return Err(Error::InvalidInput("real ISO calendar dates required"));
            }
        }
        if input["accountingFrom"].as_str() > input["accountingTo"].as_str()
            || input["bankFrom"].as_str() > input["bankTo"].as_str()
        {
            return Err(Error::InvalidInput("ordered dates required"));
        }
    }
    if matches!(op, Persist | DryRun)
        && (!hash(&input["expectedPlanHash"])
            || input["runKey"].as_str().is_none_or(|s| {
                s.trim().is_empty() || s.len() > 1024 || s.chars().any(char::is_control)
            }))
    {
        return Err(Error::InvalidInput(
            "reviewed hash and bounded exact run key required",
        ));
    }
    if matches!(op, Approve | Apply) && !hash(&input["planHash"]) {
        return Err(Error::InvalidInput("reviewed lowercase hash required"));
    }
    Ok(())
}
fn plan(v: &Value) -> bool {
    v.is_object() && hash(&v["planHash"]) && v["items"].is_array()
}
fn state(v: &Value) -> bool {
    v.is_object()
        && id(&v["run"]["runId"])
        && hash(&v["run"]["planHash"])
        && v["run"]["plan"]
            .as_str()
            .and_then(|text| serde_json::from_str::<Value>(text).ok())
            .is_some_and(|stored| plan(&stored) && stored["planHash"] == v["run"]["planHash"])
        && [
            "revisions",
            "attempts",
            "receipts",
            "finalChecks",
            "legacyEvents",
            "legacyFinalChecks",
        ]
        .iter()
        .all(|k| v[*k].is_array())
}
impl ApiClient {
    pub async fn bookkeeping_batch_original(
        &self,
        company: &str,
        op: OriginalBatchOperation,
        input: Value,
    ) -> Result<Value, Error> {
        validate_bookkeeping_batch_original(op, &input)?;
        if self.service_key.is_some() {
            return Err(Error::InvalidInput(
                "batch requires current human cookie; service unsupported",
            ));
        }
        use OriginalBatchOperation::*;
        let mut v = match op {
            Plan => self.bookkeeping_batch_plan(company, &input).await?,
            Status => {
                self.bookkeeping_batch_status(company, input["runId"].as_i64().unwrap())
                    .await?
            }
            _ => {
                self.mutate_bookkeeping_batch(
                    company,
                    match op {
                        Persist | DryRun => BookkeepingBatchMutation::Persist,
                        Approve => BookkeepingBatchMutation::Approve,
                        _ => BookkeepingBatchMutation::Apply,
                    },
                    serde_json::to_vec(&input)
                        .map_err(|_| Error::InvalidInput("batch JSON required"))?,
                    true,
                )
                .await?
            }
        };
        if v["ok"] == false {
            return Ok(v);
        }
        let valid = match op {
            Plan => v["dryRun"] == true && plan(&v["plan"]),
            Persist | DryRun => {
                id(&v["runId"])
                    && v["duplicate"].is_boolean()
                    && v["dryRun"] == true
                    && plan(&v["plan"])
                    && v["plan"]["planHash"] == input["expectedPlanHash"]
                    && state(&v["state"])
                    && v["runId"] == v["state"]["run"]["runId"]
            }
            Approve | Status => {
                state(&v["state"])
                    && v["state"]["run"]["runId"] == input["runId"]
                    && (op == Status || v["state"]["run"]["planHash"] == input["planHash"])
            }
            Apply => {
                state(&v["state"])
                    && v["runId"] == input["runId"]
                    && v["planHash"] == input["planHash"]
                    && id(&v["applyAttemptId"])
                    && v["duplicate"].is_boolean()
                    && v["results"]
                        .as_array()
                        .is_some_and(|a| a.iter().all(Value::is_object))
                    && v["checks"].is_array()
                    && v["state"]["run"]["runId"] == input["runId"]
                    && v["state"]["run"]["planHash"] == input["planHash"]
            }
        };
        if !valid {
            return Err(if op.write() {
                Error::OutcomeUnknown
            } else {
                Error::Protocol
            });
        }
        if op == Persist {
            v.as_object_mut().unwrap().remove("dryRun");
        }
        Ok(v)
    }
}
