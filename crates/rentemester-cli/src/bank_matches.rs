//! Advisory reads use current server data; applying a reviewed match is separate.
use rentemester_client::ApiClient;
use rentemester_engine::Error;
use serde_json::{Value, json};
use std::collections::BTreeMap;
type Failure = (u8, Error);
fn invalid(message: &str) -> Failure {
    (2, Error::new("invalid_input", message))
}
pub fn accepts(command: &str) -> bool {
    command == "bank-suggest-matches"
}
pub fn help() -> Value {
    json!({"usage":"remote bank-suggest-matches --company SLUG [--bank-transaction-id N] [--max N]. Explicit --server and private --cookie-file required.","policy":"Current CompanyRead, read-only advice for unmatched bank rows. max is positive and limits suggestions per row, default5; it does not limit bank rows. Scores can exceed1 and are not probabilities. Canonical source ok:false/errors remain visible and return exit1. No stdin body, confirmation, caller actor, keys, local scoring or automatic financial action."})
}
fn envelope(value: Value) -> Value {
    json!({"ok":value["ok"].as_bool().unwrap_or(false),"data":value})
}
pub fn run(
    command: &str,
    flags: &BTreeMap<&str, &str>,
    allow: bool,
    confirm: bool,
    preview: bool,
) -> Result<Value, Failure> {
    if !accepts(command)
        || confirm
        || preview
        || flags.keys().any(|key| {
            ![
                "--server",
                "--cookie-file",
                "--company",
                "--bank-transaction-id",
                "--max",
            ]
            .contains(key)
        })
    {
        return Err(invalid(
            "bank-suggest-matches accepts only company, bank transaction id and max read selectors",
        ));
    }
    let id = flags
        .get("--bank-transaction-id")
        .map(|text| {
            text.parse::<i64>()
                .ok()
                .filter(|id| (1..=9_007_199_254_740_991).contains(id))
                .ok_or_else(|| invalid("positive safe --bank-transaction-id required"))
        })
        .transpose()?;
    let max = flags
        .get("--max")
        .map(|text| {
            text.parse::<usize>()
                .ok()
                .filter(|max| *max > 0)
                .ok_or_else(|| invalid("positive integer --max required"))
        })
        .transpose()?;
    let required = |key| {
        flags
            .get(key)
            .copied()
            .ok_or_else(|| invalid("explicit company, server and private cookie-file required"))
    };
    let company = required("--company")?;
    let cookie =
        rentemester_client::read_cookie_file(std::path::Path::new(required("--cookie-file")?))
            .map_err(crate::remote::client_error)?;
    let api = ApiClient::new(required("--server")?, allow, Some(cookie))
        .map_err(crate::remote::client_error)?;
    let rt = tokio::runtime::Runtime::new().map_err(|_| invalid("could not start HTTP runtime"))?;
    let value = rt
        .block_on(api.bank_suggest_matches(company, id, max))
        .map_err(crate::remote::client_error)?;
    Ok(envelope(value))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalid_read_arguments_fail_before_cookie_or_network() {
        for flags in [
            BTreeMap::from([("--max", "0")]),
            BTreeMap::from([("--max", "-1")]),
            BTreeMap::from([("--max", "1.5")]),
            BTreeMap::from([("--bank-transaction-id", "0")]),
            BTreeMap::from([("--bank-transaction-id", "9007199254740992")]),
            BTreeMap::from([("--actor", "agent:forged")]),
            BTreeMap::from([("--body", "{}")]),
        ] {
            assert!(run("bank-suggest-matches", &flags, true, false, false).is_err());
        }
        assert!(run("bank-suggest-matches", &BTreeMap::new(), true, true, false).is_err());
        assert!(run("bank-suggest-matches", &BTreeMap::new(), true, false, true).is_err());
    }
    #[test]
    fn unsuccessful_advisory_keeps_false_outer_status_and_source_errors() {
        let source = json!({"ok":false,"count":0,"rows":[],"errors":["unmatched bank transaction 7 does not exist"]});
        let result = envelope(source.clone());
        assert_eq!(result["ok"], false);
        assert_eq!(result["data"], source);
    }
}
