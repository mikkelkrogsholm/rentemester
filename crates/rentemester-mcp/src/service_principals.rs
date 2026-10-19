use crate::tools::{native_body, tool};
use rentemester_client::{
    ApiClient, Error, MAX_SERVICE_PRINCIPAL_BODY, ServicePrincipalMutation as Op,
    service_principal_body_schema,
};
use serde::Deserialize;
use serde_json::{Value, json};
pub fn accepts(name: &str) -> bool {
    matches!(
        name,
        "service_principals"
            | "create_service_principal"
            | "rotate_service_principal"
            | "revoke_service_principal"
            | "recover_service_principal"
    )
}
pub fn tools() -> Vec<Value> {
    let mut tools = vec![tool(
        "service_principals",
        "Current verified MFA human workspace owner lists registered service metadata only. No secrets, company selector/body/confirm, namespace assumptions, issuance or grants.",
        json!({"type":"object","properties":{},"additionalProperties":false}),
        true,
    )];
    for (name, action) in [
        ("create_service_principal", Op::Create),
        ("rotate_service_principal", Op::Rotate),
        ("revoke_service_principal", Op::Revoke),
        ("recover_service_principal", Op::Recover),
    ] {
        let mut body = native_body(service_principal_body_schema(action));
        body["maxLength"] = json!(MAX_SERVICE_PRINCIPAL_BODY);
        tools.push(tool(name,"Fresh MFA human workspace owner; exact native8192-byte body, inner+outer confirm:true. Optional caller operationId for create/rotate/revoke; required for explicit recovery after uncertain outcome. Never replay issuance, generate replacement operationId or claim redisclosure. Successful create/rotate intentionally outputs ONE-TIME secret; treat the receipt as private. No supplied credentials, company grants, permission/expiry defaults or automatic retries. Recovery preserves canonical lifecycle receipt without redisclosing secret.",json!({"type":"object","required":["body","confirm"],"properties":{"body":body,"confirm":{"const":true}},"additionalProperties":false}),false));
    }
    tools
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Empty {}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Write {
    body: String,
    confirm: bool,
}
pub async fn call(
    api: &ApiClient,
    name: &str,
    args: Value,
) -> Result<Result<Value, Error>, &'static str> {
    if name == "service_principals" {
        let _: Empty =
            serde_json::from_value(args).map_err(|_| "service_principals accepts no arguments")?;
        return Ok(api.service_principals().await);
    }
    let v: Write =
        serde_json::from_value(args).map_err(|_| "service lifecycle requires only body/confirm")?;
    if v.body.len() > MAX_SERVICE_PRINCIPAL_BODY {
        return Err("service lifecycle exceeds native8192 bytes");
    }
    let op = match name {
        "create_service_principal" => Op::Create,
        "rotate_service_principal" => Op::Rotate,
        "revoke_service_principal" => Op::Revoke,
        "recover_service_principal" => Op::Recover,
        _ => return Err("unknown lifecycle operation"),
    };
    Ok(api
        .mutate_service_principal(op, v.body.into_bytes(), v.confirm)
        .await)
}
