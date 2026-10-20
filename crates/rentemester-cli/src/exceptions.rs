use rentemester_client::{ApiClient, MAX_EXCEPTION_BODY, exception_resolve_schema};
use rentemester_engine::Error;
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::Read};
type Failure = (u8, Error);
fn invalid(s: &str) -> Failure {
    (2, Error::new("invalid_input", s))
}
pub fn accepts(s: &str) -> bool {
    matches!(s, "list-exceptions" | "resolve-exception")
}
pub fn help() -> Value {
    json!({"usage":"list-exceptions --company SLUG [--status open|resolved|all] [--include-archived true|false]; resolve-exception --company SLUG --id POSITIVE --confirm < native-note.json. Explicit --server and private --cookie-file required.","policy":"CompanyRead lists canonical tasks; --include-archived requires a boolean value and defaults false. CompanyReview resolves one reviewed task, not its underlying bank condition. Native stdin is at most16KiB with only optional note (string/null, max4096 bytes); no inner confirm, caller actor or keys. No automatic retries: unknown outcomes require canonical queue/audit reads before further action. No journal or reconciliation effects.","bodySchema":exception_resolve_schema()})
}
pub fn run(
    command: &str,
    flags: &BTreeMap<&str, &str>,
    allow: bool,
    confirm: bool,
    preview: bool,
) -> Result<Value, Failure> {
    let read = command == "list-exceptions";
    if !accepts(command)
        || confirm == read
        || preview
        || flags.keys().any(|k| {
            !(["--server", "--cookie-file", "--company"].contains(k)
                || read && ["--status", "--include-archived"].contains(k)
                || !read && *k == "--id")
        })
    {
        return Err(invalid(
            "list-exceptions accepts status/include-archived without confirmation; resolve-exception requires id, --confirm and exact note stdin",
        ));
    }
    let status = flags.get("--status").copied();
    if status.is_some_and(|s| !matches!(s, "open" | "resolved" | "all")) {
        return Err(invalid("--status must be open, resolved or all"));
    }
    let include_archived = match flags.get("--include-archived").copied() {
        None | Some("false") => false,
        Some("true") => true,
        _ => return Err(invalid("--include-archived requires true or false")),
    };
    let id = if read {
        0
    } else {
        flags
            .get("--id")
            .and_then(|s| s.parse::<i64>().ok())
            .filter(|id| *id > 0)
            .ok_or_else(|| invalid("positive --id required"))?
    };
    let company = flags
        .get("--company")
        .ok_or_else(|| invalid("exceptions requires --company"))?;
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
            .take(MAX_EXCEPTION_BODY as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| invalid("could not read exception resolution body"))?;
    }
    let rt = tokio::runtime::Runtime::new().map_err(|_| invalid("could not start HTTP runtime"))?;
    let result = rt
        .block_on(async {
            if read {
                api.exceptions(company, status, include_archived).await
            } else {
                api.resolve_exception(company, id, bytes, confirm).await
            }
        })
        .map_err(crate::remote::client_error)?;
    Ok(json!({"ok":result.get("ok").and_then(Value::as_bool).unwrap_or(true),"data":result}))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalid_selectors_and_confirmation_fail_before_cookie_or_network() {
        for (command, flags, confirm) in [
            (
                "list-exceptions",
                BTreeMap::from([("--include-archived", "yes")]),
                false,
            ),
            (
                "list-exceptions",
                BTreeMap::from([("--status", "closed")]),
                false,
            ),
            ("list-exceptions", BTreeMap::new(), true),
            ("resolve-exception", BTreeMap::from([("--id", "1")]), false),
            ("resolve-exception", BTreeMap::from([("--id", "0")]), true),
            (
                "resolve-exception",
                BTreeMap::from([("--id", "1"), ("--status", "all")]),
                true,
            ),
        ] {
            assert!(run(command, &flags, true, confirm, false).is_err());
        }
    }
}
