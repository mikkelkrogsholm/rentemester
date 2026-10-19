//! Explicit current-chain planning and keyed evidence supersession; no accounting or automatic retries.
use rentemester_client::{ApiClient, Error as ClientError, MAX_BANK_CORRECTION_BODY};
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
    let read = command == "bank-reconciliation-correction-plan";
    if confirmed == read
        || preview
        || flags.keys().any(|k| {
            !(["--server", "--cookie-file", "--company"].contains(k)
                || read && ["--bank-transaction-id", "--replacement-journal-entry-id"].contains(k))
        })
    {
        return Err(invalid(
            "correction-plan requires explicit bank/replacement identifiers without confirmation; correction requires --confirm and exact native stdin JSON, no selectors",
        ));
    }
    let parse_id = |key| {
        flags.get(key).and_then(|s|s.parse::<i64>().ok()).filter(|n|(1..=9_007_199_254_740_991).contains(n)).ok_or_else(||invalid("correction-plan requires positive exact --bank-transaction-id and --replacement-journal-entry-id"))
    };
    let (bank, replacement) = if read {
        (
            parse_id("--bank-transaction-id")?,
            parse_id("--replacement-journal-entry-id")?,
        )
    } else {
        (0, 0)
    };
    let company = flags
        .get("--company")
        .ok_or_else(|| invalid("bank correction requires --company"))?;
    let server = flags
        .get("--server")
        .ok_or_else(|| invalid("explicit --server required"))?;
    let cookie = flags
        .get("--cookie-file")
        .ok_or_else(|| invalid("bank correction requires --cookie-file"))?;
    let cookie = rentemester_client::read_cookie_file(std::path::Path::new(cookie))
        .map_err(crate::remote::client_error)?;
    let api =
        ApiClient::new(server, allow_http, Some(cookie)).map_err(crate::remote::client_error)?;
    let bytes = if read {
        None
    } else {
        let mut b = vec![];
        std::io::stdin()
            .take(MAX_BANK_CORRECTION_BODY as u64 + 1)
            .read_to_end(&mut b)
            .map_err(|_| invalid("could not read exact native correction JSON"))?;
        Some(b)
    };
    let key = bytes
        .as_ref()
        .filter(|b| b.len() <= MAX_BANK_CORRECTION_BODY)
        .and_then(|b| serde_json::from_slice::<Value>(b).ok())
        .and_then(|v| v["idempotencyKey"].as_str().map(str::to_owned));
    let runtime = tokio::runtime::Runtime::new()
        .map_err(|_| (1, Error::new("transport", "could not start HTTP runtime")))?;
    let outcome = runtime.block_on(async {
        if read {
            api.bank_reconciliation_correction_plan(company, bank, replacement)
                .await
        } else {
            api.correct_bank_reconciliation(company, bytes.unwrap(), confirmed)
                .await
        }
    });
    if outcome == Err(ClientError::OutcomeUnknown) {
        return Ok(
            json!({"ok":false,"code":"outcome_unknown","idempotencyKey":key,"errors":["Correction outcome unknown; inspect canonical bank/history and preserve original key/body/current principal before any explicit replay. Never retry automatically."]}),
        );
    }
    let value = outcome.map_err(crate::remote::client_error)?;
    Ok(json!({"ok":value.get("ok").and_then(Value::as_bool).unwrap_or(true),"data":value}))
}
