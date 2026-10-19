//! Thin explicit plan/apply commands; no accounting, generated keys or automatic retry.
use rentemester_client::{ApiClient, Error as ClientError, MAX_PAYABLE_CORRECTION_BODY};
use rentemester_engine::Error;
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::Read};
pub fn accepts(name: &str) -> bool {
    matches!(
        name,
        "direct-bank-payable-correction-plan" | "direct-bank-payable-correction"
    )
}
pub fn help() -> Value {
    json!({"commands":["direct-bank-payable-correction-plan","direct-bank-payable-correction"],"input":"Exact native16KiB stdin JSON; plan forbids --confirm, apply requires --confirm plus inner confirm:true, reviewed planHash, reason and caller idempotencyKey. Explicit --server/--cookie-file/--company. Server owns all source checks, dates, money and atomic correction. Never retry automatically; inspect payables/journal/bank after unknown outcome.","planSchema":rentemester_client::payable_correction_body_schema(false),"applySchema":rentemester_client::payable_correction_body_schema(true)})
}
pub fn run(
    command: &str,
    flags: &BTreeMap<&str, &str>,
    allow_http: bool,
    confirmed: bool,
    preview: bool,
) -> Result<Value, (u8, Error)> {
    let invalid = |s| (2, Error::new("invalid_input", s));
    let apply = command == "direct-bank-payable-correction";
    if confirmed != apply
        || preview
        || flags
            .keys()
            .any(|k| !["--server", "--cookie-file", "--company"].contains(k))
    {
        return Err(invalid(
            "correction plan forbids confirmation; apply requires --confirm; both require exact stdin JSON without selectors",
        ));
    }
    let company = flags
        .get("--company")
        .ok_or_else(|| invalid("explicit --company required"))?;
    let server = flags
        .get("--server")
        .ok_or_else(|| invalid("explicit --server required"))?;
    let file = flags
        .get("--cookie-file")
        .ok_or_else(|| invalid("explicit --cookie-file required"))?;
    let cookie = rentemester_client::read_cookie_file(std::path::Path::new(file))
        .map_err(crate::remote::client_error)?;
    let api =
        ApiClient::new(server, allow_http, Some(cookie)).map_err(crate::remote::client_error)?;
    let mut bytes = Vec::new();
    std::io::stdin()
        .take(MAX_PAYABLE_CORRECTION_BODY as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| invalid("could not read native correction JSON"))?;
    let key = serde_json::from_slice::<Value>(&bytes)
        .ok()
        .and_then(|v| v["idempotencyKey"].as_str().map(str::to_owned));
    let runtime = tokio::runtime::Runtime::new()
        .map_err(|_| (1, Error::new("transport", "could not start HTTP runtime")))?;
    let outcome =
        runtime.block_on(api.direct_bank_payable_correction(company, bytes, apply, confirmed));
    if outcome == Err(ClientError::OutcomeUnknown) {
        return Ok(
            json!({"ok":false,"code":"outcome_unknown","idempotencyKey":key,"errors":["Correction outcome unknown; inspect canonical payables, journal and bank before any explicit action. Preserve original body/key; never retry automatically."]}),
        );
    }
    let value = outcome.map_err(crate::remote::client_error)?;
    Ok(json!({"ok":value["ok"].as_bool().unwrap_or(false),"data":value}))
}
