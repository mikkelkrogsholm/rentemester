use rentemester_client::{ApiClient, MAX_REMINDER_CLAIM_BODY, ReminderClaimMutation as Op};
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
        "invoice-reminder-claims"
            | "register-invoice-reminder"
            | "post-invoice-reminder"
            | "settle-invoice-claims"
    )
}
pub fn run(
    command: &str,
    flags: &BTreeMap<&str, &str>,
    allow: bool,
    confirm: bool,
    preview: bool,
) -> Result<Value, Failure> {
    let read = command == "invoice-reminder-claims";
    if confirm == read
        || preview
        || flags.keys().any(|k| {
            !(["--server", "--cookie-file", "--company"].contains(k) || read && *k == "--id")
        })
    {
        return Err(invalid(
            "claim read requires --id without confirmation; writes require --confirm and exact native stdin only",
        ));
    }
    let company = flags
        .get("--company")
        .ok_or_else(|| invalid("explicit --company required"))?;
    let server = flags
        .get("--server")
        .ok_or_else(|| invalid("explicit --server required"))?;
    let cookie = flags
        .get("--cookie-file")
        .ok_or_else(|| invalid("private --cookie-file required"))?;
    let cookie = rentemester_client::read_cookie_file(std::path::Path::new(cookie))
        .map_err(crate::remote::client_error)?;
    let api = ApiClient::new(server, allow, Some(cookie)).map_err(crate::remote::client_error)?;
    let id = if read {
        flags
            .get("--id")
            .and_then(|s| s.parse::<i64>().ok())
            .filter(|n| (1..=9007199254740991).contains(n))
            .ok_or_else(|| invalid("positive exact --id required"))?
    } else {
        0
    };
    let mut bytes = vec![];
    if !read {
        std::io::stdin()
            .take(MAX_REMINDER_CLAIM_BODY as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| invalid("could not read exact claim body"))?;
    }
    let rt = tokio::runtime::Runtime::new().map_err(|_| invalid("could not start HTTP runtime"))?;
    let out = rt
        .block_on(async {
            if read {
                api.invoice_reminder_claims(company, id).await
            } else {
                api.reminder_claim_mutation(
                    company,
                    match command {
                        "register-invoice-reminder" => Op::Register,
                        "post-invoice-reminder" => Op::Post,
                        _ => Op::Settle,
                    },
                    bytes,
                    confirm,
                )
                .await
            }
        })
        .map_err(crate::remote::client_error)?;
    Ok(json!({"ok":out.get("ok").and_then(Value::as_bool).unwrap_or(true),"data":out}))
}
