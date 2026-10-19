use crate::tools::{company_schema, native_body, tool};
use rentemester_client::{
    ApiClient, CorporateRecordMutation, Error, MAX_CORPORATE_BODY, MAX_CORPORATE_INGEST_BODY,
    corporate_record_body_schema,
};
use serde::Deserialize;
use serde_json::{Value, json};
pub fn accepts(s: &str) -> bool {
    matches!(
        s,
        "corporate_records"
            | "corporate_record"
            | "corporate_record_original"
            | "ingest_corporate_record"
            | "link_corporate_record"
            | "enrich_corporate_record"
            | "supersede_corporate_record"
    )
}
pub fn tools() -> Vec<Value> {
    let mut tools = vec![tool(
        "corporate_records",
        "Read full authorized corporate source records/history, current complete company scope before pagination. No inference, accounting or sensitivity bypass.",
        json!({"type":"object","required":["company"],"properties":{"company":company_schema(),"cursor":{"type":"integer","minimum":0},"limit":{"type":"integer","minimum":1,"maximum":100}},"additionalProperties":false}),
        true,
    )];
    for name in ["corporate_record", "corporate_record_original"] {
        tools.push(tool(name,if name=="corporate_record"{"Inspect canonical authorized corporate record/history; current non-normal sensitivity additionally requires all-scope MasterData."}else{"Read exact native JSON {ok,bytesBase64,sha256}, bounded24MiB for16MiB original. Source verification/current rights are server-owned. No invented MIME/filename, file access or client verification claim."},json!({"type":"object","required":["company","recordId"],"properties":{"company":company_schema(),"recordId":{"type":"string","minLength":1,"maxLength":64}},"additionalProperties":false}),true));
    }
    for (name, action, id) in [
        (
            "ingest_corporate_record",
            CorporateRecordMutation::Ingest,
            false,
        ),
        (
            "link_corporate_record",
            CorporateRecordMutation::Link(String::new()),
            true,
        ),
        (
            "enrich_corporate_record",
            CorporateRecordMutation::Enrich(String::new()),
            true,
        ),
        (
            "supersede_corporate_record",
            CorporateRecordMutation::Supersede(String::new()),
            true,
        ),
    ] {
        let mut body = native_body(corporate_record_body_schema(&action));
        body["maxLength"] = json!(if id {
            MAX_CORPORATE_BODY
        } else {
            MAX_CORPORATE_INGEST_BODY
        });
        let mut schema = json!({"type":"object","required":["company","body","confirm"],"properties":{"company":company_schema(),"body":body,"confirm":{"const":true}},"additionalProperties":false});
        if id {
            schema["properties"]["recordId"] =
                json!({"type":"string","minLength":1,"maxLength":64});
            schema["required"]
                .as_array_mut()
                .unwrap()
                .push(json!("recordId"));
        }
        tools.push(tool(name,"Exact native confirmed body once; current all-scope MasterData/source/sensitivity policy enforced by server. Ingest24MiB JSON/16MiB decoded original, named corporate-ingest frame48MiB+64KiB; other writes32KiB and all other non-upload frames1MiB. Explicit factual uploader/provenance/links; no inferred actor, keys, automatic retry or accounting. Same-id source replay does not amend old facts/links. After unknown outcome inspect canonical record/history/original before any explicit further action.",schema,false));
    }
    tools
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct List {
    company: String,
    cursor: Option<usize>,
    limit: Option<usize>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Read {
    company: String,
    record_id: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Write {
    company: String,
    record_id: Option<String>,
    body: String,
    confirm: bool,
}
pub async fn call(
    api: &ApiClient,
    name: &str,
    args: Value,
) -> Result<Result<Value, Error>, &'static str> {
    if name == "corporate_records" {
        let r: List = serde_json::from_value(args)
            .map_err(|_| "corporate_records requires only company/cursor?/limit?")?;
        return Ok(api
            .corporate_records(&r.company, r.cursor.unwrap_or(0), r.limit.unwrap_or(25))
            .await);
    }
    if matches!(name, "corporate_record" | "corporate_record_original") {
        let r: Read = serde_json::from_value(args)
            .map_err(|_| "corporate record read requires only company/recordId")?;
        return Ok(if name == "corporate_record_original" {
            api.corporate_record_original(&r.company, &r.record_id)
                .await
        } else {
            api.corporate_record(&r.company, &r.record_id).await
        });
    }
    if name == "ingest_corporate_record" && args.get("recordId").is_some() {
        return Err("ingestion recordId belongs only inside the exact native body");
    }
    let r: Write = serde_json::from_value(args).map_err(
        |_| "corporate write requires company/body/confirm and recordId only for existing record",
    )?;
    let action = match (name, r.record_id) {
        ("ingest_corporate_record", None) => CorporateRecordMutation::Ingest,
        ("link_corporate_record", Some(id)) => CorporateRecordMutation::Link(id),
        ("enrich_corporate_record", Some(id)) => CorporateRecordMutation::Enrich(id),
        ("supersede_corporate_record", Some(id)) => CorporateRecordMutation::Supersede(id),
        _ => return Err("recordId required only for existing corporate record mutation"),
    };
    Ok(api
        .mutate_corporate_record(&r.company, action, r.body.into_bytes(), r.confirm)
        .await)
}
