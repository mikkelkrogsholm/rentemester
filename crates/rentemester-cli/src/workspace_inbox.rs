use rentemester_client::{
    ApiClient, MAX_WORKSPACE_INBOX_BODY, WorkspaceInboxMutation, workspace_inbox_body_schema,
};
use rentemester_engine::Error;
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::Read};
type Failure = (u8, Error);
fn invalid(s: &str) -> Failure {
    (2, Error::new("invalid_input", s))
}
pub fn accepts(s: &str) -> bool {
    matches!(
        s,
        "workspace-inbox"
            | "workspace-inbox-source"
            | "ingest-workspace-inbox"
            | "assign-workspace-inbox"
            | "complete-workspace-inbox"
    )
}
pub fn help() -> Value {
    json!({"usage":"workspace-inbox --company ANCHOR [--cursor N] [--limit 1..100]; workspace-inbox-source --company ANCHOR --source-id ID; ingest-workspace-inbox --company ANCHOR --confirm < native.json; assign-workspace-inbox/complete-workspace-inbox additionally --source-id ID and exact {companySlug,confirm:true} stdin. Explicit server/private MFA cookie required.","policy":"Anchor never implies destination. Native35MiB JSON, decoded25MiB original; caller supplies factual metadata/explicit upload, original supported idempotencyKey preserved. Review source/candidates, explicitly assign then independently confirm completion. Creates target document only, no journal/automatic processing/keys/retries/local file access. Unknown outcomes require source/assignments and canonical target documents/original reads before any explicit replay.","bodySchemas":{"ingest":workspace_inbox_body_schema(&WorkspaceInboxMutation::Ingest),"destination":workspace_inbox_body_schema(&WorkspaceInboxMutation::Assign(String::new()))}})
}
pub fn run(
    command: &str,
    flags: &BTreeMap<&str, &str>,
    allow: bool,
    confirm: bool,
    preview: bool,
) -> Result<Value, Failure> {
    let list = command == "workspace-inbox";
    let read = list || command == "workspace-inbox-source";
    let id = command != "workspace-inbox" && command != "ingest-workspace-inbox";
    if confirm == read
        || preview
        || flags.keys().any(|k| {
            !(["--server", "--cookie-file", "--company"].contains(k)
                || id && *k == "--source-id"
                || list && ["--cursor", "--limit"].contains(k))
        })
    {
        return Err(invalid(
            "inbox selectors/body/confirmation do not match command; see remote help",
        ));
    }
    let id = if id {
        Some(
            *flags
                .get("--source-id")
                .ok_or_else(|| invalid("explicit source-id required"))?,
        )
    } else {
        None
    };
    let company = flags
        .get("--company")
        .ok_or_else(|| invalid("explicit company required"))?;
    let server = flags
        .get("--server")
        .ok_or_else(|| invalid("explicit server required"))?;
    let cookie = flags
        .get("--cookie-file")
        .ok_or_else(|| invalid("private cookie file required"))?;
    let parse = |key, default| -> Result<usize, Failure> {
        flags.get(key).map_or(Ok(default), |v| {
            if v.is_empty() || !v.bytes().all(|b| b.is_ascii_digit()) {
                return Err(invalid("canonical nonnegative cursor/limit required"));
            }
            v.parse().map_err(|_| invalid("cursor/limit out of range"))
        })
    };
    let cursor = parse("--cursor", 0)?;
    let limit = parse("--limit", 25)?;
    let cookie = rentemester_client::read_cookie_file(std::path::Path::new(cookie))
        .map_err(crate::remote::client_error)?;
    let api = ApiClient::new(server, allow, Some(cookie)).map_err(crate::remote::client_error)?;
    let mut bytes = Vec::new();
    if !read {
        std::io::stdin()
            .take(MAX_WORKSPACE_INBOX_BODY as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| invalid("could not read inbox body"))?;
    }
    let rt = tokio::runtime::Runtime::new().map_err(|_| invalid("HTTP runtime unavailable"))?;
    let key = if command == "ingest-workspace-inbox" && bytes.len() <= MAX_WORKSPACE_INBOX_BODY {
        serde_json::from_slice::<Value>(&bytes)
            .ok()
            .and_then(|v| v["idempotencyKey"].as_str().map(str::to_owned))
    } else {
        None
    };
    let outcome = rt.block_on(async {
        if list {
            api.workspace_inbox(company, cursor, limit).await
        } else if read {
            api.workspace_inbox_source(company, id.unwrap()).await
        } else {
            let action = match command {
                "ingest-workspace-inbox" => WorkspaceInboxMutation::Ingest,
                "assign-workspace-inbox" => WorkspaceInboxMutation::Assign(id.unwrap().into()),
                _ => WorkspaceInboxMutation::Complete(id.unwrap().into()),
            };
            api.mutate_workspace_inbox(company, action, bytes, confirm)
                .await
        }
    });
    if outcome == Err(rentemester_client::Error::OutcomeUnknown) {
        return Ok(
            json!({"ok":false,"code":"outcome_unknown","idempotencyKey":key,"errors":["Inbox outcome unknown; inspect source/assignments and target documents/original. Preserve original request/key/current principal; never retry automatically."]}),
        );
    }
    let result = outcome.map_err(crate::remote::client_error)?;
    Ok(json!({"ok":result.get("ok").and_then(Value::as_bool).unwrap_or(true),"data":result}))
}
