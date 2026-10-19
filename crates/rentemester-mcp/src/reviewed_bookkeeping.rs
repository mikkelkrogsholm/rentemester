//! Source-bound drafts and independent review through existing server authority.
use crate::tools::{company_schema, native_body, tool};
use rentemester_client::{
    ApiClient, Error, ReviewedMutation, ReviewedRead, reviewed_body_schema, valid_draft_id,
};
use serde::Deserialize;
use serde_json::{Value, json};
pub fn accepts(name: &str) -> bool {
    matches!(
        name,
        "reviewed_bank"
            | "accounting_drafts"
            | "posting_rules"
            | "create_bank_draft"
            | "submit_draft"
            | "reject_draft"
            | "approve_draft"
    )
}
pub fn tools() -> Vec<Value> {
    let mut result = vec![];
    for (name, description) in [
        (
            "reviewed_bank",
            "Read unposted source-bound queue; expectedSourceHash and advisory account hint come from actual server evidence",
        ),
        (
            "accounting_drafts",
            "Read canonical draft payload, status, eventHash and journal receipt for review and unknown-outcome reconciliation",
        ),
        (
            "posting_rules",
            "Read actual existing manual rule proposals/lifecycle; hints never authorize posting",
        ),
    ] {
        result.push(tool(name,description,json!({"type":"object","required":["company"],"properties":{"company":company_schema()},"additionalProperties":false}),true));
    }
    for (name, op, description) in [
        (
            "create_bank_draft",
            ReviewedMutation::CreateBankDraft,
            "Create one source-bound draft with caller-selected draftId, account and current expectedSourceHash; never invent bank evidence or accounts",
        ),
        (
            "submit_draft",
            ReviewedMutation::Submit("id"),
            "Submit stored draft using current expectedEventHash; exact native body may omit confirm, outer confirm remains mandatory",
        ),
        (
            "reject_draft",
            ReviewedMutation::Reject("id"),
            "Independent reviewer rejects submitted stored payload with current expectedEventHash and reason",
        ),
        (
            "approve_draft",
            ReviewedMutation::ApprovePost("id"),
            "Independent reviewer approves stored payload and posts one journal under current server rights; server rechecks evidence and effective period locks",
        ),
    ] {
        let mut schema = json!({"type":"object","required":["company","body","confirm"],"properties":{"company":company_schema(),"body":native_body(reviewed_body_schema(op)),"confirm":{"const":true}},"additionalProperties":false});
        if name != "create_bank_draft" {
            schema["required"]
                .as_array_mut()
                .unwrap()
                .push(json!("draftId"));
            schema["properties"]["draftId"] =
                json!({"type":"string","minLength":1,"maxLength":64,"pattern":"^[a-z][a-z0-9-]*$"});
        }
        result.push(tool(name,&format!("{description}. Exact body forwarded once with explicit confirmation, no actor/hash/idempotency injection or automatic retry. On outcome_unknown read accounting_drafts/reviewed_bank/journal and preserve original body/hash/actor. Only explicit same-reviewer identical approval can read a durable receipt; never resend automatically. Draft writes require company.draft.write; reject/approve require company.review and distinct author/submitter."),schema,false));
    }
    result
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Read {
    company: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Write {
    company: String,
    body: String,
    confirm: bool,
    draft_id: Option<String>,
}
pub async fn call(
    api: &ApiClient,
    name: &str,
    args: Value,
) -> Result<Result<Value, Error>, &'static str> {
    let read = match name {
        "reviewed_bank" => Some(ReviewedRead::Queue),
        "accounting_drafts" => Some(ReviewedRead::Drafts),
        "posting_rules" => Some(ReviewedRead::Rules),
        _ => None,
    };
    if let Some(read) = read {
        let input: Read =
            serde_json::from_value(args).map_err(|_| "reviewed reads require only company")?;
        return Ok(api.reviewed_bookkeeping(&input.company, read).await);
    }
    if (name == "create_bank_draft") == args.get("draftId").is_some() {
        return Err("only submit/reject/approve require explicit draftId");
    }
    let input:Write=serde_json::from_value(args).map_err(|_|"reviewed writes require company, exact native body string, confirmation and draftId for submit/reject/approve")?;
    if (name == "create_bank_draft") != input.draft_id.is_none()
        || input
            .draft_id
            .as_deref()
            .is_some_and(|s| !valid_draft_id(s))
    {
        return Err("only submit/reject/approve require explicit bounded lowercase draftId");
    }
    let id = input.draft_id.as_deref().unwrap_or("");
    let op = match name {
        "create_bank_draft" => ReviewedMutation::CreateBankDraft,
        "submit_draft" => ReviewedMutation::Submit(id),
        "reject_draft" => ReviewedMutation::Reject(id),
        "approve_draft" => ReviewedMutation::ApprovePost(id),
        _ => return Err("unknown reviewed tool"),
    };
    Ok(api
        .mutate_reviewed_bookkeeping(&input.company, op, input.body.into_bytes(), input.confirm)
        .await)
}
