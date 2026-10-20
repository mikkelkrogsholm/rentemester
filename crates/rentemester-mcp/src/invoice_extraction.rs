//! Explicit optional invoice extraction and retained evidence; never infers bookkeeping.
use crate::tools::{company_schema, tool};
use rentemester_client::{ApiClient, Error};
use serde::Deserialize;
use serde_json::{Value, json};

pub fn tools() -> Vec<Value> {
    [("documents_invoice_extraction",true),("documents_extract_invoice",false)].into_iter().map(|(name,read_only)|{
        let mut properties=json!({"company":company_schema(),"documentId":{"type":"integer","minimum":1,"maximum":9007199254740991u64}});
        let mut required=vec!["company","documentId"];
        if !read_only{properties["confirm"]=json!({"const":true});required.push("confirm");}
        tool(name,if read_only {
            "Read current retained invoice extraction evidence for one registered document. Current company document-read permission is required. Returns cited fields, confidence, provenance, cache/conflict and unresolved evidence unchanged; absent evidence stays absent. This read never calls a provider or changes metadata, journals or originals. No provider configuration, local files or secrets."
        } else {
            "Explicitly request optional invoice extraction for one registered physical PDF (max10MiB), with confirm:true. Requires current company master-data permission and MFA plus a configured verified-HTTPS provider; disabled means unavailable, not inferred success. Server owns source proof, frozen context, citations/confidence/conflicts and completed cache. No inferred metadata, posting or accounting decisions. After outcome_unknown read documents_invoice_extraction first; never automatically retry or replay provider work."
        },json!({"type":"object","required":required,"properties":properties,"additionalProperties":false}),read_only)
    }).collect()
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Read {
    company: String,
    document_id: i64,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Extract {
    company: String,
    document_id: i64,
    confirm: bool,
}
pub async fn call(
    api: &ApiClient,
    name: &str,
    args: Value,
) -> Result<Result<Value, Error>, &'static str> {
    match name {
        "documents_invoice_extraction" => {
            let i: Read = serde_json::from_value(args)
                .map_err(|_| "expected company and positive exact documentId only")?;
            Ok(api
                .document_invoice_extraction(&i.company, i.document_id)
                .await)
        }
        "documents_extract_invoice" => {
            let i: Extract = serde_json::from_value(args)
                .map_err(|_| "expected company, positive exact documentId and confirm:true only")?;
            if !i.confirm {
                return Err("explicit confirm:true required for invoice extraction");
            }
            Ok(api
                .extract_document_invoice(&i.company, i.document_id, true)
                .await)
        }
        _ => Err("unknown invoice extraction operation"),
    }
}
