use rentemester_client::{
    ApiClient, MAX_OWNERSHIP_BODY, MAX_OWNERSHIP_PROPOSE_BODY, OwnershipMutation,
    ownership_body_schema,
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
        "ownership"
            | "ownership-history"
            | "propose-ownership"
            | "review-ownership"
            | "apply-ownership"
    )
}
pub fn help() -> Value {
    json!({"usage":"ownership --company ANCHOR --as-of YYYY-MM-DD; ownership-history --company ANCHOR [--snapshot-id ID] [--as-of YYYY-MM-DD]; propose-ownership/review-ownership/apply-ownership --company ANCHOR --confirm < exact-native.json. Explicit server/private MFA cookie required.","policy":"Server-owned current endpoint authority, source/evidence, graph arithmetic and effective dates. Propose2MiB, review/apply32KiB native confirmed body. Review requires a different authenticated principal; apply uses exact canonical snapshotHash/diffHash after approval. Later explicit reviewed snapshot may end an earlier fact; omissions never revoke. No ownership inference, generated IDs/hashes/keys, automatic retry, accounting or consolidation-manifest changes. Unknown outcomes require canonical snapshot/history/as-of reads before further explicit action.","bodySchemas":{"propose":ownership_body_schema(OwnershipMutation::Propose),"review":ownership_body_schema(OwnershipMutation::Review),"apply":ownership_body_schema(OwnershipMutation::Apply)}})
}
pub fn run(
    command: &str,
    flags: &BTreeMap<&str, &str>,
    allow: bool,
    confirm: bool,
    preview: bool,
) -> Result<Value, Failure> {
    let history = command == "ownership-history";
    let read = history || command == "ownership";
    if preview
        || confirm == read
        || flags.keys().any(|k| {
            !(["--server", "--cookie-file", "--company"].contains(k)
                || read && *k == "--as-of"
                || history && *k == "--snapshot-id")
        })
    {
        return Err(invalid(
            "ownership selectors/confirmation do not match command; see remote help",
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
        .ok_or_else(|| invalid("private MFA cookie file required"))?;
    let date = flags.get("--as-of").copied();
    if command == "ownership" && date.is_none() {
        return Err(invalid("explicit as-of required"));
    }
    let cookie = rentemester_client::read_cookie_file(std::path::Path::new(cookie))
        .map_err(crate::remote::client_error)?;
    let api = ApiClient::new(server, allow, Some(cookie)).map_err(crate::remote::client_error)?;
    let action = match command {
        "propose-ownership" => OwnershipMutation::Propose,
        "review-ownership" => OwnershipMutation::Review,
        _ => OwnershipMutation::Apply,
    };
    let mut bytes = vec![];
    if !read {
        let max = if matches!(action, OwnershipMutation::Propose) {
            MAX_OWNERSHIP_PROPOSE_BODY
        } else {
            MAX_OWNERSHIP_BODY
        };
        std::io::stdin()
            .take(max as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| invalid("could not read exact ownership body"))?;
    }
    let rt = tokio::runtime::Runtime::new().map_err(|_| invalid("HTTP runtime unavailable"))?;
    let value = rt
        .block_on(async {
            if history {
                api.ownership_history(company, flags.get("--snapshot-id").copied(), date)
                    .await
            } else if read {
                api.ownership(company, date.unwrap()).await
            } else {
                api.mutate_ownership(company, action, bytes, confirm).await
            }
        })
        .map_err(crate::remote::client_error)?;
    Ok(json!({"ok":value.get("ok").and_then(Value::as_bool).unwrap_or(true),"data":value}))
}
