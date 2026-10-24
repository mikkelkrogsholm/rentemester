use rentemester_client::{ApiClient, MAX_PARTY_BODY, PartyMutation, party_body_schema};
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
        "workspace-parties"
            | "workspace-party"
            | "create-workspace-party"
            | "link-workspace-party-role"
            | "assert-workspace-party-field"
            | "assert-workspace-party-alias"
            | "propose-workspace-party-merge"
            | "approve-workspace-party-merge"
    )
}
pub fn help() -> Value {
    json!({"usage":"workspace-parties --company SLUG [--query TEXT] [--cursor N] [--limit 1..100]; workspace-party --company SLUG --party-id ID. create-workspace-party --company SLUG --confirm < native.json; link-workspace-party-role/assert-workspace-party-field/assert-workspace-party-alias additionally --party-id ID. Explicit server/private MFA cookie required.","policy":"CompanyRead visible full company-set projection before pagination. Create and role use current anchor MasterData; global field/alias assertions require current MasterData for ALL associated companies and an existing visible association. Archived/hidden associations refuse, no owner bypass. Immutable identity plus append-only source assertions; never updates accounting/contacts or infers defaults. Explicit propose-workspace-party-merge/approve-workspace-party-merge use --party-id and confirmed native JSON; both full writable party scopes required. No keys or automatic retry. Unknown outcomes require canonical party/list/history reads.","bodySchemas":{"proposeMerge":party_body_schema(&PartyMutation::ProposeMerge(String::new())),"approveMerge":party_body_schema(&PartyMutation::ApproveMerge(String::new())),"create":party_body_schema(&PartyMutation::Create),"role":party_body_schema(&PartyMutation::Role(String::new())),"assertion":party_body_schema(&PartyMutation::Assertion(String::new())),"alias":party_body_schema(&PartyMutation::Alias(String::new()))}})
}
pub fn run(
    command: &str,
    flags: &BTreeMap<&str, &str>,
    allow: bool,
    confirm: bool,
    preview: bool,
) -> Result<Value, Failure> {
    let list = command == "workspace-parties";
    let read = list || command == "workspace-party";
    let id = command != "workspace-parties" && command != "create-workspace-party";
    if confirm == read
        || preview
        || flags.keys().any(|k| {
            !(["--server", "--cookie-file", "--company"].contains(k)
                || id && *k == "--party-id"
                || list && ["--query", "--cursor", "--limit"].contains(k))
        })
    {
        return Err(invalid(
            "party selectors/body/confirmation do not match command; see remote help",
        ));
    }
    let id = if id {
        Some(
            *flags
                .get("--party-id")
                .ok_or_else(|| invalid("explicit party-id required"))?,
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
            .take(MAX_PARTY_BODY as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| invalid("could not read party body"))?;
    }
    let rt = tokio::runtime::Runtime::new().map_err(|_| invalid("HTTP runtime unavailable"))?;
    let result = rt
        .block_on(async {
            if list {
                api.workspace_parties(company, flags.get("--query").copied(), cursor, limit)
                    .await
            } else if read {
                api.workspace_party(company, id.unwrap()).await
            } else {
                let action = match command {
                    "create-workspace-party" => PartyMutation::Create,
                    "link-workspace-party-role" => PartyMutation::Role(id.unwrap().into()),
                    "assert-workspace-party-field" => PartyMutation::Assertion(id.unwrap().into()),
                    "propose-workspace-party-merge" => PartyMutation::ProposeMerge(id.unwrap().into()),
                    "approve-workspace-party-merge" => PartyMutation::ApproveMerge(id.unwrap().into()),
                    _ => PartyMutation::Alias(id.unwrap().into()),
                };
                api.mutate_workspace_party(company, action, bytes, confirm)
                    .await
            }
        })
        .map_err(crate::remote::client_error)?;
    Ok(json!({"ok":result.get("ok").and_then(Value::as_bool).unwrap_or(true),"data":result}))
}
