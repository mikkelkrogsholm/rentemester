//! Bounded protocol upload and exact authorized original bytes, without local files.
use crate::tools::{company_schema, native_body, tool};
use rentemester_client::{ApiClient, Error, MAX_DOCUMENT_INGEST_BODY, document_ingest_schema};
use serde::Deserialize;
use serde_json::{Value, json};
pub fn tools() -> Vec<Value> {
    let mut body = native_body(document_ingest_schema());
    body["maxLength"] = json!(MAX_DOCUMENT_INGEST_BODY);
    body["description"] = json!(
        "Exact native UTF-8 JSON string; max35MiB raw bytes, ingest_document-only frame limit70MiB+64KiB. Base64 file bytes and factual metadata supplied by caller; no local path access or inferred values."
    );
    vec![
        tool(
            "ingest_document",
            "Ingest immutable original using exact native JSON body once, confirm:true inside body plus outer confirmation. Current documents.upload authority; server enforces decoded25MiB bound, metadata, MIME/hash and retention. Native JSON body max35MiB; document-only protocol frame max70MiB+64KiB; other tools stay1MiB. No force/vendor links, keys or automatic retry. On outcome_unknown inspect documents and document_original; preserve original request. Duplicates refuse and do not authorize resend.",
            json!({"type":"object","required":["company","body","confirm"],"properties":{"company":company_schema(),"body":body,"confirm":{"const":true}},"additionalProperties":false}),
            false,
        ),
        tool(
            "document_original",
            "Read recorded MIME and exact authorized original bytes as {original:{contentType,bytes}}. Current documents.read authority; server verifies stored hash. Positive exact document id, max25MiB original; creates no files or journals. No confirmation/body/filters.",
            json!({"type":"object","required":["company","id"],"properties":{"company":company_schema(),"id":{"type":"integer","minimum":1,"maximum":9007199254740991u64}},"additionalProperties":false}),
            true,
        ),
    ]
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Read {
    company: String,
    id: i64,
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
    if name == "document_original" {
        let input: Read = serde_json::from_value(args)
            .map_err(|_| "document_original requires only company and positive exact id")?;
        if !(1..=9_007_199_254_740_991).contains(&input.id) {
            return Err("document_original requires positive exact id");
        }
        return Ok(api.document_original(&input.company, input.id).await);
    }
    let input: Write = serde_json::from_value(args)
        .map_err(|_| "ingest_document requires only company, exact body string and confirmation")?;
    if input.body.len() > MAX_DOCUMENT_INGEST_BODY {
        return Err("ingest_document body exceeds native35MiB UTF-8 JSON bound");
    }
    Ok(api
        .ingest_document(&input.company, input.body.into_bytes(), input.confirm)
        .await)
}
