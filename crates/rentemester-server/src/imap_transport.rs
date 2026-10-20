//! Operator-only mail transport trust and deadline. Company mailbox JSON cannot set these.
use rentemester_runtime::imap::OperatorConfig;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct Config {
    trusted_ca_pem: Option<String>,
    timeout_ms: Option<u64>,
}
pub(crate) fn build(config: Option<&Config>) -> Result<OperatorConfig, &'static str> {
    let timeout_ms = config.and_then(|c| c.timeout_ms).unwrap_or(30_000);
    if !(100..=30_000).contains(&timeout_ms) {
        return Err("imapTransport timeoutMs must be 100 to 30000");
    }
    let trusted_ca_pem = config.and_then(|c| c.trusted_ca_pem.clone());
    if trusted_ca_pem.as_ref().is_some_and(|s| s.is_empty() || s.len() > 65_536) {
        return Err("imapTransport trustedCaPem must be 1 to 65536 bytes");
    }
    let operator = OperatorConfig { trusted_ca_pem, timeout_ms };
    rentemester_runtime::imap::NetworkClient::new(operator.clone())
        .map_err(|_| "invalid imapTransport trust configuration")?;
    Ok(operator)
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn operator_defaults_and_request_boundary_are_explicit() {
        assert_eq!(build(None).unwrap().timeout_ms, 30_000);
        for input in [json!({"timeoutMs":0}), json!({"timeoutMs":30001}),json!({"trustedCaPem":""}),json!({"trustedCaPem":"invalid PEM"})] {
            let c: Config = serde_json::from_value(input).unwrap();
            assert!(build(Some(&c)).is_err());
        }
        assert!(serde_json::from_value::<Config>(json!({"acceptInvalidCertificates":true})).is_err());
        assert!(serde_json::from_value::<Config>(json!({"password":"not-an-operator-setting"})).is_err());
    }
}
