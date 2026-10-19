use rentemester_client::{ApiClient, KnowledgeMutation, MAX_KNOWLEDGE_BODY, knowledge_body_schema};
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
        "company-knowledge"
            | "propose-company-knowledge"
            | "review-company-knowledge"
            | "supersede-company-knowledge"
    )
}
pub fn help() -> Value {
    json!({"usage":"company-knowledge --company SLUG [--as-of ISO] [--include-proposed true|false]; propose-company-knowledge/review-company-knowledge/supersede-company-knowledge --company SLUG --confirm < exact-native.json. Explicit server/private MFA cookie required.","policy":"CompanyKnowledgeRead/Manage on current company. Native256KiB inner confirm. Source-backed advisory assertions never change accounting. Supersede creates proposed replacement requiring separate review; current context honors effective windows and conflicts. No actor/principal/company overrides, keys or automatic retries. Unknown writes require canonical context/history before further action.","bodySchemas":{"propose":knowledge_body_schema(KnowledgeMutation::Propose),"review":knowledge_body_schema(KnowledgeMutation::Review),"supersede":knowledge_body_schema(KnowledgeMutation::Supersede)}})
}
pub fn run(
    command: &str,
    flags: &BTreeMap<&str, &str>,
    allow: bool,
    confirm: bool,
    preview: bool,
) -> Result<Value, Failure> {
    let read = command == "company-knowledge";
    if confirm == read
        || preview
        || flags.keys().any(|k| {
            !(["--server", "--cookie-file", "--company"].contains(k)
                || read && ["--as-of", "--include-proposed"].contains(k))
        })
    {
        return Err(invalid(
            "knowledge read accepts only optional as-of/include-proposed; mutations require confirm and exact stdin without selectors",
        ));
    }
    let company = flags
        .get("--company")
        .ok_or_else(|| invalid("explicit company required"))?;
    let server = flags
        .get("--server")
        .ok_or_else(|| invalid("explicit server required"))?;
    let cookie = flags
        .get("--cookie-file")
        .ok_or_else(|| invalid("private cookie file required"))?;
    let proposed = match flags.get("--include-proposed").copied() {
        None | Some("false") => false,
        Some("true") => true,
        _ => return Err(invalid("include-proposed must be true or false")),
    };
    let cookie = rentemester_client::read_cookie_file(std::path::Path::new(cookie))
        .map_err(crate::remote::client_error)?;
    let api = ApiClient::new(server, allow, Some(cookie)).map_err(crate::remote::client_error)?;
    let mut bytes = vec![];
    if !read {
        std::io::stdin()
            .take(MAX_KNOWLEDGE_BODY as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| invalid("could not read knowledge body"))?;
    }
    let runtime =
        tokio::runtime::Runtime::new().map_err(|_| invalid("HTTP runtime unavailable"))?;
    let result = runtime
        .block_on(async {
            if read {
                api.company_knowledge(company, flags.get("--as-of").copied(), proposed)
                    .await
            } else {
                let action = match command {
                    "propose-company-knowledge" => KnowledgeMutation::Propose,
                    "review-company-knowledge" => KnowledgeMutation::Review,
                    _ => KnowledgeMutation::Supersede,
                };
                api.mutate_company_knowledge(company, action, bytes, confirm)
                    .await
            }
        })
        .map_err(crate::remote::client_error)?;
    Ok(json!({"ok":result.get("ok").and_then(Value::as_bool).unwrap_or(true),"data":result}))
}
