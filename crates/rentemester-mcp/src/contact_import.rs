//! Thin source contact-import wire contract.
use crate::tools::{company_schema, native_body, tool};
use rentemester_client::{ApiClient, Error, MAX_CONTACT_IMPORT_BODY, contact_import_body_schema};
use serde::Deserialize;
use serde_json::{Value, json};
pub fn accepts(name: &str) -> bool {
    name == "import_contacts"
}
pub fn tools() -> Vec<Value> {
    let mut body = native_body(contact_import_body_schema());
    body["maxLength"] = json!(MAX_CONTACT_IMPORT_BODY);
    vec![tool(
        "import_contacts",
        "Explicit contact-only Dinero CSV import. Exact native12MiB JSON {fileName,content,enrichCvr?,confirm:true}, explicit company and outer confirm:true. Server owns parsing, roles, natural-key dedup, optional CVR/cache and current rights. Full partial-row summary/errors retained; no financial posting or provider credentials. Unknown outcome: inspect canonical contacts before another explicit import; never retry automatically or fabricate a receipt/key.",
        json!({"type":"object","properties":{"company":company_schema(),"body":body,"confirm":{"const":true}},"required":["company","body","confirm"],"additionalProperties":false}),
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
pub async fn call(
    api: &ApiClient,
    _name: &str,
    args: Value,
) -> Result<Result<Value, Error>, &'static str> {
    let v: Input = serde_json::from_value(args)
        .map_err(|_| "requires only company, exact confirmed body and confirm:true")?;
    if !v.confirm || v.body.len() > MAX_CONTACT_IMPORT_BODY {
        return Err("explicit confirmation and native12MiB body required");
    }
    Ok(api
        .import_contacts(&v.company, v.body.into_bytes(), v.confirm)
        .await)
}
