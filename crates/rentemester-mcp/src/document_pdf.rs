//! Bounded explicit PDF parsing and verified evidence reads.
use crate::tools::{company_schema, tool};
use rentemester_client::{ApiClient, Error};
use serde::Deserialize;
use serde_json::{Value, json};
pub fn tools() -> Vec<Value> {
    ["parse_document","document_parse_status","document_parsed_text","parse_pending_documents"].into_iter().map(|name| {
        let write=matches!(name,"parse_document"|"parse_pending_documents");
        let pending=name=="parse_pending_documents";
        let paged=pending||name=="document_parsed_text";
        let mut props=json!({"company":company_schema()});
        let mut required=vec!["company"];
        if !pending {props["id"]=json!({"type":"integer","minimum":1,"maximum":9007199254740991u64});required.push("id");}
        if write {props["confirm"]=json!({"const":true});required.push("confirm");}
        if paged {
            props[if pending {"cursor"} else {"offset"}]=json!({"type":"integer","minimum":0,"maximum":9007199254740991u64});
            props["limit"]=json!({"type":"integer","minimum":1,"maximum":if pending {100} else {10}});
        }
        tool(name,"Explicit registered-PDF inspection. Parsing requires current upload authority and confirmation; status/text require read authority. Server verifies original/hash and retained evidence, bounds and supervises the local parser, and caches results. No OCR, inferred accounting, local file access, keys or automatic retries. After uncertain parsing inspect canonical status before a new request. Pending limit1..100; text limit1..10 pages, default10; cursor/offset default0.",json!({"type":"object","required":required,"properties":props,"additionalProperties":false}),!write)
    }).collect()
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    company: String,
    id: Option<i64>,
    confirm: Option<bool>,
    offset: Option<u64>,
    cursor: Option<u64>,
    limit: Option<u8>,
}
pub async fn call(
    api: &ApiClient,
    name: &str,
    args: Value,
) -> Result<Result<Value, Error>, &'static str> {
    let i: Input = serde_json::from_value(args).map_err(|_| "expected documented PDF arguments")?;
    let pending = name == "parse_pending_documents";
    let write = pending || name == "parse_document";
    if write && i.confirm != Some(true)
        || !write && i.confirm.is_some()
        || pending && (i.id.is_some() || i.offset.is_some())
        || !pending && i.cursor.is_some()
        || name != "document_parsed_text" && i.offset.is_some()
        || !matches!(name, "document_parsed_text" | "parse_pending_documents") && i.limit.is_some()
    {
        return Err("unexpected PDF arguments or missing explicit confirmation");
    }
    if pending {
        return Ok(api
            .documents_parse_pending(
                &i.company,
                i.cursor.unwrap_or(0),
                i.limit.unwrap_or(100),
                true,
            )
            .await);
    }
    let id = i.id.ok_or("positive exact document id required")?;
    Ok(match name {
        "parse_document" => api.document_parse(&i.company, id, true).await,
        "document_parse_status" => api.document_parse_status(&i.company, id).await,
        "document_parsed_text" => {
            api.document_parsed_text(&i.company, id, i.offset.unwrap_or(0), i.limit.unwrap_or(10))
                .await
        }
        _ => return Err("unknown PDF operation"),
    })
}
