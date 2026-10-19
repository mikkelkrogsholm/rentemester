//! Source-owned direct purchase correction over exact reviewed JSON.
use crate::tools::{company_schema, native_body, tool};
use rentemester_client::{
    ApiClient, Error, MAX_PAYABLE_CORRECTION_BODY, payable_correction_body_schema,
};
use serde::Deserialize;
use serde_json::{Value, json};
pub fn accepts(name: &str) -> bool {
    matches!(
        name,
        "direct_bank_payable_correction_plan" | "direct_bank_payable_correction"
    )
}
pub fn tools() -> Vec<Value> {
    [false,true].into_iter().map(|apply|{
        let mut body=native_body(payable_correction_body_schema(apply));body["maxLength"]=json!(MAX_PAYABLE_CORRECTION_BODY);
        let mut schema=json!({"type":"object","properties":{"company":company_schema(),"body":body},"required":["company","body"],"additionalProperties":false});
        if apply{schema["properties"]["confirm"]=json!({"const":true});schema["required"].as_array_mut().unwrap().push(json!("confirm"));}
        tool(if apply{"direct_bank_payable_correction"}else{"direct_bank_payable_correction_plan"},if apply{"Explicit atomic correction of a direct bank purchase into a dated payable and settlement. Exact native16KiB JSON, reviewed planHash, reason, caller key and inner/outer confirm:true. Server owns original proof, reversal, payable, payment and bank relation. Current authority first. Unknown outcome: inspect canonical payables/journal/bank and retain exact body/key; no automatic retry."}else{"Read-only POST: obtain full exact source-bound direct-bank payable correction plan. Native16KiB JSON without confirmation or key. No posting, local arithmetic, invented dates or hashes."},schema,!apply)
    }).collect()
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Plan {
    company: String,
    body: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Apply {
    company: String,
    body: String,
    confirm: bool,
}
pub async fn call(
    api: &ApiClient,
    name: &str,
    args: Value,
) -> Result<Result<Value, Error>, &'static str> {
    let apply = name == "direct_bank_payable_correction";
    let (company, body, confirmed) = if apply {
        let v: Apply = serde_json::from_value(args)
            .map_err(|_| "requires company, exact body and confirm:true")?;
        (v.company, v.body, v.confirm)
    } else {
        let v: Plan = serde_json::from_value(args)
            .map_err(|_| "plan requires only company and exact body")?;
        (v.company, v.body, false)
    };
    if body.len() > MAX_PAYABLE_CORRECTION_BODY {
        return Err("correction body exceeds native16KiB");
    }
    Ok(api
        .direct_bank_payable_correction(&company, body.into_bytes(), apply, confirmed)
        .await)
}
