use rentemester_client::{ApiClient, MAX_COMPANY_CREATION_BODY, company_creation_body_schema};
use rentemester_engine::Error;
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::Read};
fn invalid(s: &str) -> (u8, Error) {
    (2, Error::new("invalid_input", s))
}
pub fn help() -> Value {
    json!({"usage":"create-company --server ORIGIN --cookie-file PRIVATE_FILE --confirm < exact-native.json. No --company/year/id selectors or inner confirm; native16KiB.","policy":"Current WorkspaceManage and verified session required. Creates a fresh separate ledger and explicit creator owner membership. Server owns slug/calendar/VAT defaults and current namespace checks; no copied chart, balances or company data. No invented actor/key/defaults or automatic retry. Unknown publication outcome requires companies/current profile reads and operator reconciliation before another creation; duplicate refusal is not a safe retry receipt.","bodySchema":company_creation_body_schema()})
}
pub fn run(
    flags: &BTreeMap<&str, &str>,
    allow: bool,
    confirmed: bool,
    preview: bool,
) -> Result<Value, (u8, Error)> {
    if !confirmed
        || preview
        || flags
            .keys()
            .any(|k| !["--server", "--cookie-file"].contains(k))
    {
        return Err(invalid(
            "create-company requires --confirm and exact stdin; no company or other selectors",
        ));
    }
    let server = flags
        .get("--server")
        .ok_or_else(|| invalid("explicit --server required"))?;
    let cookie = flags
        .get("--cookie-file")
        .ok_or_else(|| invalid("private --cookie-file required"))?;
    let cookie = rentemester_client::read_cookie_file(std::path::Path::new(cookie))
        .map_err(crate::remote::client_error)?;
    let api = ApiClient::new(server, allow, Some(cookie)).map_err(crate::remote::client_error)?;
    let mut bytes = vec![];
    std::io::stdin()
        .take(MAX_COMPANY_CREATION_BODY as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| invalid("could not read exact creation body"))?;
    let runtime =
        tokio::runtime::Runtime::new().map_err(|_| invalid("could not start HTTP runtime"))?;
    let value = runtime
        .block_on(api.create_company(bytes, true))
        .map_err(crate::remote::client_error)?;
    Ok(json!({"ok":value.get("ok").and_then(Value::as_bool).unwrap_or(true),"data":value}))
}
