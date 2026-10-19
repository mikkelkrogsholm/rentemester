use crate::tools::{company_schema, native_body, tool};
use rentemester_client::{
    ApiClient, CompanySetupMutation, Error, MAX_COMPANY_SETUP_BODY, company_setup_body_schema,
};
use serde::Deserialize;
use serde_json::{Value, json};
pub fn accepts(s: &str) -> bool {
    matches!(
        s,
        "company_profile" | "update_company_profile" | "create_account" | "confirm_account_role"
    )
}
pub fn tools() -> Vec<Value> {
    let mut out = vec![tool(
        "company_profile",
        "CompanyRead: full current native company profile, VAT cadence/payment/obligation facts unchanged. Existing accounts tool reads chart and explicit role resolutions/proposals. No query/confirmation.",
        json!({"type":"object","required":["company"],"properties":{"company":company_schema()},"additionalProperties":false}),
        true,
    )];
    for (name, op) in [
        (
            "update_company_profile",
            CompanySetupMutation::UpdateProfile,
        ),
        ("create_account", CompanySetupMutation::CreateAccount),
        (
            "confirm_account_role",
            CompanySetupMutation::ConfirmAccountRole,
        ),
    ] {
        let mut body = native_body(company_setup_body_schema(op));
        body["maxLength"] = json!(MAX_COMPANY_SETUP_BODY);
        out.push(tool(name,"CompanyAdmin: exact native16KiB body once, outer confirm:true. Profile PATCH/create-account bodies have NO inner confirm; role requires inner confirm:true and explicitly chosen role/accountNo from current chart. Server validates current compatibility/VAT cadence/payment restrictions and duplicate accounts. Preserve full authoritative response, incomplete/ambiguous role facts. No inferred bank/VAT/sales/creditor defaults, new-company operation, keys, journals or automatic retries. Unknown outcomes require company_profile/accounts reads before further action; current same-role resolution is not a caller-key receipt.",json!({"type":"object","required":["company","body","confirm"],"properties":{"company":company_schema(),"body":body,"confirm":{"const":true}},"additionalProperties":false}),false));
    }
    out
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Read {
    company: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Write {
    company: String,
    body: String,
    confirm: bool,
}
pub async fn call(
    api: &ApiClient,
    name: &str,
    args: Value,
) -> Result<Result<Value, Error>, &'static str> {
    if name == "company_profile" {
        let r: Read =
            serde_json::from_value(args).map_err(|_| "company_profile requires only company")?;
        return Ok(api.company_profile(&r.company).await);
    }
    let r: Write =
        serde_json::from_value(args).map_err(|_| "setup requires only company/body/confirm")?;
    if r.body.len() > MAX_COMPANY_SETUP_BODY {
        return Err("setup body exceeds native16KiB");
    }
    let op = match name {
        "update_company_profile" => CompanySetupMutation::UpdateProfile,
        "create_account" => CompanySetupMutation::CreateAccount,
        _ => CompanySetupMutation::ConfirmAccountRole,
    };
    Ok(api
        .company_setup_mutation(&r.company, op, r.body.into_bytes(), r.confirm)
        .await)
}
