use crate::tools::{company_schema, native_body, tool};
use rentemester_client::{
    AccrualMutation, ApiClient, Error, MAX_ACCRUAL_BODY, accrual_body_schema,
};
use serde::Deserialize;
use serde_json::{Value, json};
pub fn accepts(s: &str) -> bool {
    matches!(
        s,
        "accruals" | "accrual_schedule" | "register_accrual" | "recognize_accrual"
    )
}
pub fn tools() -> Vec<Value> {
    let id = json!({"type":"integer","minimum":1,"maximum":9007199254740991u64});
    let mut tools = vec![
        tool(
            "accruals",
            "Read full canonical accrual register, recognized/remaining amounts and totals unchanged. CompanyRead; no query/year selectors or side effects.",
            json!({"type":"object","required":["company"],"properties":{"company":company_schema()},"additionalProperties":false}),
            true,
        ),
        tool(
            "accrual_schedule",
            "Read full server schedule: periodIndex, recognitionDate, amount and recognized facts. CompanyRead; review before explicit recognition, no local amount/date math or accounting effects.",
            json!({"type":"object","required":["company","id"],"properties":{"company":company_schema(),"id":id},"additionalProperties":false}),
            true,
        ),
    ];
    for (name, op, needs_id) in [
        ("register_accrual", AccrualMutation::Register, false),
        ("recognize_accrual", AccrualMutation::Recognize(1), true),
    ] {
        let mut body = native_body(accrual_body_schema(op));
        body["maxLength"] = json!(MAX_ACCRUAL_BODY);
        let mut props = json!({"company":company_schema(),"body":body,"confirm":{"const":true}});
        let mut required = vec!["company", "body", "confirm"];
        if needs_id {
            props["id"] = id.clone();
            required.push("id");
        }
        tools.push(tool(name,"CompanyLedgerPost, exact native256KiB body with inner/outer confirm:true once. Source original/current explicit accounts and period gates rechecked. Register creates an immediate journal, no registration dedup promise. Recognize explicit reviewed schedule periodIndex/date; date cannot precede scheduled date. accrued_expense requires explicit settlementAccountNo. Server handles all three types, schedule math and remaining balances. Duplicate recognized period refuses; no caller keys/receipt, automatic recognition or retries. Unknown outcomes require canonical accruals/schedule/journal/reports/document reads before further action.",json!({"type":"object","required":required,"properties":props,"additionalProperties":false}),false));
    }
    tools
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Read {
    company: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Next {
    company: String,
    id: i64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Register {
    company: String,
    body: String,
    confirm: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Write {
    company: String,
    id: i64,
    body: String,
    confirm: bool,
}
fn valid(id: i64) -> Result<(), &'static str> {
    if (1..=9_007_199_254_740_991).contains(&id) {
        Ok(())
    } else {
        Err("positive exact accrual id required")
    }
}
pub async fn call(
    api: &ApiClient,
    name: &str,
    args: Value,
) -> Result<Result<Value, Error>, &'static str> {
    if name == "accruals" {
        let r: Read = serde_json::from_value(args).map_err(|_| "accruals requires only company")?;
        return Ok(api.accruals(&r.company).await);
    }
    if name == "accrual_schedule" {
        let r: Next = serde_json::from_value(args)
            .map_err(|_| "accrual_schedule requires only company/id")?;
        valid(r.id)?;
        return Ok(api.accrual_schedule(&r.company, r.id).await);
    }
    let (company, body, confirmed, op) = if name == "register_accrual" {
        let r: Register = serde_json::from_value(args)
            .map_err(|_| "register requires only company/body/confirm")?;
        (r.company, r.body, r.confirm, AccrualMutation::Register)
    } else {
        let r: Write = serde_json::from_value(args)
            .map_err(|_| "recognize requires only company/id/body/confirm")?;
        valid(r.id)?;
        (
            r.company,
            r.body,
            r.confirm,
            AccrualMutation::Recognize(r.id),
        )
    };
    if body.len() > MAX_ACCRUAL_BODY {
        return Err("accrual body exceeds native256KiB");
    }
    Ok(api
        .accrual_mutation(&company, op, body.into_bytes(), confirmed)
        .await)
}
