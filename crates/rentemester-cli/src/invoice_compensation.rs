use rentemester_client::{
    ApiClient, InvoiceCompensationOperation as Op, MAX_INVOICE_COMPENSATION_BODY,
};
use rentemester_engine::Error;
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::Read};
type Failure = (u8, Error);
pub const COMMANDS: [&str; 3] = [
    "calculate-invoice-compensation",
    "register-invoice-compensation",
    "post-invoice-compensation",
];
pub fn accepts(name: &str) -> bool {
    COMMANDS.contains(&name)
}
fn invalid() -> Failure {
    (
        2,
        Error::new(
            "invalid_input",
            "compensation requires explicit company/server/private cookie and exact 16KiB JSON stdin; writes additionally require --confirm; no actor, service credential, preview or body path",
        ),
    )
}
pub fn help() -> Value {
    json!({"commands":COMMANDS,"usage":"remote calculate-invoice-compensation|register-invoice-compensation|post-invoice-compensation --company SLUG --server ORIGIN --cookie-file FILE [--allow-loopback-http]; writes additionally --confirm; exact JSON stdin","policy":"Current human MFA: CompanyRead calculate, CompanyDraftWrite register, CompanyLedgerPost post. Separate registration/posting; explicit compatible accounts, claims DKK and principal in invoice currency. No email, inferred accounts, keys or automatic retry. Inspect canonical invoice-reminder-claims and calculation after unknown writes before manual resolution.","bodySchemas":{"calculate":rentemester_client::invoice_compensation_body_schema(Op::Calculate),"register":rentemester_client::invoice_compensation_body_schema(Op::Register),"post":rentemester_client::invoice_compensation_body_schema(Op::Post)}})
}
pub fn run(
    command: &str,
    flags: &BTreeMap<&str, &str>,
    allow: bool,
    confirm: bool,
    preview: bool,
) -> Result<Value, Failure> {
    let op = match command {
        "calculate-invoice-compensation" => Op::Calculate,
        "register-invoice-compensation" => Op::Register,
        "post-invoice-compensation" => Op::Post,
        _ => return Err(invalid()),
    };
    if preview
        || confirm == (op == Op::Calculate)
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
        .take(MAX_INVOICE_COMPENSATION_BODY as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| invalid())?;
    let rt = tokio::runtime::Runtime::new().map_err(|_| invalid())?;
    let data = rt
        .block_on(api.invoice_compensation(company, op, bytes, confirm))
        .map_err(crate::remote::client_error)?;
    Ok(json!({"ok":data.get("ok").and_then(Value::as_bool).unwrap_or(true),"data":data}))
}
