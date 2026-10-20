//! Explicit PDF inspection through the server, without local parsing or automatic retries.
use rentemester_client::ApiClient;
use rentemester_engine::Error;
use serde_json::{Value, json};
use std::collections::BTreeMap;
type Failure = (u8, Error);
pub fn accepts(command: &str) -> bool {
    matches!(
        command,
        "parse-document"
            | "document-parse-status"
            | "document-parsed-text"
            | "parse-pending-documents"
    )
}
fn invalid(message: &str) -> Failure {
    (2, Error::new("invalid_input", message))
}
pub fn run(
    command: &str,
    flags: &BTreeMap<&str, &str>,
    allow_http: bool,
    confirmed: bool,
    preview: bool,
) -> Result<Value, Failure> {
    let write = matches!(command, "parse-document" | "parse-pending-documents");
    let paged = matches!(command, "document-parsed-text" | "parse-pending-documents");
    if confirmed != write
        || preview
        || flags.keys().any(|k| {
            !(["--server", "--cookie-file", "--company"].contains(k)
                || command != "parse-pending-documents" && *k == "--id"
                || paged && ["--cursor", "--limit"].contains(k))
        })
    {
        return Err(invalid(
            "PDF operations require explicit company/server/cookie, --id except pending, --confirm only for parsing, --cursor/--limit only for text/pending",
        ));
    }
    let required = |key| {
        flags.get(key).copied().ok_or_else(|| {
            invalid("PDF operations require explicit company/server/cookie and document id")
        })
    };
    let id = if command == "parse-pending-documents" {
        0
    } else {
        required("--id")?
            .parse::<i64>()
            .ok()
            .filter(|n| (1..=9_007_199_254_740_991).contains(n))
            .ok_or_else(|| invalid("positive exact --id required"))?
    };
    let cursor = flags.get("--cursor").map_or(Ok(0), |s| {
        s.parse::<u64>()
            .map_err(|_| invalid("nonnegative exact --cursor required"))
    })?;
    let limit = flags.get("--limit").map_or(
        Ok(if command == "parse-pending-documents" {
            100
        } else {
            10
        }),
        |s| {
            s.parse::<u8>()
                .map_err(|_| invalid("bounded --limit required"))
        },
    )?;
    let cookie =
        rentemester_client::read_cookie_file(std::path::Path::new(required("--cookie-file")?))
            .map_err(crate::remote::client_error)?;
    let api = ApiClient::new(required("--server")?, allow_http, Some(cookie))
        .map_err(crate::remote::client_error)?;
    let company = required("--company")?;
    let runtime = tokio::runtime::Runtime::new()
        .map_err(|_| (1, Error::new("transport", "could not start HTTP runtime")))?;
    let value = runtime
        .block_on(async {
            match command {
                "parse-document" => api.document_parse(company, id, confirmed).await,
                "document-parse-status" => api.document_parse_status(company, id).await,
                "document-parsed-text" => {
                    api.document_parsed_text(company, id, cursor, limit).await
                }
                _ => {
                    api.documents_parse_pending(company, cursor, limit, confirmed)
                        .await
                }
            }
        })
        .map_err(crate::remote::client_error)?;
    Ok(json!({"ok":value.get("ok").and_then(Value::as_bool).unwrap_or(true),"data":value}))
}
