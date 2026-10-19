use crate::tools::{company_schema, native_body, tool};
use rentemester_client::{
    ApiClient, Error, KnowledgeMutation, MAX_KNOWLEDGE_BODY, knowledge_body_schema,
};
use serde::Deserialize;
use serde_json::{Value, json};
pub fn accepts(s: &str) -> bool {
    matches!(
        s,
        "company_knowledge"
            | "propose_company_knowledge"
            | "review_company_knowledge"
            | "supersede_company_knowledge"
    )
}
pub fn tools() -> Vec<Value> {
    let mut result = vec![tool(
        "company_knowledge",
        "Current CompanyKnowledgeRead: full advisory context/history with provenance, half-open effective windows and conflicts. Optional asOf ISO date/explicit-zone datetime; includeProposed defaults false. No accounting effects.",
        json!({"type":"object","required":["company"],"properties":{"company":company_schema(),"asOf":{"type":"string","minLength":1,"maxLength":64},"includeProposed":{"type":"boolean"}},"additionalProperties":false}),
        true,
    )];
    for (name, action) in [
        ("propose_company_knowledge", KnowledgeMutation::Propose),
        ("review_company_knowledge", KnowledgeMutation::Review),
        ("supersede_company_knowledge", KnowledgeMutation::Supersede),
    ] {
        let mut body = native_body(knowledge_body_schema(action));
        body["maxLength"] = json!(MAX_KNOWLEDGE_BODY);
        result.push(tool(name,"Current CompanyKnowledgeManage, exact native256KiB body and inner/outer confirm. Server principal/company/state/source proof only. Proposal dedup follows source identity; review proposed only; supersede approved creates a proposed replacement needing separate review. Never changes accounting. No caller keys or automatic retries. Unknown outcome requires canonical company_knowledge history reads.",json!({"type":"object","required":["company","body","confirm"],"properties":{"company":company_schema(),"body":body,"confirm":{"const":true}},"additionalProperties":false}),false));
    }
    result
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Read {
    company: String,
    as_of: Option<String>,
    include_proposed: Option<bool>,
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
    if name == "company_knowledge" {
        let r: Read = serde_json::from_value(args)
            .map_err(|_| "knowledge requires only company/asOf?/includeProposed?")?;
        return Ok(api
            .company_knowledge(
                &r.company,
                r.as_of.as_deref(),
                r.include_proposed.unwrap_or(false),
            )
            .await);
    }
    let r: Write = serde_json::from_value(args)
        .map_err(|_| "knowledge mutation requires only company/body/confirm")?;
    let action = match name {
        "propose_company_knowledge" => KnowledgeMutation::Propose,
        "review_company_knowledge" => KnowledgeMutation::Review,
        _ => KnowledgeMutation::Supersede,
    };
    Ok(api
        .mutate_company_knowledge(&r.company, action, r.body.into_bytes(), r.confirm)
        .await)
}
