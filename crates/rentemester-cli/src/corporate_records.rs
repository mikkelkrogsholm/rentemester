use rentemester_client::{
    ApiClient, CorporateRecordMutation, MAX_CORPORATE_BODY, MAX_CORPORATE_INGEST_BODY,
    corporate_record_body_schema,
};
use rentemester_engine::Error;
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::Read};
type Failure = (u8, Error);
fn invalid(s: &str) -> Failure {
    (2, Error::new("invalid_input", s))
}
pub fn accepts(s: &str) -> bool {
    matches!(
        s,
        "corporate-records"
            | "corporate-record"
            | "corporate-record-original"
            | "ingest-corporate-record"
            | "link-corporate-record"
            | "enrich-corporate-record"
            | "supersede-corporate-record"
    )
}
pub fn help() -> Value {
    json!({"usage":"corporate-records --company ANCHOR [--cursor N] [--limit 1..100]; corporate-record/corporate-record-original --company ANCHOR --record-id ID. ingest-corporate-record --company ANCHOR --confirm < native.json; link-corporate-record/enrich-corporate-record/supersede-corporate-record additionally --record-id ID. Explicit server/private MFA cookie required.","policy":"Current scope/sensitivity and source history are server-owned. Original returns native {ok,bytesBase64,sha256} JSON bounded24MiB, no MIME/filename/file or verification claim. Ingest24MiB JSON/decoded16MiB; other writes32KiB. Exact inner and outer confirmation, explicit links/factual provenance; no inferred actor/scope, keys or automatic retry. Same-id source replay does not amend facts/links; do not infer safe resend. Unknown outcomes require canonical record/list/history/original reconciliation.","bodySchemas":{"ingest":corporate_record_body_schema(&CorporateRecordMutation::Ingest),"link":corporate_record_body_schema(&CorporateRecordMutation::Link(String::new())),"enrich":corporate_record_body_schema(&CorporateRecordMutation::Enrich(String::new())),"supersede":corporate_record_body_schema(&CorporateRecordMutation::Supersede(String::new()))}})
}
pub fn run(
    command: &str,
    flags: &BTreeMap<&str, &str>,
    allow: bool,
    confirm: bool,
    preview: bool,
) -> Result<Value, Failure> {
    let list = command == "corporate-records";
    let read = list || matches!(command, "corporate-record" | "corporate-record-original");
    let id = command != "corporate-records" && command != "ingest-corporate-record";
    if confirm == read
        || preview
        || flags.keys().any(|k| {
            !(["--server", "--cookie-file", "--company"].contains(k)
                || id && *k == "--record-id"
                || list && ["--cursor", "--limit"].contains(k))
        })
    {
        return Err(invalid(
            "corporate selectors/body/confirmation do not match command; see remote help",
        ));
    }
    let id = if id {
        Some(
            *flags
                .get("--record-id")
                .ok_or_else(|| invalid("explicit record-id required"))?,
        )
    } else {
        None
    };
    let company = flags
        .get("--company")
        .ok_or_else(|| invalid("explicit company required"))?;
    let server = flags
        .get("--server")
        .ok_or_else(|| invalid("explicit server required"))?;
    let cookie = flags
        .get("--cookie-file")
        .ok_or_else(|| invalid("private cookie file required"))?;
    let parse = |key, default| -> Result<usize, Failure> {
        flags.get(key).map_or(Ok(default), |v| {
            if v.is_empty() || !v.bytes().all(|b| b.is_ascii_digit()) {
                return Err(invalid("canonical nonnegative cursor/limit required"));
            }
            v.parse().map_err(|_| invalid("cursor/limit out of range"))
        })
    };
    let cursor = parse("--cursor", 0)?;
    let limit = parse("--limit", 25)?;
    let cookie = rentemester_client::read_cookie_file(std::path::Path::new(cookie))
        .map_err(crate::remote::client_error)?;
    let api = ApiClient::new(server, allow, Some(cookie)).map_err(crate::remote::client_error)?;
    let mut bytes = Vec::new();
    if !read {
        std::io::stdin()
            .take(if command == "ingest-corporate-record" {
                MAX_CORPORATE_INGEST_BODY as u64 + 1
            } else {
                MAX_CORPORATE_BODY as u64 + 1
            })
            .read_to_end(&mut bytes)
            .map_err(|_| invalid("could not read corporate body"))?;
    }
    let rt = tokio::runtime::Runtime::new().map_err(|_| invalid("HTTP runtime unavailable"))?;
    let outcome = rt.block_on(async {
        if list {
            api.corporate_records(company, cursor, limit).await
        } else if read {
            if command == "corporate-record-original" {
                api.corporate_record_original(company, id.unwrap()).await
            } else {
                api.corporate_record(company, id.unwrap()).await
            }
        } else {
            let action = match command {
                "ingest-corporate-record" => CorporateRecordMutation::Ingest,
                "link-corporate-record" => CorporateRecordMutation::Link(id.unwrap().into()),
                "enrich-corporate-record" => CorporateRecordMutation::Enrich(id.unwrap().into()),
                _ => CorporateRecordMutation::Supersede(id.unwrap().into()),
            };
            api.mutate_corporate_record(company, action, bytes, confirm)
                .await
        }
    });
    let result = outcome.map_err(crate::remote::client_error)?;
    Ok(json!({"ok":result.get("ok").and_then(Value::as_bool).unwrap_or(true),"data":result}))
}
