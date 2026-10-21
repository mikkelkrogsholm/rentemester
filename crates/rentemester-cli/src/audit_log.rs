use rentemester_client::AuditLogQuery;
use rentemester_engine::Error;
use serde_json::{Value, json};
use std::collections::BTreeMap;
type Failure = (u8, Error);
fn invalid() -> Failure {
    (
        2,
        Error::new(
            "invalid_input",
            "audit-log-list requires company and valid audit read filters only; no body, confirmation or actor authority",
        ),
    )
}
pub fn help() -> Value {
    json!({"usage":"remote audit-log-list --company SLUG [--limit 1..5000] [--offset N] [--from-date YYYY-MM-DD|RFC3339] [--to-date YYYY-MM-DD|RFC3339] [--event-type-like TEXT] [--actor-like TEXT]. Explicit --server plus private --cookie-file or scoped --service-key-file.","policy":"Current CompanyRead, read-only. Default limit500/offset0. Bare dates use UTC start/end of day; timestamps require an explicit offset. LIKE filters are ANDed case-insensitive substrings: % and _ remain SQL wildcards. Stable createdAt DESC,id DESC ordering; total/count/hasMore/nextOffset are returned by the server. Pages are separate observations if writes intervene. Messages are exact plain text; no verification/health conclusion."})
}
pub fn run(
    flags: &BTreeMap<&str, &str>,
    allow: bool,
    confirm: bool,
    preview: bool,
) -> Result<Value, Failure> {
    if confirm
        || preview
        || flags.keys().any(|k| {
            ![
                "--company",
                "--server",
                "--cookie-file",
                "--service-key-file",
                "--limit",
                "--offset",
                "--from-date",
                "--to-date",
                "--event-type-like",
                "--actor-like",
            ]
            .contains(k)
        })
    {
        return Err(invalid());
    }
    let number = |key| {
        flags
            .get(key)
            .map(|v| {
                if v.is_empty() || !v.bytes().all(|b| b.is_ascii_digit()) {
                    return Err(invalid());
                }
                v.parse::<u64>().map_err(|_| invalid())
            })
            .transpose()
    };
    let text = |key| flags.get(key).map(|s| (*s).to_owned());
    let query = AuditLogQuery {
        limit: number("--limit")?,
        offset: number("--offset")?,
        from_date: text("--from-date"),
        to_date: text("--to-date"),
        event_type_like: text("--event-type-like"),
        actor_like: text("--actor-like"),
    };
    query.validate().map_err(crate::remote::client_error)?;
    let company = flags
        .get("--company")
        .copied()
        .filter(|s| {
            !s.is_empty()
                && s.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        })
        .ok_or_else(invalid)?;
    let server = flags.get("--server").copied().ok_or_else(invalid)?;
    let api = crate::remote::credential_client(server, allow, flags, false)?;
    let runtime = tokio::runtime::Runtime::new().map_err(|_| invalid())?;
    let data = runtime
        .block_on(api.audit_log(company, &query))
        .map_err(crate::remote::client_error)?;
    Ok(json!({"ok":true,"data":data}))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalid_filters_and_authority_fail_before_credentials() {
        for flags in [
            BTreeMap::from([("--limit", "0")]),
            BTreeMap::from([("--offset", "-1")]),
            BTreeMap::from([("--offset", "1.5")]),
            BTreeMap::from([("--from-date", "2026-02-30")]),
            BTreeMap::from([("--actor", "forged")]),
            BTreeMap::from([("--body", "{}")]),
        ] {
            assert_eq!(run(&flags, true, false, false).unwrap_err().0, 2)
        }
        assert!(run(&BTreeMap::new(), true, true, false).is_err());
        let help = crate::remote::run(&["--help".into()]).unwrap();
        assert!(
            help["commands"]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v == "audit-log-list")
        );
    }
}
