//! Explicit interactive CVR actions using canonical server results.
use rentemester_client::ApiClient;
use rentemester_engine::Error;
use serde_json::{Value, json};
use std::collections::BTreeMap;
type Failure = (u8, Error);
fn invalid(message: &str) -> Failure {
    (2, Error::new("invalid_input", message))
}
pub fn accepts(command: &str) -> bool {
    matches!(command, "cvr-status" | "cvr-lookup" | "sync-company-cvr")
}
pub fn help() -> Value {
    json!({"usage":"remote cvr-status; remote cvr-lookup --company SLUG --cvr TEXT --confirm; remote sync-company-cvr --company SLUG --confirm. All require explicit --server and private --cookie-file.","policy":"Status reads credential presence only. Lookup explicitly calls the operator CVR provider or retained cache; sync updates only own-company CVR master data and audits even no-change sync. No caller credentials, endpoint, force-refresh, clock, actor, body or keys. Fiscal year and accounting are never changed. Exact inner result/errors/fetchedAt preserved; business ok:false returns exit1. No automatic retries; reconcile canonical state after unknown outcomes."})
}
fn envelope(value: Value, status: bool) -> Value {
    json!({"ok":status || value["ok"].as_bool().unwrap_or(false),"data":value})
}
pub fn run(
    command: &str,
    flags: &BTreeMap<&str, &str>,
    allow: bool,
    confirm: bool,
    preview: bool,
) -> Result<Value, Failure> {
    let status = command == "cvr-status";
    let lookup = command == "cvr-lookup";
    if !accepts(command)
        || confirm == status
        || preview
        || flags.keys().any(|key| {
            !(["--server", "--cookie-file"].contains(key)
                || !status && *key == "--company"
                || lookup && *key == "--cvr")
        })
    {
        return Err(invalid(
            "CVR status accepts no company or confirmation; lookup/sync require explicit confirmation and exact selectors",
        ));
    }
    let required = |key| {
        flags
            .get(key)
            .copied()
            .ok_or_else(|| invalid("explicit CVR command arguments required"))
    };
    let company = if status {
        None
    } else {
        Some(required("--company")?)
    };
    let cvr = if lookup {
        Some(required("--cvr")?)
    } else {
        None
    };
    if cvr.is_some_and(|cvr| cvr.len() > 128) {
        return Err(invalid("CVR lookup text exceeds128 bytes"));
    }
    let cookie =
        rentemester_client::read_cookie_file(std::path::Path::new(required("--cookie-file")?))
            .map_err(crate::remote::client_error)?;
    let api = ApiClient::new(required("--server")?, allow, Some(cookie))
        .map_err(crate::remote::client_error)?;
    let runtime =
        tokio::runtime::Runtime::new().map_err(|_| invalid("could not start HTTP runtime"))?;
    let value = runtime
        .block_on(async {
            if status {
                api.cvr_status().await
            } else if lookup {
                api.cvr_lookup(company.unwrap(), cvr.unwrap(), confirm)
                    .await
            } else {
                api.sync_company_cvr(company.unwrap(), confirm).await
            }
        })
        .map_err(crate::remote::client_error)?;
    Ok(envelope(value, status))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn confirmation_and_exact_command_flags_precede_cookie_access() {
        for (command, flags, confirm) in [
            ("cvr-status", BTreeMap::new(), true),
            (
                "cvr-status",
                BTreeMap::from([("--company", "synthetic")]),
                false,
            ),
            (
                "cvr-lookup",
                BTreeMap::from([("--company", "synthetic"), ("--cvr", "12345678")]),
                false,
            ),
            (
                "sync-company-cvr",
                BTreeMap::from([("--company", "synthetic")]),
                false,
            ),
            (
                "sync-company-cvr",
                BTreeMap::from([("--cvr", "12345678")]),
                true,
            ),
            (
                "cvr-lookup",
                BTreeMap::from([("--endpoint", "https://forged.invalid")]),
                true,
            ),
            ("cvr-lookup", BTreeMap::from([("--actor", "forged")]), true),
        ] {
            assert!(run(command, &flags, true, confirm, false).is_err());
        }
    }
    #[test]
    fn inner_business_failure_stays_false_while_missing_configuration_status_is_successful_read() {
        let failure =
            json!({"ok":false,"cached":false,"company":null,"errors":["Missing CVR credentials"]});
        let wrapped = envelope(failure.clone(), false);
        assert_eq!(wrapped["ok"], false);
        assert_eq!(wrapped["data"], failure);
        assert_eq!(envelope(json!({"configured":false}), true)["ok"], true);
    }
}
