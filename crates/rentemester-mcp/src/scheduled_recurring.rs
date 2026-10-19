use crate::tools::{company_schema, native_body, tool};
use rentemester_client::{
    ApiClient, Error, MAX_SCHEDULED_RECURRING_BODY, ScheduledRecurringMutation as Op,
    scheduled_recurring_body_schema,
};
use serde::Deserialize;
use serde_json::{Value, json};
pub fn accepts(name: &str) -> bool {
    matches!(
        name,
        "scheduled_recurring"
            | "scheduled_recurring_plan"
            | "enable_scheduled_recurring"
            | "pause_scheduled_recurring"
            | "resume_scheduled_recurring"
            | "cancel_scheduled_recurring"
            | "catch_up_scheduled_recurring"
            | "skip_scheduled_recurring"
            | "dispatch_scheduled_recurring"
    )
}
pub fn tools() -> Vec<Value> {
    let mut out = vec![
        tool(
            "scheduled_recurring",
            "Read full canonical scheduled-recurring status/history at the server date; CompanyRead. No timer, dispatch, query, body or confirmation.",
            json!({"type":"object","required":["company"],"properties":{"company":company_schema()},"additionalProperties":false}),
            true,
        ),
        tool(
            "scheduled_recurring_plan",
            "Read exact current template/state/plan hashes, normalDue, missed/truncated and queued occurrences. Explicit templateId, no date/default math/confirmation/body/dispatch.",
            json!({"type":"object","required":["company","templateId"],"properties":{"company":company_schema(),"templateId":{"type":"integer","minimum":1,"maximum":9007199254740991u64}},"additionalProperties":false}),
            true,
        ),
    ];
    for (name, action) in [
        ("enable_scheduled_recurring", Op::Enable),
        ("pause_scheduled_recurring", Op::Pause),
        ("resume_scheduled_recurring", Op::Resume),
        ("cancel_scheduled_recurring", Op::Cancel),
        ("catch_up_scheduled_recurring", Op::CatchUp),
        ("skip_scheduled_recurring", Op::Skip),
        ("dispatch_scheduled_recurring", Op::Dispatch),
    ] {
        let mut body = native_body(scheduled_recurring_body_schema(action));
        body["maxLength"] = json!(MAX_SCHEDULED_RECURRING_BODY);
        out.push(tool(name,"Exact native16KiB JSON once with inner+outer confirm:true; current CompanyDraftWrite and server-bound consenting owner, current template/state/plan hashes required. Catch-up ONLY queues; skip ONLY audits. Explicit caller-driven dispatch creates actual issued JSON/PDF UNPOSTED/UNSENT; no hidden credentials/service, date/period/currency/template/principal defaults, keys/math or automatic retries. After outcome_unknown read scheduled_recurring and scheduled_recurring_plan before any manual action.",json!({"type":"object","required":["company","body","confirm"],"properties":{"company":company_schema(),"body":body,"confirm":{"const":true}},"additionalProperties":false}),false));
    }
    out
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Read {
    company: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Plan {
    company: String,
    template_id: i64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Write {
    company: String,
    body: String,
    confirm: bool,
}
pub async fn call(
    api: &ApiClient,
    name: &str,
    args: Value,
) -> Result<Result<Value, Error>, &'static str> {
    if name == "scheduled_recurring" {
        let v: Read = serde_json::from_value(args)
            .map_err(|_| "scheduled_recurring requires only company")?;
        return Ok(api.scheduled_recurring(&v.company).await);
    }
    if name == "scheduled_recurring_plan" {
        let v: Plan = serde_json::from_value(args)
            .map_err(|_| "scheduled_recurring_plan requires only company/templateId")?;
        if !(1..=9_007_199_254_740_991).contains(&v.template_id) {
            return Err("positive exact templateId required");
        }
        return Ok(api
            .scheduled_recurring_plan(&v.company, v.template_id)
            .await);
    }
    let v: Write = serde_json::from_value(args)
        .map_err(|_| "scheduled recurring mutation requires only company/body/confirm")?;
    if v.body.len() > MAX_SCHEDULED_RECURRING_BODY {
        return Err("scheduled recurring body exceeds native16KiB");
    }
    let action = match name {
        "enable_scheduled_recurring" => Op::Enable,
        "pause_scheduled_recurring" => Op::Pause,
        "resume_scheduled_recurring" => Op::Resume,
        "cancel_scheduled_recurring" => Op::Cancel,
        "catch_up_scheduled_recurring" => Op::CatchUp,
        "skip_scheduled_recurring" => Op::Skip,
        _ => Op::Dispatch,
    };
    Ok(api
        .mutate_scheduled_recurring(&v.company, action, v.body.into_bytes(), v.confirm)
        .await)
}
