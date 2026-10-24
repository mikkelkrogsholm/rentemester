use crate::tools::{company_schema, native_body, tool};
use rentemester_client::{ApiClient, Error, MAX_PARTY_BODY, PartyMutation, party_body_schema};
use serde::Deserialize;
use serde_json::{Value, json};
pub fn accepts(s: &str) -> bool {
    matches!(
        s,
        "workspace_parties"
            | "workspace_party"
            | "create_workspace_party"
            | "link_workspace_party_role"
            | "assert_workspace_party_field"
            | "assert_workspace_party_alias"
            | "propose_workspace_party_merge"
            | "approve_workspace_party_merge"
    )
}
pub fn tools() -> Vec<Value> {
    let mut tools = vec![
        tool(
            "workspace_parties",
            "Current CompanyRead: full live readable-company projection before pagination, hidden parties/defaults/association events excluded. Canonical original identity and full source assertion history only; no accounting effects.",
            json!({"type":"object","required":["company"],"properties":{"company":company_schema(),"query":{"type":"string","maxLength":512},"cursor":{"type":"integer","minimum":0},"limit":{"type":"integer","minimum":1,"maximum":100}},"additionalProperties":false}),
            true,
        ),
        tool(
            "workspace_party",
            "Inspect exact visible canonical party and provenance/source assertion history. No unfiltered cross-company defaults/association events, current company-set authorization required.",
            json!({"type":"object","required":["company","partyId"],"properties":{"company":company_schema(),"partyId":{"type":"string","minLength":1,"maxLength":64}},"additionalProperties":false}),
            true,
        ),
    ];
    for (name, action, id) in [
        ("create_workspace_party", PartyMutation::Create, false),
        ("propose_workspace_party_merge", PartyMutation::ProposeMerge(String::new()), true),
        ("approve_workspace_party_merge", PartyMutation::ApproveMerge(String::new()), true),
        (
            "link_workspace_party_role",
            PartyMutation::Role(String::new()),
            true,
        ),
        (
            "assert_workspace_party_field",
            PartyMutation::Assertion(String::new()),
            true,
        ),
        (
            "assert_workspace_party_alias",
            PartyMutation::Alias(String::new()),
            true,
        ),
    ] {
        let mut body = native_body(party_body_schema(&action));
        body["maxLength"] = json!(MAX_PARTY_BODY);
        let mut schema = json!({"type":"object","required":["company","body","confirm"],"properties":{"company":company_schema(),"body":body,"confirm":{"const":true}},"additionalProperties":false});
        if id {
            schema["properties"]["partyId"] = json!({"type":"string","minLength":1,"maxLength":64});
            schema["required"]
                .as_array_mut()
                .unwrap()
                .push(json!("partyId"));
        }
        tools.push(tool(name,"Exact native256KiB once, inner/outer confirm. Current anchor CompanyMasterData; global field/alias assertions additionally require MasterData for EVERY associated company (archived/hidden refuses). Immutable canonical identity plus source provenance; no automatic accounting/contacts updates or inferred defaults, keys or retries. Unknown outcomes require canonical workspace_party/list/history reads.",schema,false));
    }
    tools
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct List {
    company: String,
    query: Option<String>,
    cursor: Option<usize>,
    limit: Option<usize>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Read {
    company: String,
    party_id: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Write {
    company: String,
    party_id: Option<String>,
    body: String,
    confirm: bool,
}
pub async fn call(
    api: &ApiClient,
    name: &str,
    args: Value,
) -> Result<Result<Value, Error>, &'static str> {
    if name == "workspace_parties" {
        let r: List = serde_json::from_value(args)
            .map_err(|_| "workspace_parties requires only company/query?/cursor?/limit?")?;
        return Ok(api
            .workspace_parties(
                &r.company,
                r.query.as_deref(),
                r.cursor.unwrap_or(0),
                r.limit.unwrap_or(25),
            )
            .await);
    }
    if name == "workspace_party" {
        let r: Read = serde_json::from_value(args)
            .map_err(|_| "workspace_party requires only company/partyId")?;
        return Ok(api.workspace_party(&r.company, &r.party_id).await);
    }
    let r: Write = serde_json::from_value(args).map_err(
        |_| "party write requires exact company/body/confirm and partyId for existing party",
    )?;
    let action = match (name, r.party_id) {
        ("create_workspace_party", None) => PartyMutation::Create,
        ("link_workspace_party_role", Some(id)) => PartyMutation::Role(id),
        ("assert_workspace_party_field", Some(id)) => PartyMutation::Assertion(id),
        ("assert_workspace_party_alias", Some(id)) => PartyMutation::Alias(id),
        ("propose_workspace_party_merge", Some(id)) => PartyMutation::ProposeMerge(id),
        ("approve_workspace_party_merge", Some(id)) => PartyMutation::ApproveMerge(id),
        _ => return Err("partyId required only for existing party mutation"),
    };
    Ok(api
        .mutate_workspace_party(&r.company, action, r.body.into_bytes(), r.confirm)
        .await)
}
