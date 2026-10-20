//! Explicit operator configuration for optional invoice evidence extraction.
use rentemester_runtime::invoice_extractor::{GatewayConfig, HttpJsonExtractor};
use serde::Deserialize;
use std::{io::Read, path::PathBuf};
use zeroize::Zeroize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct Config {
    provider: String,
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
) -> Result<Option<HttpJsonExtractor>, Box<dyn std::error::Error>> {
    let Some(config) = config else {
        return Ok(None);
    };
    if config.provider == "disabled" {
        if config.url.is_some()
            || config.bearer_token.is_some()
            || config.timeout_ms.is_some()
            || config.trusted_ca_file.is_some()
        {
            return Err("invoiceExtractor disabled must not configure a provider".into());
        }
        return Ok(None);
    }
    if config.provider != "http-json-v1" {
        return Err("invalid invoiceExtractor provider".into());
    }
    let gateway = GatewayConfig {
        endpoint: config
            .url
            .as_deref()
            .ok_or("invoiceExtractor URL required")?,
        bearer_token: config
            .bearer_token
            .as_deref()
            .ok_or("invoiceExtractor bearer token required")?,
        timeout_ms: config.timeout_ms.unwrap_or(15_000),
    };
    let extractor = if let Some(path) = &config.trusted_ca_file {
        if !path.is_absolute() {
            return Err("invoiceExtractor trustedCaFile requires an absolute path".into());
        }
        let mut pem = Vec::new();
        std::fs::File::open(path)
            .map_err(|_| "invoiceExtractor trust unavailable")?
            .take(65_537)
            .read_to_end(&mut pem)
            .map_err(|_| "invoiceExtractor trust unavailable")?;
        if pem.is_empty() || pem.len() > 65_536 {
            return Err("invoiceExtractor trusted CA must be 1 to 65536 bytes".into());
        }
        HttpJsonExtractor::with_trusted_ca(gateway, &pem)
    } else {
        HttpJsonExtractor::new(gateway)
    }
    .map_err(|_| "invalid invoiceExtractor configuration")?;
    Ok(Some(extractor))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};
    fn configured() -> Value {
        json!({"provider":"http-json-v1","url":"https://extractor.example.invalid/invoice","bearerToken":"synthetic-token"})
    }
    #[test]
    fn optional_disabled_and_known_provider_are_explicit() {
        assert!(build(None).unwrap().is_none());
        let off: Config = serde_json::from_value(json!({"provider":"disabled"})).unwrap();
        assert!(build(Some(&off)).unwrap().is_none());
        let on: Config = serde_json::from_value(configured()).unwrap();
        let extractor = build(Some(&on)).unwrap().unwrap();
        assert_eq!(extractor.identity().id, "http-json-v1");
        assert_eq!(extractor.identity().version, "1");
        for value in [
            json!({"provider":"unknown"}),
            json!({"provider":"http-json-v1"}),
            json!({"provider":"disabled","url":"https://ignored.invalid"}),
            json!({"provider":"disabled","bearerToken":"synthetic-token"}),
            json!({"provider":"disabled","timeoutMs":15000}),
        ] {
            let config: Config = serde_json::from_value(value).unwrap();
            assert!(build(Some(&config)).is_err());
        }
        assert!(
            serde_json::from_value::<Config>(json!({"provider":"disabled","callerActor":"u"}))
                .is_err()
        );
    }
    #[test]
    fn invalid_transport_token_and_timeout_fail_before_owner_open() {
        for (key, value) in [
            ("url", json!("http://extractor.example.invalid")),
            ("url", json!("https://u:secret@extractor.example.invalid")),
            ("url", json!("https://127.0.0.1/invoice")),
            ("bearerToken", json!("")),
            ("bearerToken", json!(" secret")),
            ("bearerToken", json!("secret\r\nHeader: injected")),
            ("bearerToken", json!("x".repeat(4097))),
            ("timeoutMs", json!(99)),
            ("timeoutMs", json!(120001)),
            ("trustedCaFile", json!("relative-ca.pem")),
        ] {
            let mut value_config = configured();
            value_config[key] = value;
            let config: Config = serde_json::from_value(value_config).unwrap();
            let error = build(Some(&config)).err().unwrap().to_string();
            assert!(!error.contains("secret"));
        }
    }
    #[test]
    fn private_ca_is_bounded_and_invalid_pem_is_refused() {
        use std::io::Write;
        let path = std::env::temp_dir().join(format!(
            "rm-extractor-config-{}-{}.pem",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&path).unwrap();
        let mut value = configured();
        value["trustedCaFile"] = json!(path);
        let config: Config = serde_json::from_value(value).unwrap();
        for bytes in [vec![], vec![b'x'; 65537], b"not a certificate".to_vec()] {
            file.set_len(0).unwrap();
            use std::io::{Seek, SeekFrom};
            file.seek(SeekFrom::Start(0)).unwrap();
            file.write_all(&bytes).unwrap();
            file.sync_all().unwrap();
            assert!(build(Some(&config)).is_err());
        }
        drop(file);
        std::fs::remove_file(path).unwrap();
    }
}
