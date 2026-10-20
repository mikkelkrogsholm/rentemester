//! Operator-selected scanner for ordinary document ingestion; no caller-selected provider.
use rentemester_runtime::document_scanner::{GatewayConfig, HttpJsonScanner};
use serde::Deserialize;
use std::{io::Read, path::PathBuf};
use zeroize::Zeroize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct Config {
    policy: String,
    provider: Option<String>,
    url: Option<String>,
    bearer_token: Option<String>,
    timeout_ms: Option<u64>,
    trusted_ca_file: Option<PathBuf>,
}
impl Drop for Config {
    fn drop(&mut self) {
        if let Some(token) = &mut self.bearer_token {
            token.zeroize();
        }
    }
}
pub(crate) fn build(
    config: Option<&Config>,
) -> Result<Option<HttpJsonScanner>, Box<dyn std::error::Error>> {
    let Some(config) = config else {
        return Ok(None);
    };
    if config.policy == "off" {
        if config.provider.is_some()
            || config.url.is_some()
            || config.bearer_token.is_some()
            || config.timeout_ms.is_some()
            || config.trusted_ca_file.is_some()
        {
            return Err("documentScanner off must not configure a provider".into());
        }
        return Ok(None);
    }
    if config.policy != "required" || config.provider.as_deref() != Some("http-json-v1") {
        return Err("invalid documentScanner policy or provider".into());
    }
    let gateway = GatewayConfig {
        endpoint: config
            .url
            .as_deref()
            .ok_or("documentScanner URL required")?,
        bearer_token: config
            .bearer_token
            .as_deref()
            .ok_or("documentScanner bearer token required")?,
        timeout_ms: config.timeout_ms.unwrap_or(15_000),
    };
    let scanner = if let Some(path) = &config.trusted_ca_file {
        if !path.is_absolute() {
            return Err("documentScanner trustedCaFile requires an absolute path".into());
        }
        let mut pem = Vec::new();
        std::fs::File::open(path)
            .map_err(|_| "documentScanner trust unavailable")?
            .take(65_537)
            .read_to_end(&mut pem)
            .map_err(|_| "documentScanner trust unavailable")?;
        HttpJsonScanner::with_trusted_ca(gateway, &pem)
    } else {
        HttpJsonScanner::new(gateway)
    }
    .map_err(|_| "invalid documentScanner configuration")?;
    Ok(Some(scanner))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scanner_policy_never_silently_discards_provider_or_required_configuration() {
        assert!(build(None).unwrap().is_none());
        let off: Config = serde_json::from_str(r#"{"policy":"off"}"#).unwrap();
        assert!(build(Some(&off)).unwrap().is_none());
        for value in [
            serde_json::json!({"policy":"unknown"}),
            serde_json::json!({"policy":"off","timeoutMs":1000}),
            serde_json::json!({"policy":"off","provider":"http-json-v1"}),
            serde_json::json!({"policy":"required"}),
            serde_json::json!({"policy":"required","provider":"http-json-v1","url":"https://scanner.example.invalid"}),
            serde_json::json!({"policy":"required","provider":"http-json-v1","url":"http://scanner.example.invalid","bearerToken":"synthetic-token"}),
        ] {
            let config: Config = serde_json::from_value(value).unwrap();
            assert!(build(Some(&config)).is_err());
        }
        assert!(
            serde_json::from_str::<Config>(r#"{"policy":"required","callerEvidence":true}"#)
                .is_err()
        );
    }
}
