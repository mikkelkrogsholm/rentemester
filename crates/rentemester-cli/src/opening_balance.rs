//! Explicit read/plan/apply; no generated source facts, actor, key or retry.
use rentemester_client::{ApiClient, MAX_OPENING_BALANCE_BODY};
use rentemester_engine::Error;
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::Read};
pub const COMMANDS: [&str; 3] = [
    "opening-balance",
    "opening-balance-plan",
    "opening-balance-apply",
];
pub fn accepts(name: &str) -> bool {
    COMMANDS.contains(&name)
}
pub fn help() -> Value {
    json!({"commands":COMMANDS,"usage":"Explicit --server/--cookie-file/--company only. opening-balance reads current attested opening; opening-balance-plan reads exact64KiB native stdin Input, readonly with no --confirm; opening-balance-apply requires bare --confirm plus exact {input,planHash,idempotencyKey,confirm:true}. Review server plan/account amounts/cutover/citations first. Server owns current human authority/atomic entry/proof/receipt, no inferred archive amounts, actor/defaults/keys/retry. Read canonical opening/journal after outcome_unknown; retain exact original input/hash/key for deliberate authorized receipt recovery. No year/account/id/source/path selectors.","planSchema":rentemester_client::opening_balance_body_schema(false),"applySchema":rentemester_client::opening_balance_body_schema(true)})
}
pub fn run(
    command: &str,
    flags: &BTreeMap<&str, &str>,
    allow_http: bool,
    confirmed: bool,
    preview: bool,
) -> Result<Value, (u8, Error)> {
    let invalid = |m| (2, Error::new("invalid_input", m));
    let apply = command == "opening-balance-apply";
    if confirmed != apply
        || preview
        || flags
            .keys()
            .any(|k| !["--server", "--cookie-file", "--company"].contains(k))
    {
        return Err(invalid(
            "exact opening selectors only; confirmation belongs exclusively to apply",
        ));
    }
    let required = |k| {
        flags
            .get(k)
            .copied()
            .ok_or_else(|| invalid("explicit server/cookie/company required"))
    };
    let cookie =
        rentemester_client::read_cookie_file(std::path::Path::new(required("--cookie-file")?))
            .map_err(crate::remote::client_error)?;
    let api = ApiClient::new(required("--server")?, allow_http, Some(cookie))
        .map_err(crate::remote::client_error)?;
    let company = required("--company")?;
    let rt = tokio::runtime::Runtime::new()
        .map_err(|_| (1, Error::new("transport", "could not start HTTP runtime")))?;
    let value = if command == "opening-balance" {
        rt.block_on(api.opening_balance(company))
    } else {
        let mut bytes = vec![];
        std::io::stdin()
            .take(MAX_OPENING_BALANCE_BODY as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| invalid("could not read native opening JSON"))?;
        if apply {
            rt.block_on(api.opening_balance_apply(company, bytes, confirmed))
        } else {
            rt.block_on(api.opening_balance_plan(company, bytes))
        }
    }
    .map_err(crate::remote::client_error)?;
    Ok(json!({"ok":value["ok"].as_bool().unwrap_or(false),"data":value}))
}
