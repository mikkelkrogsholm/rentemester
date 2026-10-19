//! Existing journal reconciliation over server-owned plans and immutable archived source proof.
use rentemester_client::{ApiClient, MAX_BANK_RECONCILIATION_BODY};
use rentemester_engine::Error;
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::Read};
type Failure = (u8, Error);
fn invalid(s: &str) -> Failure {
    (2, Error::new("invalid_input", s))
}
pub fn run(
    command: &str,
    flags: &BTreeMap<&str, &str>,
    allow_http: bool,
    confirmed: bool,
    preview: bool,
) -> Result<Value, Failure> {
    let read = command == "bank-reconciliation-candidates";
    if confirmed == read
        || preview
        || flags.keys().any(|k| {
            !(["--server", "--cookie-file", "--company"].contains(k)
                || read && *k == "--bank-transaction-id")
        })
    {
        return Err(invalid(
            "candidates read requires bank-transaction-id without confirmation; link requires --confirm and exact native stdin body, no other selectors",
        ));
    }
    let bank_id = if read {
        flags
            .get("--bank-transaction-id")
            .and_then(|s| s.parse::<i64>().ok())
            .filter(|n| (1..=9_007_199_254_740_991).contains(n))
            .ok_or_else(|| invalid("candidates require positive exact --bank-transaction-id"))?
    } else {
        0
    };
    let company = flags
        .get("--company")
        .ok_or_else(|| invalid("bank reconciliation requires --company"))?;
    let server = flags
        .get("--server")
        .ok_or_else(|| invalid("explicit --server required"))?;
    let cookie = flags
        .get("--cookie-file")
        .ok_or_else(|| invalid("bank reconciliation requires --cookie-file"))?;
    let cookie = rentemester_client::read_cookie_file(std::path::Path::new(cookie))
        .map_err(crate::remote::client_error)?;
    let api =
        ApiClient::new(server, allow_http, Some(cookie)).map_err(crate::remote::client_error)?;
    let bytes = if read {
        None
    } else {
        let mut b = vec![];
        std::io::stdin()
            .take(MAX_BANK_RECONCILIATION_BODY as u64 + 1)
            .read_to_end(&mut b)
            .map_err(|_| invalid("could not read exact native reconciliation body"))?;
        Some(b)
    };
    let runtime = tokio::runtime::Runtime::new()
        .map_err(|_| (1, Error::new("transport", "could not start HTTP runtime")))?;
    let value = runtime
        .block_on(async {
            if read {
                api.bank_reconciliation_candidates(company, bank_id).await
            } else {
                api.link_bank_journal(company, bytes.unwrap(), confirmed)
                    .await
            }
        })
        .map_err(crate::remote::client_error)?;
    Ok(json!({"ok":value.get("ok").and_then(Value::as_bool).unwrap_or(true),"data":value}))
}
