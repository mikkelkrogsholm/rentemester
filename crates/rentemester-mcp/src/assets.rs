use crate::tools::{company_schema, native_body, tool};
use rentemester_client::{ApiClient, AssetMutation, Error, MAX_ASSET_BODY, asset_body_schema};
use serde::Deserialize;
use serde_json::{Value, json};
pub fn accepts(s: &str) -> bool {
    matches!(
        s,
        "assets" | "next_asset_depreciation" | "register_asset" | "depreciate_asset"
    )
}
pub fn tools() -> Vec<Value> {
    let id = json!({"type":"integer","minimum":1,"maximum":9007199254740991u64});
    let mut tools = vec![
        tool(
            "assets",
            "Read full canonical asset register, totals and evidence. CompanyRead, no side effects.",
            json!({"type":"object","required":["company"],"properties":{"company":company_schema()},"additionalProperties":false}),
            true,
        ),
        tool(
            "next_asset_depreciation",
            "Read server-computed nextPeriodIndex/nextPeriodAmount and posted/remaining schedule facts. CompanyRead; no accounting effects. Preserve full JSON, review before explicit depreciation; do not calculate amount locally.",
            json!({"type":"object","required":["company","id"],"properties":{"company":company_schema(),"id":id},"additionalProperties":false}),
            true,
        ),
    ];
    for (name, op, needs_id) in [
        ("register_asset", AssetMutation::Register, false),
        ("depreciate_asset", AssetMutation::Depreciate(1), true),
    ] {
        let mut body = native_body(asset_body_schema(op));
        body["maxLength"] = json!(MAX_ASSET_BODY);
        let mut props = json!({"company":company_schema(),"body":body,"confirm":{"const":true}});
        let mut required = vec!["company", "body", "confirm"];
        if needs_id {
            props["id"] = id.clone();
            required.push("id");
        }
        tools.push(tool(name,"CompanyLedgerPost, exact native body max256KiB and inner/outer confirm:true. Registration requires immutable purchase original, actual capitalisation and explicit correctly typed asset/expense/accumulated accounts. Depreciation uses shared server accounting, current original/account/period checks. Review next_asset_depreciation and select its explicit periodIndex: omitted/null selects next and blindly resending could post another period. Already posted explicit period refuses, no caller keys or receipt. No automatic runs/retries. Unknown outcomes require assets/next/journal/trial-balance reads before further action.",json!({"type":"object","required":required,"properties":props,"additionalProperties":false}),false));
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
        Err("positive exact asset id required")
    }
}
pub async fn call(
    api: &ApiClient,
    name: &str,
    args: Value,
) -> Result<Result<Value, Error>, &'static str> {
    if name == "assets" {
        let r: Read = serde_json::from_value(args).map_err(|_| "assets requires only company")?;
        return Ok(api.assets(&r.company).await);
    }
    if name == "next_asset_depreciation" {
        let r: Next = serde_json::from_value(args)
            .map_err(|_| "next_asset_depreciation requires only company/id")?;
        valid(r.id)?;
        return Ok(api.next_asset_depreciation(&r.company, r.id).await);
    }
    let (company, body, confirmed, op) = if name == "register_asset" {
        let r: Register = serde_json::from_value(args)
            .map_err(|_| "register requires only company/body/confirm")?;
        (r.company, r.body, r.confirm, AssetMutation::Register)
    } else {
        let r: Write = serde_json::from_value(args)
            .map_err(|_| "depreciate requires only company/id/body/confirm")?;
        valid(r.id)?;
        (
            r.company,
            r.body,
            r.confirm,
            AssetMutation::Depreciate(r.id),
        )
    };
    if body.len() > MAX_ASSET_BODY {
        return Err("asset body exceeds native256KiB");
    }
    Ok(api
        .asset_mutation(&company, op, body.into_bytes(), confirmed)
        .await)
}
