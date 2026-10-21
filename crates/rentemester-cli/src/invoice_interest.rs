use rentemester_client::{ApiClient, InvoiceInterestOperation as Op, MAX_INVOICE_INTEREST_BODY};
use rentemester_engine::Error;
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::Read};
type Failure = (u8, Error);
pub const COMMANDS: [&str; 5] = [
    "calculate-invoice-interest",
    "register-invoice-interest",
    "post-invoice-interest",
    "calculate-invoice-interest-correction",
    "post-invoice-interest-correction",
];
pub fn accepts(name: &str) -> bool {
    COMMANDS.contains(&name)
}
fn invalid() -> Failure {
    (
        2,
        Error::new(
            "invalid_input",
            "interest requires explicit company/server/private cookie and exact 16KiB JSON stdin; writes additionally require --confirm; no actor, service credential, preview or body path",
        ),
    )
}
pub fn help() -> Value {
    json!({"commands":COMMANDS,"usage":"remote COMMAND --company SLUG --server ORIGIN --cookie-file FILE [--allow-loopback-http]; exact16KiB JSON stdin, register/post/post-correction additionally --confirm","policy":"Current human MFA only. Calculate/correction CompanyRead, registration DraftWrite with no journal, posting/correction LedgerPost. Omitted referenceRatePercent uses statutory table; no client rate/account/math defaults. Current native interest is DKK; foreign interest may refuse explicitly without changing foreign principal/compensation. Correction hasProposal is mathematical only: canPost/requiresRefund/proposedLines govern the server-owned safe original-account plan; posting accepts only reason/date, never client amount/accounts/allocation. No email, keys or automatic retries. Unknown write requires canonical calculate/correction and invoice-reminder-claims before manual resolution.","bodySchemas":{"calculate":rentemester_client::invoice_interest_body_schema(Op::Calculate),"register":rentemester_client::invoice_interest_body_schema(Op::Register),"post":rentemester_client::invoice_interest_body_schema(Op::Post),"correction":rentemester_client::invoice_interest_body_schema(Op::Correction),"postCorrection":rentemester_client::invoice_interest_body_schema(Op::PostCorrection)}})
}
pub fn run(
    command: &str,
    flags: &BTreeMap<&str, &str>,
    allow: bool,
    confirm: bool,
    preview: bool,
) -> Result<Value, Failure> {
    let op = match command {
        "calculate-invoice-interest" => Op::Calculate,
        "register-invoice-interest" => Op::Register,
        "post-invoice-interest" => Op::Post,
        "calculate-invoice-interest-correction" => Op::Correction,
        "post-invoice-interest-correction" => Op::PostCorrection,
        _ => return Err(invalid()),
    };
    if preview
        || confirm == op.is_read_only()
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
        .take(MAX_INVOICE_INTEREST_BODY as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| invalid())?;
    let rt = tokio::runtime::Runtime::new().map_err(|_| invalid())?;
    let data = rt
        .block_on(api.invoice_interest(company, op, bytes, confirm))
        .map_err(crate::remote::client_error)?;
    Ok(json!({"ok":data.get("ok").and_then(Value::as_bool).unwrap_or(true),"data":data}))
}
