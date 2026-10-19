use rentemester_client::{AccrualMutation, ApiClient, MAX_ACCRUAL_BODY, accrual_body_schema};
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
        "accruals" | "accrual-schedule" | "register-accrual" | "recognize-accrual"
    )
}
pub fn help() -> Value {
    json!({"usage":"accruals --company SLUG; accrual-schedule --company SLUG --id ACCRUAL_ID; register-accrual --company SLUG --confirm < native.json; recognize-accrual additionally --id ACCRUAL_ID. Explicit --server/private --cookie-file required. Native max256KiB, no year/query selectors.","policy":"CompanyRead for reads, CompanyLedgerPost for writes. Register verifies source original and current explicit result/balance/settlement account choices, creates a registration journal immediately. Three server types: prepaid_expense/accrued_expense/deferred_revenue. Server computes entire schedule/remaining balance. Read accrual-schedule and explicitly select periodIndex/transactionDate; later open posting date allowed but cannot precede scheduled date. accrued_expense recognition requires settlementAccountNo. Duplicate recognition refuses; registration has no dedup promise. No caller keys, client math, automatic recognition or retries. On unknown read accruals/schedule/journal/reports/documents before further action.","bodySchemas":{"register":accrual_body_schema(AccrualMutation::Register),"recognize":accrual_body_schema(AccrualMutation::Recognize(1))}})
}
pub fn run(
    command: &str,
    flags: &BTreeMap<&str, &str>,
    allow: bool,
    confirmed: bool,
    preview: bool,
) -> Result<Value, Failure> {
    let read = matches!(command, "accruals" | "accrual-schedule");
    let needs_id = matches!(command, "accrual-schedule" | "recognize-accrual");
    if confirmed == read
        || preview
        || flags.keys().any(|k| {
            !(["--server", "--cookie-file", "--company"].contains(k) || needs_id && *k == "--id")
        })
    {
        return Err(invalid(
            "accrual reads have no confirmation; register/recognize require --confirm; next/recognize require --id only",
        ));
    }
    let id = if needs_id {
        flags
            .get("--id")
            .and_then(|s| s.parse::<i64>().ok())
            .filter(|n| (1..=9_007_199_254_740_991).contains(n))
            .ok_or_else(|| invalid("positive exact --id required"))?
    } else {
        0
    };
    let company = flags
        .get("--company")
        .ok_or_else(|| invalid("accruals require --company"))?;
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
    if !read {
        std::io::stdin()
            .take(MAX_ACCRUAL_BODY as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| invalid("could not read exact accrual body"))?;
    }
    let runtime =
        tokio::runtime::Runtime::new().map_err(|_| invalid("could not start HTTP runtime"))?;
    let result = runtime
        .block_on(async {
            match command {
                "accruals" => api.accruals(company).await,
                "accrual-schedule" => api.accrual_schedule(company, id).await,
                _ => {
                    api.accrual_mutation(
                        company,
                        if command == "register-accrual" {
                            AccrualMutation::Register
                        } else {
                            AccrualMutation::Recognize(id)
                        },
                        bytes,
                        confirmed,
                    )
                    .await
                }
            }
        })
        .map_err(crate::remote::client_error)?;
    Ok(json!({"ok":result.get("ok").and_then(Value::as_bool).unwrap_or(true),"data":result}))
}
