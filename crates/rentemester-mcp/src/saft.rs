//! Bounded source-profile export over shared transport. No official validation/submission.
use crate::tools::{company_schema, native_body, tool};
use rentemester_client::{ApiClient, Error, MAX_SAFT_BODY, saft_body_schema};
use serde::Deserialize;
use serde_json::{Value, json};
pub fn tools() -> Vec<Value> {
    let mut body = native_body(saft_body_schema());
    body["maxLength"] = json!(MAX_SAFT_BODY);
    vec![tool(
        "saft_export",
        "CompanyExport: explicitly confirmed exact4096-byte native periodStart/periodEnd/confirm:true body once. Returns application/x-tar integer bytes (max256MiB) with existing bounded custom Rentemester v3 XML, README and manifest. Trusted runtime generation clock, full original/hash/audit context; no original bytes, official generic SAF-T validation, filing, keys, local files, journals or automatic retry. Lost response may have committed export audit.",
        json!({"type":"object","required":["company","body","confirm"],"properties":{"company":company_schema(),"body":body,"confirm":{"const":true}},"additionalProperties":false}),
        false,
    )]
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    company: String,
    body: String,
    confirm: bool,
}
pub async fn call(api: &ApiClient, args: Value) -> Result<Result<Value, Error>, &'static str> {
    let i: Input = serde_json::from_value(args)
        .map_err(|_| "saft_export requires only company/body/confirm")?;
    if i.body.len() > MAX_SAFT_BODY {
        return Err("SAF-T body exceeds4096 bytes");
    }
    Ok(api
        .saft_export(&i.company, i.body.into_bytes(), i.confirm)
        .await)
}
