//! Human-cookie mileage posting commands; exact body files, never retries.
use rentemester_client::{
    ApiClient, MAX_MILEAGE_POSTING_BODY, MileagePostingOperation as Op,
    validate_mileage_posting_body,
};
use rentemester_engine::Error;
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::Read};
pub const COMMANDS: [&str; 5] = [
    "mileage-rate-versions",
    "configure-mileage-rates",
    "preview-mileage-post",
    "post-mileage",
    "mileage-postings",
];
pub fn accepts(s: &str) -> bool {
    COMMANDS.contains(&s)
}
pub fn help() -> Value {
    json!({"commands":COMMANDS,"usage":"--server ORIGIN --cookie-file PRIVATE_PATH --company SLUG; configure/preview/post require --body-file PATH with exact native32KiB JSON; configure/post additionally require --confirm and inner confirm:true. No query selectors. CompanyRead reads/preview, CompanyAdmin config, CompanyLedgerPost post. Cookie only; service routes not admitted, no credential fallback. Explicit version/source/basis/accounts; log registration never posts. Server owns rates, evidence, arithmetic, atomic journal and receipts. No automatic retry; reconcile outcome_unknown via mileage-postings and retain exact original intent/key/hash."})
}
pub fn run(
    command: &str,
    flags: &BTreeMap<&str, &str>,
    allow: bool,
    confirmed: bool,
    preview: bool,
) -> Result<Value, (u8, Error)> {
    let invalid = |s| (2, Error::new("invalid_input", s));
    let op = match command {
        "configure-mileage-rates" => Some(Op::Configure),
        "preview-mileage-post" => Some(Op::Preview),
        "post-mileage" => Some(Op::Post),
        _ => None,
    };
    let write = matches!(op, Some(Op::Configure | Op::Post));
    if confirmed != write
        || preview
        || flags
            .keys()
            .any(|k| !["--server", "--cookie-file", "--company", "--body-file"].contains(k))
        || flags.contains_key("--body-file") != op.is_some()
    {
        return Err(invalid(
            "exact mileage selectors only; writes require --confirm; preview and reads forbid confirmation",
        ));
    }
    let required = |k| {
        flags.get(k).copied().ok_or_else(|| {
            invalid(
                "explicit server/cookie/company and body-file for configure/preview/post required",
            )
        })
    };
    let mut bytes = vec![];
    if let Some(op) = op {
        let file = std::fs::File::open(required("--body-file")?)
            .map_err(|_| invalid("could not open body file"))?;
        if !file
            .metadata()
            .map_err(|_| invalid("could not inspect body file"))?
            .is_file()
        {
            return Err(invalid("body file must be regular"));
        }
        file.take(MAX_MILEAGE_POSTING_BODY as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| invalid("could not read body file"))?;
        validate_mileage_posting_body(&bytes, op).map_err(crate::remote::client_error)?;
    }
    let company = required("--company")?;
    let cookie =
        rentemester_client::read_cookie_file(std::path::Path::new(required("--cookie-file")?))
            .map_err(crate::remote::client_error)?;
    let api = ApiClient::new(required("--server")?, allow, Some(cookie))
        .map_err(crate::remote::client_error)?;
    let rt = tokio::runtime::Runtime::new().map_err(|_| invalid("HTTP runtime unavailable"))?;
    let v = rt
        .block_on(async {
            match command {
                "mileage-rate-versions" => api.mileage_rate_versions(company).await,
                "mileage-postings" => api.mileage_postings(company).await,
                "configure-mileage-rates" => {
                    api.configure_mileage_rates(company, bytes, confirmed).await
                }
                "preview-mileage-post" => api.preview_mileage_post(company, bytes).await,
                _ => api.post_mileage(company, bytes, confirmed).await,
            }
        })
        .map_err(crate::remote::client_error)?;
    Ok(json!({"ok":v["ok"].as_bool().unwrap_or(false),"data":v}))
}
