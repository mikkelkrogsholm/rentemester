use rentemester_client::{ApiClient, MAX_PRIVACY_BODY, privacy_body_schema};
use rentemester_engine::Error;
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::Read};
type Failure = (u8, Error);
fn invalid(s: &str) -> Failure {
    (2, Error::new("invalid_input", s))
}
pub fn accepts(s: &str) -> bool {
    matches!(s, "retention" | "gdpr-export" | "gdpr-erase")
}
pub fn help() -> Value {
    json!({"usage":"retention --company SLUG; gdpr-export/gdpr-erase --company SLUG --confirm < exact-native.json. Explicit server/private cookie; no query/selectors. Native16KiB body with cvr?/name?/confirm:true; export additionally permits asOf?, erase forbids even asOf:null.","policy":"CompanyRead retention; CompanyExport audited export; CompanyAdmin overlay. Export is an audited mutation and can strengthen subject aliases. Review full subject export erasable/underRetention/retentionUnknown/retainUntil and coverage/refused rows before explicitly confirming same scoped subject. Server alone establishes trusted current erasure date/retention. Full coverage.complete:false and unknown facts retained. Overlay is not universal physical deletion, journal/original rewrite or backup erasure guarantee. No caller keys or automatic erase/retry. Unknown outcome requires canonical subject/contact/retention reads before further action; repeated export also creates audit evidence.","bodySchemas":{"export":privacy_body_schema(false),"erase":privacy_body_schema(true)}})
}
pub fn run(
    command: &str,
    flags: &BTreeMap<&str, &str>,
    allow: bool,
    confirm: bool,
    preview: bool,
) -> Result<Value, Failure> {
    let read = command == "retention";
    if confirm == read
        || preview
        || flags
            .keys()
            .any(|k| !["--server", "--cookie-file", "--company"].contains(k))
    {
        return Err(invalid(
            "retention has no confirmation/body/selectors; GDPR operations require --confirm and exact scoped native stdin",
        ));
    }
    let company = flags
        .get("--company")
        .ok_or_else(|| invalid("privacy requires --company"))?;
    let server = flags
        .get("--server")
        .ok_or_else(|| invalid("explicit --server required"))?;
    let cookie = flags
        .get("--cookie-file")
        .ok_or_else(|| invalid("private --cookie-file required"))?;
    let cookie = rentemester_client::read_cookie_file(std::path::Path::new(cookie))
        .map_err(crate::remote::client_error)?;
    let api = ApiClient::new(server, allow, Some(cookie)).map_err(crate::remote::client_error)?;
    let mut bytes = vec![];
    if !read {
        std::io::stdin()
            .take(MAX_PRIVACY_BODY as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| invalid("could not read exact privacy body"))?;
    }
    let rt = tokio::runtime::Runtime::new().map_err(|_| invalid("could not start HTTP runtime"))?;
    let result = rt
        .block_on(async {
            match command {
                "retention" => api.retention(company).await,
                "gdpr-export" => api.gdpr_export(company, bytes, confirm).await,
                _ => api.gdpr_erase(company, bytes, confirm).await,
            }
        })
        .map_err(crate::remote::client_error)?;
    Ok(json!({"ok":result.get("ok").and_then(Value::as_bool).unwrap_or(true),"data":result}))
}
