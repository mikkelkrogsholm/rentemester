use crate::tools::{company_schema, native_body, tool};
use rentemester_client::{
    ApiClient, Error, MAX_WORKSPACE_INBOX_BODY, WorkspaceInboxMutation, workspace_inbox_body_schema,
};
use serde::Deserialize;
use serde_json::{Value, json};
pub fn accepts(s: &str) -> bool {
    matches!(
        s,
        "workspace_inbox"
            | "workspace_inbox_source"
            | "workspace_inbox_original"
            | "review_workspace_inbox"
            | "ingest_workspace_inbox"
            | "assign_workspace_inbox"
            | "complete_workspace_inbox"
    )
}
pub fn tools() -> Vec<Value> {
    let mut tools = vec![
        tool(
            "workspace_inbox",
            "Read current authorized workspace ingress sources with native pagination. Anchor is access scope, never implied ledger destination. Full canonical metadata/candidates/assignment/history; no originals or accounting effects.",
            json!({"type":"object","required":["company"],"properties":{"company":company_schema(),"cursor":{"type":"integer","minimum":0},"limit":{"type":"integer","minimum":1,"maximum":100}},"additionalProperties":false}),
            true,
        ),
        tool(
            "workspace_inbox_source",
            "Inspect authorized canonical inbox source/evidence/assignments/history. Use after unknown mutations; original bytes are available through workspace_inbox_original before handoff or target document_original after completion.",
            json!({"type":"object","required":["company","sourceId"],"properties":{"company":company_schema(),"sourceId":{"type":"string","minLength":1,"maxLength":100}},"additionalProperties":false}),
            true,
        ),
    ];
    tools.push(tool("workspace_inbox_original","Read exact authorized retained original as {original:{contentType,sha256,bytes}}, including original EML or attachment. SDK verifies server SHA header against bytes, max25MiB; no local files, automatic viewer/executable launch, inferred metadata or effects. Current source visibility and document-read authority required.",json!({"type":"object","required":["company","sourceId"],"properties":{"company":company_schema(),"sourceId":{"type":"string","minLength":1,"maxLength":100}},"additionalProperties":false}),true));
    for (name, action, id) in [
        (
            "ingest_workspace_inbox",
            WorkspaceInboxMutation::Ingest,
            false,
        ),
        (
            "assign_workspace_inbox",
            WorkspaceInboxMutation::Assign(String::new()),
            true,
        ),
        (
            "review_workspace_inbox",
            WorkspaceInboxMutation::Review(String::new()),
            true,
        ),
        (
            "complete_workspace_inbox",
            WorkspaceInboxMutation::Complete(String::new()),
            true,
        ),
    ] {
        let mut body = native_body(workspace_inbox_body_schema(&action));
        body["maxLength"] = json!(MAX_WORKSPACE_INBOX_BODY);
        let mut schema = json!({"type":"object","required":["company","body","confirm"],"properties":{"company":company_schema(),"body":body,"confirm":{"const":true}},"additionalProperties":false});
        if id {
            schema["properties"]["sourceId"] =
                json!({"type":"string","minLength":1,"maxLength":100});
            schema["required"]
                .as_array_mut()
                .unwrap()
                .push(json!("sourceId"));
        }
        tools.push(tool(name,"Exact native body once, inner/outer confirm. Ingest max35MiB JSON/25MiB decoded upload with caller factual metadata and source-defined idempotencyKey; only ingest_workspace_inbox gets70MiB+64KiB frame, other tools stay1MiB. Native source replay/current principal policy, no generated keys/retries/local files. Native mail requires explicit target-bound review {companySlug,sourceHash,metadata,confirm:true}, which atomically assigns; separate complete {companySlug,confirm:true}, never infer target/metadata from anchor or mail text. Completion creates immutable document only, no journal. On unknown read source/assignments and target documents/original first; preserve exact body/key/principal.",schema,false));
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
    source_id: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Write {
    company: String,
    source_id: Option<String>,
    body: String,
    confirm: bool,
}
pub async fn call(
    api: &ApiClient,
    name: &str,
    args: Value,
) -> Result<Result<Value, Error>, &'static str> {
    if name == "workspace_inbox" {
        let r: List = serde_json::from_value(args)
            .map_err(|_| "workspace_inbox requires only company/cursor?/limit?")?;
        return Ok(api
            .workspace_inbox(&r.company, r.cursor.unwrap_or(0), r.limit.unwrap_or(25))
            .await);
    }
    if name == "workspace_inbox_original" {
        let r: Read = serde_json::from_value(args)
            .map_err(|_| "workspace_inbox_original requires only company/sourceId")?;
        return Ok(api
            .workspace_inbox_original(&r.company, &r.source_id)
            .await
            .map(|original| json!({"ok":true,"original":original})));
    }
    if name == "workspace_inbox_source" {
        let r: Read = serde_json::from_value(args)
            .map_err(|_| "workspace_inbox_source requires only company/sourceId")?;
        return Ok(api.workspace_inbox_source(&r.company, &r.source_id).await);
    }
    let r: Write = serde_json::from_value(args).map_err(
        |_| "inbox mutation requires company/body/confirm and sourceId only for review/assign/complete",
    )?;
    let action = match (name, r.source_id) {
        ("ingest_workspace_inbox", None) => WorkspaceInboxMutation::Ingest,
        ("assign_workspace_inbox", Some(id)) => WorkspaceInboxMutation::Assign(id),
        ("review_workspace_inbox", Some(id)) => WorkspaceInboxMutation::Review(id),
        ("complete_workspace_inbox", Some(id)) => WorkspaceInboxMutation::Complete(id),
        _ => return Err("sourceId required only for review/assign/complete"),
    };
    Ok(api
        .mutate_workspace_inbox(&r.company, action, r.body.into_bytes(), r.confirm)
        .await)
}
