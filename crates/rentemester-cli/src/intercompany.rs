use rentemester_client::{
    ApiClient, IntercompanyMutation, MAX_INTERCOMPANY_BODY, intercompany_body_schema,
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
        "intercompany"
            | "group-reconciliation"
            | "apply-group-manifest"
            | "propose-intercompany-mapping"
            | "approve-intercompany-mapping"
            | "revoke-intercompany-mapping"
    )
}
pub fn help() -> Value {
    json!({"usage":"intercompany --company ANCHOR; group-reconciliation --as-of YYYY-MM-DD (global, no company selector); apply-group-manifest/propose-intercompany-mapping/approve-intercompany-mapping/revoke-intercompany-mapping --company ANCHOR --confirm < exact-native.json. Explicit server/private MFA cookie required.","policy":"Exact confirmed256KiB body once; ordinary1MiB MCP frames. Server owns V1 manifest, current all-reference permissions, mapped actual accounts, date/currency/source checks and independent authenticated approval. Approve/revoke use exact canonical mappingId/mappingHash. Reconciliation preserves decimal-string balances/differences/sourceRefs.amount, current source snapshots and blockers without calculations. No inferred ownership, generated actor/IDs/keys/defaults, automatic retry, ledger writes, FX or consolidation. After unknown outcome read current intercompany state and global reconciliation before further explicit action.","bodySchemas":{"manifest":intercompany_body_schema(IntercompanyMutation::Manifest),"propose":intercompany_body_schema(IntercompanyMutation::Propose),"approve":intercompany_body_schema(IntercompanyMutation::Approve),"revoke":intercompany_body_schema(IntercompanyMutation::Revoke)}})
}
pub fn run(
    command: &str,
    flags: &BTreeMap<&str, &str>,
    allow: bool,
    confirm: bool,
    preview: bool,
) -> Result<Value, Failure> {
    let global = command == "group-reconciliation";
    let read = global || command == "intercompany";
    if preview
        || confirm == read
        || flags.keys().any(|k| !match *k {
            "--server" | "--cookie-file" => true,
            "--as-of" => global,
            "--company" => !global,
            _ => false,
        })
    {
        return Err(invalid(
            "intercompany selectors/confirmation do not match command; see remote help",
        ));
    }
    let company = if global {
        None
    } else {
        Some(
            *flags
                .get("--company")
                .ok_or_else(|| invalid("explicit company required"))?,
        )
    };
    let date = if global {
        Some(
            *flags
                .get("--as-of")
                .ok_or_else(|| invalid("explicit as-of required"))?,
        )
    } else {
        None
    };
    let server = flags
        .get("--server")
        .ok_or_else(|| invalid("explicit server required"))?;
    let cookie = flags
        .get("--cookie-file")
        .ok_or_else(|| invalid("private MFA cookie file required"))?;
    let cookie = rentemester_client::read_cookie_file(std::path::Path::new(cookie))
        .map_err(crate::remote::client_error)?;
    let api = ApiClient::new(server, allow, Some(cookie)).map_err(crate::remote::client_error)?;
    let action = match command {
        "apply-group-manifest" => IntercompanyMutation::Manifest,
        "propose-intercompany-mapping" => IntercompanyMutation::Propose,
        "approve-intercompany-mapping" => IntercompanyMutation::Approve,
        _ => IntercompanyMutation::Revoke,
    };
    let mut bytes = vec![];
    if !read {
        std::io::stdin()
            .take(MAX_INTERCOMPANY_BODY as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| invalid("could not read exact intercompany body"))?;
    }
    let rt = tokio::runtime::Runtime::new().map_err(|_| invalid("HTTP runtime unavailable"))?;
    let value = rt
        .block_on(async {
            if global {
                api.group_reconciliation(date.unwrap()).await
            } else if read {
                api.intercompany(company.unwrap()).await
            } else {
                api.mutate_intercompany(company.unwrap(), action, bytes, confirm)
                    .await
            }
        })
        .map_err(crate::remote::client_error)?;
    Ok(json!({"ok":value.get("ok").and_then(Value::as_bool).unwrap_or(true),"data":value}))
}
