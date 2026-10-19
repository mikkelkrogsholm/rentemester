use rentemester_client::{
    ApiClient, MAX_SERVICE_PRINCIPAL_BODY, ServicePrincipalMutation, service_principal_body_schema,
};
use rentemester_engine::Error;
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::Read};
type Failure = (u8, Error);
fn invalid(s: &str) -> Failure {
    (2, Error::new("invalid_input", s))
}
pub fn accepts(command: &str) -> bool {
    matches!(
        command,
        "service-principals"
            | "create-service-principal"
            | "rotate-service-principal"
            | "revoke-service-principal"
            | "recover-service-principal"
    )
}
pub fn help() -> Value {
    json!({"commands":["service-principals", "create-service-principal", "rotate-service-principal", "revoke-service-principal", "recover-service-principal"],"policy":"Current human workspace owner; list current MFA, mutations fresh MFA. Exact native8192-byte JSON stdin + --confirm, inner confirm:true. Caller may supply operationId for create/rotate/revoke and MUST supply it for recover. Preserve original operationId after unknown issuance: recover explicitly, never repeat create/rotate and never manufacture a replacement operationId. Successful create/rotate output intentionally contains a ONE-TIME secret; route stdout to private storage, no secret in argv/URLs/body/config/logs. Recover does not redisclose it. No implicit company grants, permissions, expiry default, namespace, service executor or automatic retry.","bodySchemas":{"create-service-principal":service_principal_body_schema(ServicePrincipalMutation::Create),"rotate-service-principal":service_principal_body_schema(ServicePrincipalMutation::Rotate),"revoke-service-principal":service_principal_body_schema(ServicePrincipalMutation::Revoke),"recover-service-principal":service_principal_body_schema(ServicePrincipalMutation::Recover)}})
}
pub fn run(
    command: &str,
    flags: &BTreeMap<&str, &str>,
    allow: bool,
    confirmed: bool,
    preview: bool,
) -> Result<Value, Failure> {
    let read = command == "service-principals";
    if confirmed == read
        || preview
        || flags
            .keys()
            .any(|key| !(["--server", "--cookie-file"].contains(key)))
    {
        return Err(invalid(
            "command-specific selectors required; readonly has no confirmation and writes require exact confirmed stdin body",
        ));
    }

    let server = flags
        .get("--server")
        .ok_or_else(|| invalid("explicit --server required"))?;
    let path = flags
        .get("--cookie-file")
        .ok_or_else(|| invalid("private MFA --cookie-file required"))?;
    let cookie = rentemester_client::read_cookie_file(std::path::Path::new(path))
        .map_err(crate::remote::client_error)?;
    let api = ApiClient::new(server, allow, Some(cookie)).map_err(crate::remote::client_error)?;
    let mut bytes = vec![];
    if !read {
        std::io::stdin()
            .take(MAX_SERVICE_PRINCIPAL_BODY as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| invalid("could not read exact native body"))?;
    }
    let runtime =
        tokio::runtime::Runtime::new().map_err(|_| invalid("could not start HTTP runtime"))?;
    let result = runtime
        .block_on(async {
            if read {
                api.service_principals().await
            } else {
                let action = match command {
                    "create-service-principal" => ServicePrincipalMutation::Create,
                    "rotate-service-principal" => ServicePrincipalMutation::Rotate,
                    "revoke-service-principal" => ServicePrincipalMutation::Revoke,
                    "recover-service-principal" => ServicePrincipalMutation::Recover,
                    _ => return Err(rentemester_client::Error::InvalidInput("unknown operation")),
                };
                api.mutate_service_principal(action, bytes, confirmed).await
            }
        })
        .map_err(crate::remote::client_error)?;
    Ok(json!({"ok":result.get("ok").and_then(Value::as_bool).unwrap_or(true),"data":result}))
}
