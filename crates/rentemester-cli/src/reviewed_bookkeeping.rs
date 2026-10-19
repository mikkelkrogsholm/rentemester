//! Finite reviewed source workflow; exact native input and server-owned authority.
use rentemester_client::{
    ApiClient, ReviewedMutation, ReviewedRead, reviewed_body_schema, valid_draft_id,
};
use rentemester_engine::Error;
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::Read};
type Failure = (u8, Error);
fn invalid(s: &str) -> Failure {
    (2, Error::new("invalid_input", s))
}
pub fn accepts(command: &str) -> bool {
    matches!(
        command,
        "reviewed-bank"
            | "accounting-drafts"
            | "posting-rules"
            | "create-bank-draft"
            | "submit-draft"
            | "reject-draft"
            | "approve-draft"
    )
}
pub fn help() -> Value {
    json!({"reads":["reviewed-bank","accounting-drafts","posting-rules"],"writes":["create-bank-draft","submit-draft","reject-draft","approve-draft"],"usage":"Explicit --server --cookie-file --company for every command. Writes require --confirm and exact native stdin body; submit/reject/approve also --draft-id ID. No year/account/id/date filters, attribution or generic idempotency keys. Select draftId explicitly in create body and use expectedSourceHash from queue; expectedEventHash from canonical accounting-drafts read. Submit may omit native body confirm (outer confirmation still required). Independent current reviewer must approve/reject stored payload; server rechecks all sources and closed periods. No readonly POST planning endpoint and no automatic retries. On outcome_unknown read drafts, reviewed-bank and journal; preserve original body/hash/actor. Only explicit identical reviewer approval can read an existing durable receipt under current server authority; a successful read is not permission to automatically resend.","bodySchemas":{"create":reviewed_body_schema(ReviewedMutation::CreateBankDraft),"submit":reviewed_body_schema(ReviewedMutation::Submit("id")),"reject":reviewed_body_schema(ReviewedMutation::Reject("id")),"approve":reviewed_body_schema(ReviewedMutation::ApprovePost("id"))}})
}
pub fn run(
    command: &str,
    flags: &BTreeMap<&str, &str>,
    allow_http: bool,
    confirmed: bool,
    preview_reviewed: bool,
) -> Result<Value, Failure> {
    let read = match command {
        "reviewed-bank" => Some(ReviewedRead::Queue),
        "accounting-drafts" => Some(ReviewedRead::Drafts),
        "posting-rules" => Some(ReviewedRead::Rules),
        _ => None,
    };
    let needs_id = matches!(command, "submit-draft" | "reject-draft" | "approve-draft");
    if confirmed != read.is_none()
        || preview_reviewed
        || flags.keys().any(|k| {
            !(["--server", "--cookie-file", "--company"].contains(k)
                || needs_id && *k == "--draft-id")
        })
    {
        return Err(invalid(
            "reviewed writes require --confirm; only submit/reject/approve accept --draft-id; reads accept no confirmation or selectors",
        ));
    }
    let id = flags.get("--draft-id").copied().unwrap_or("");
    if needs_id && !valid_draft_id(id) {
        return Err(invalid(
            "submit/reject/approve require bounded lowercase --draft-id",
        ));
    }
    let company = flags
        .get("--company")
        .ok_or_else(|| invalid("reviewed bookkeeping requires explicit --company"))?;
    let server = flags
        .get("--server")
        .ok_or_else(|| invalid("explicit --server required"))?;
    let cookie = flags
        .get("--cookie-file")
        .ok_or_else(|| invalid("reviewed bookkeeping requires --cookie-file"))?;
    let cookie = rentemester_client::read_cookie_file(std::path::Path::new(cookie))
        .map_err(crate::remote::client_error)?;
    let api =
        ApiClient::new(server, allow_http, Some(cookie)).map_err(crate::remote::client_error)?;
    let bytes = if read.is_none() {
        let mut bytes = Vec::new();
        std::io::stdin()
            .take(262145)
            .read_to_end(&mut bytes)
            .map_err(|_| invalid("could not read native body"))?;
        Some(bytes)
    } else {
        None
    };
    let runtime = tokio::runtime::Runtime::new()
        .map_err(|_| (1, Error::new("transport", "could not start HTTP runtime")))?;
    let value = runtime
        .block_on(async {
            if let Some(read) = read {
                api.reviewed_bookkeeping(company, read).await
            } else {
                let operation = match command {
                    "create-bank-draft" => ReviewedMutation::CreateBankDraft,
                    "submit-draft" => ReviewedMutation::Submit(id),
                    "reject-draft" => ReviewedMutation::Reject(id),
                    _ => ReviewedMutation::ApprovePost(id),
                };
                api.mutate_reviewed_bookkeeping(company, operation, bytes.unwrap(), confirmed)
                    .await
            }
        })
        .map_err(crate::remote::client_error)?;
    Ok(json!({"ok":value.get("ok").and_then(Value::as_bool).unwrap_or(true),"data":value}))
}
