//! Exact stdin ingestion and authorized original bytes on stdout; never reads or writes document paths.
use rentemester_client::MAX_DOCUMENT_INGEST_BODY;
use rentemester_engine::Error;
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::Read};
type Failure = (u8, Error);
fn invalid(s: &str) -> Failure {
    (2, Error::new("invalid_input", s))
}
pub fn run(
    command: &str,
    flags: &BTreeMap<&str, &str>,
    allow_http: bool,
    confirmed: bool,
    preview: bool,
) -> Result<Value, Failure> {
    let ingest = command == "ingest-document";
    if confirmed != ingest
        || preview
        || flags.keys().any(|k| {
            !([
                "--server",
                "--cookie-file",
                "--service-key-file",
                "--company",
            ]
            .contains(k)
                || !ingest && *k == "--id")
        })
    {
        return Err(invalid(
            "ingest-document requires --confirm and exact stdin JSON; document-original requires only --id, no confirmation or filters",
        ));
    }
    let id = if ingest {
        0
    } else {
        flags
            .get("--id")
            .and_then(|s| s.parse::<i64>().ok())
            .filter(|n| (1..=9_007_199_254_740_991).contains(n))
            .ok_or_else(|| invalid("document-original requires positive exact --id"))?
    };
    let company = flags
        .get("--company")
        .ok_or_else(|| invalid("document operations require --company"))?;
    let server = flags
        .get("--server")
        .ok_or_else(|| invalid("explicit --server required"))?;
    let api = crate::remote::credential_client(server, allow_http, flags, false)?;
    let bytes = if ingest {
        let mut b = vec![];
        std::io::stdin()
            .take(MAX_DOCUMENT_INGEST_BODY as u64 + 1)
            .read_to_end(&mut b)
            .map_err(|_| invalid("could not read native document JSON"))?;
        Some(b)
    } else {
        None
    };
    let runtime = tokio::runtime::Runtime::new()
        .map_err(|_| (1, Error::new("transport", "could not start HTTP runtime")))?;
    let value = runtime
        .block_on(async {
            if ingest {
                api.ingest_document(company, bytes.unwrap(), confirmed)
                    .await
            } else {
                api.document_original(company, id).await
            }
        })
        .map_err(crate::remote::client_error)?;
    Ok(json!({"ok":value.get("ok").and_then(Value::as_bool).unwrap_or(true),"data":value}))
}
