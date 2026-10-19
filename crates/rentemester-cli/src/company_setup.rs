use rentemester_client::{
    ApiClient, CompanySetupMutation, MAX_COMPANY_SETUP_BODY, company_setup_body_schema,
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
        "company-profile" | "update-company-profile" | "create-account" | "confirm-account-role"
    )
}
pub fn help() -> Value {
    json!({"usage":"company-profile --company SLUG reads current profile (accounts reads chart/role state). update-company-profile/create-account/confirm-account-role --company SLUG --confirm < exact-native.json. Explicit server/private cookie, no year/id/query selectors. Native16KiB.","policy":"CompanyRead reads, CompanyAdmin writes. Profile PATCH supports only cvr/address/postalCode/city/vatPeriodType/paymentTermsDays/payment. Profile and account creation have NO inner confirm; role body has confirm:true and exact chosen role/accountNo. Server validates current accounts/types/normal balances/VAT cadence/payment profile, duplicates and obligations. Preserve full authoritative chart/role incomplete/ambiguous state. No inferred bank/VAT/debtor/creditor defaults, company creation, client policy/math, caller keys, hidden journals or automatic retries. On unknown read company-profile/accounts before further explicit action; same mapped role may return current resolution but is no caller-key receipt.","bodySchemas":{"profile":company_setup_body_schema(CompanySetupMutation::UpdateProfile),"account":company_setup_body_schema(CompanySetupMutation::CreateAccount),"role":company_setup_body_schema(CompanySetupMutation::ConfirmAccountRole)}})
}
pub fn run(
    command: &str,
    flags: &BTreeMap<&str, &str>,
    allow: bool,
    confirm: bool,
    preview: bool,
) -> Result<Value, Failure> {
    let read = command == "company-profile";
    if confirm == read
        || preview
        || flags
            .keys()
            .any(|k| !["--server", "--cookie-file", "--company"].contains(k))
    {
        return Err(invalid(
            "company-profile reads without confirmation/selectors; setup writes require --confirm and exact stdin only",
        ));
    }
    let company = flags
        .get("--company")
        .ok_or_else(|| invalid("current setup requires --company"))?;
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
            .take(MAX_COMPANY_SETUP_BODY as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| invalid("could not read exact setup body"))?;
    }
    let rt = tokio::runtime::Runtime::new().map_err(|_| invalid("could not start HTTP runtime"))?;
    let result = rt
        .block_on(async {
            if read {
                api.company_profile(company).await
            } else {
                api.company_setup_mutation(
                    company,
                    match command {
                        "update-company-profile" => CompanySetupMutation::UpdateProfile,
                        "create-account" => CompanySetupMutation::CreateAccount,
                        _ => CompanySetupMutation::ConfirmAccountRole,
                    },
                    bytes,
                    confirm,
                )
                .await
            }
        })
        .map_err(crate::remote::client_error)?;
    Ok(json!({"ok":result.get("ok").and_then(Value::as_bool).unwrap_or(true),"data":result}))
}
