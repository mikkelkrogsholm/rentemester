use rentemester_engine::Error;
use serde_json::{Value, json};
use std::collections::BTreeMap;
fn invalid() -> (u8, Error) {
    (
        2,
        Error::new(
            "invalid_input",
            "audit-verify requires company, server and one private credential; no query, body, confirmation or actor override",
        ),
    )
}
pub fn help() -> Value {
    json!({"usage":"remote audit-verify --company SLUG --server URL --cookie-file FILE|--service-key-file FILE [--allow-loopback-http]","policy":"Current CompanyRead; explicit readonly full audit observation. data.audit.ok/available/components/errors carry findings. A successful command can contain failed accounting checks. No auto-repair, inferred paths, actor override or retry."})
}
pub fn run(
    flags: &BTreeMap<&str, &str>,
    allow: bool,
    confirm: bool,
    preview: bool,
) -> Result<Value, (u8, Error)> {
    if confirm
        || preview
        || flags.keys().any(|key| {
            ![
                "--company",
                "--server",
                "--cookie-file",
                "--service-key-file",
            ]
            .contains(key)
        })
    {
        return Err(invalid());
    }
    let company = flags
        .get("--company")
        .copied()
        .filter(|value| {
            !value.is_empty()
                && value.len() <= 128
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
        })
        .ok_or_else(invalid)?;
    let server = flags.get("--server").copied().ok_or_else(invalid)?;
    let api = crate::remote::credential_client(server, allow, flags, false)?;
    let runtime = tokio::runtime::Runtime::new().map_err(|_| invalid())?;
    let data = runtime
        .block_on(api.audit_verify(company))
        .map_err(crate::remote::client_error)?;
    Ok(json!({"ok":true,"data":data}))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn audit_verify_rejects_actor_or_paging_before_credentials() {
        for (key, value) in [
            ("--actor", "agent:forged"),
            ("--limit", "1"),
            ("--body", "{}"),
        ] {
            let flags = BTreeMap::from([
                ("--company", "synthetic"),
                ("--server", "http://127.0.0.1:9"),
                (key, value),
            ]);
            assert_eq!(run(&flags, true, false, false).unwrap_err().0, 2);
        }
    }
}
