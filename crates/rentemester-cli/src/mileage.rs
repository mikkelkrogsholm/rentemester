use rentemester_client::{ApiClient, MAX_MILEAGE_BODY, mileage_body_schema};
use rentemester_engine::Error;
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::Read};
type Failure = (u8, Error);
fn invalid(s: &str) -> Failure {
    (2, Error::new("invalid_input", s))
}
pub fn accepts(s: &str) -> bool {
    matches!(s, "mileage" | "register-mileage")
}
pub fn help() -> Value {
    json!({"usage":"mileage --company SLUG --year EXACT_FISCAL_LABEL; register-mileage --company SLUG --confirm < native.json (native32KiB), no year selector. Explicit server/private cookie required.","policy":"CompanyRead reads, CompanyLedgerPost registration. Exact confirmed route/driver/vehicle/kilometers/ratePerKm/rateBasis supplied by caller; server owns precision, amount and advisories. No default/official rate selection or tax eligibility calculation, journal posting, caller keys, deduplication or automatic retries. Unknown outcomes require canonical mileage entry reads. Full fiscal/month reports returned unchanged.","bodySchema":mileage_body_schema()})
}
pub fn run(
    command: &str,
    flags: &BTreeMap<&str, &str>,
    allow: bool,
    confirm: bool,
    preview: bool,
) -> Result<Value, Failure> {
    let read = command == "mileage";
    if confirm == read
        || preview
        || flags.keys().any(|k| {
            !(["--server", "--cookie-file", "--company"].contains(k) || read && *k == "--year")
        })
    {
        return Err(invalid(
            "mileage read requires --year without confirmation; register requires --confirm and exact stdin, no selectors",
        ));
    }
    let year = if read {
        Some(
            *flags
                .get("--year")
                .ok_or_else(|| invalid("explicit --year required"))?,
        )
    } else {
        None
    };
    let company = flags
        .get("--company")
        .ok_or_else(|| invalid("mileage requires --company"))?;
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
            .take(MAX_MILEAGE_BODY as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| invalid("could not read mileage body"))?;
    }
    let rt = tokio::runtime::Runtime::new().map_err(|_| invalid("could not start HTTP runtime"))?;
    let result = rt
        .block_on(async {
            if read {
                api.mileage(company, year.unwrap()).await
            } else {
                api.register_mileage(company, bytes, confirm).await
            }
        })
        .map_err(crate::remote::client_error)?;
    Ok(json!({"ok":result.get("ok").and_then(Value::as_bool).unwrap_or(true),"data":result}))
}
