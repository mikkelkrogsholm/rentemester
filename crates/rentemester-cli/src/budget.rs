use rentemester_client::{ApiClient, MAX_BUDGET_BODY, budget_body_schema};
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
        "budget" | "budget-vs-actual" | "budget-dimension-actuals" | "set-budget"
    )
}
pub fn help() -> Value {
    json!({"usage":"budget/budget-vs-actual/budget-dimension-actuals --company SLUG --year EXACT_FISCAL_LABEL; dimension comparison additionally --dimension ID [--member ID], or no selection. set-budget --company SLUG --confirm < native.json, no year/selectors. Explicit server/private cookie required.","policy":"CompanyRead reads, CompanyAdmin revisions. Exact native16KiB {accountNo,period,amount,notes?} has NO inner confirm or keys. Server chooses latest immutable revision, actuals and dimension evidence: preserve null dimension budget/variance, account context and all notes. No inferred dimension allocation, arithmetic, journal effects or automatic retries; unknown revision outcome requires canonical budget reads before further action. Read year from existing native fiscalYears/selectedYear; no guessed calendar labels.","bodySchema":budget_body_schema()})
}
pub fn run(
    command: &str,
    flags: &BTreeMap<&str, &str>,
    allow: bool,
    confirm: bool,
    preview: bool,
) -> Result<Value, Failure> {
    let read = command != "set-budget";
    let scoped = command == "budget-dimension-actuals";
    if confirm == read
        || preview
        || flags.keys().any(|k| {
            !(["--server", "--cookie-file", "--company"].contains(k)
                || read && *k == "--year"
                || scoped && ["--dimension", "--member"].contains(k))
        })
        || flags.contains_key("--member") && !flags.contains_key("--dimension")
    {
        return Err(invalid(
            "budget reads require explicit --year, dimension/member only on dimension comparison; set-budget requires --confirm and exact native stdin without year",
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
        .ok_or_else(|| invalid("budget requires --company"))?;
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
            .take(MAX_BUDGET_BODY as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| invalid("could not read budget body"))?;
    }
    let rt = tokio::runtime::Runtime::new().map_err(|_| invalid("could not start HTTP runtime"))?;
    let result = rt
        .block_on(async {
            match command {
                "budget" => api.budget(company, year.unwrap()).await,
                "budget-vs-actual" => api.budget_vs_actual(company, year.unwrap()).await,
                "budget-dimension-actuals" => {
                    api.budget_dimension_actuals(
                        company,
                        year.unwrap(),
                        flags.get("--dimension").copied(),
                        flags.get("--member").copied(),
                    )
                    .await
                }
                _ => api.set_budget(company, bytes, confirm).await,
            }
        })
        .map_err(crate::remote::client_error)?;
    Ok(json!({"ok":result.get("ok").and_then(Value::as_bool).unwrap_or(true),"data":result}))
}
