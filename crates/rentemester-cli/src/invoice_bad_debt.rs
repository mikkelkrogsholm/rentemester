use rentemester_client::{ApiClient, MAX_INVOICE_BAD_DEBT_BODY};
use rentemester_engine::Error;
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::Read};
type Failure = (u8, Error);
pub const COMMANDS: [&str; 1] = ["write-off-invoice-bad-debt"];
pub fn accepts(name: &str) -> bool {
    COMMANDS.contains(&name)
}
fn invalid() -> Failure {
    (
        2,
        Error::new(
            "invalid_input",
            "bad-debt write-off requires explicit company/server/private cookie and exact 16KiB JSON stdin; writes additionally require --confirm; no actor, service credential, preview or body path",
        ),
    )
}
pub fn help() -> Value {
    json!({"commands":COMMANDS,"usage":"remote write-off-invoice-bad-debt --company SLUG --server ORIGIN --cookie-file PRIVATE_FILE [--allow-loopback-http] --confirm; exact16KiB JSON stdin","policy":"Current human MFA CompanyLedgerPost. Explicit established documented loss and expense account. grossAmount is ORIGINAL invoice currency; omit for current open principal. Server derives immutable debtor/VAT accounts, dated carrying, VAT allocation and all gates. Claims stay collectible separately. No recovery-of-written-off-debt operation, email, actor, idempotency keys, local body path or automatic retry. Unknown write requires canonical invoice-reminder-claims/invoice/journal reads and manual reconciliation.","bodySchema":rentemester_client::invoice_bad_debt_body_schema()})
}
pub fn run(
    command: &str,
    flags: &BTreeMap<&str, &str>,
    allow: bool,
    confirm: bool,
    preview: bool,
) -> Result<Value, Failure> {
    if !accepts(command)
        || preview
        || !confirm
        || flags
            .keys()
            .any(|k| !["--company", "--server", "--cookie-file"].contains(k))
    {
        return Err(invalid());
    }
    let company = flags.get("--company").ok_or_else(invalid)?;
    let server = flags.get("--server").ok_or_else(invalid)?;
    let cookie = flags.get("--cookie-file").ok_or_else(invalid)?;
    let cookie = rentemester_client::read_cookie_file(std::path::Path::new(cookie))
        .map_err(crate::remote::client_error)?;
    let api = ApiClient::new(server, allow, Some(cookie)).map_err(crate::remote::client_error)?;
    let mut bytes = Vec::new();
    std::io::stdin()
        .take(MAX_INVOICE_BAD_DEBT_BODY as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| invalid())?;
    let rt = tokio::runtime::Runtime::new().map_err(|_| invalid())?;
    let data = rt
        .block_on(api.invoice_bad_debt(company, bytes, confirm))
        .map_err(crate::remote::client_error)?;
    Ok(json!({"ok":data.get("ok").and_then(Value::as_bool).unwrap_or(true),"data":data}))
}
