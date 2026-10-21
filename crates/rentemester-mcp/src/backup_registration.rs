use crate::tools::{company_schema, native_body, tool};
use rentemester_client::{
    ApiClient, BackupRegistrationMutation, Error, MAX_BACKUP_REGISTRATION_BODY,
    backup_registration_body_schema,
};
use serde::Deserialize;
use serde_json::{Value, json};
pub fn accepts(name: &str) -> bool {
    matches!(
        name,
        "system_backup_destination_list"
            | "system_backup_destination_add"
            | "system_backup_destination_remove"
            | "system_backup_confirm_placement"
            | "backup_registration_operation"
    )
}
pub fn tools() -> Vec<Value> {
    let mut result = vec![
        tool(
            "system_backup_destination_list",
            "Current human CompanyAdmin: list inert owner destination registrations, immutable history, qualified retained backup selectors, and public operator target metadata. No upload, provider request or filesystem authority.",
            json!({"type":"object","required":["company"],"properties":{"company":company_schema()},"additionalProperties":false}),
            true,
        ),
        tool(
            "backup_registration_operation",
            "Current human CompanyAdmin: reconcile the original backup registration operation ID after an unknown outcome. Returns its immutable receipt or null. Never automatically repeat the write.",
            json!({"type":"object","required":["company","operationId"],"properties":{"company":company_schema(),"operationId":{"type":"string","minLength":8,"maxLength":64}},"additionalProperties":false}),
            true,
        ),
    ];
    for (name, action) in [
        (
            "system_backup_destination_add",
            BackupRegistrationMutation::Add,
        ),
        (
            "system_backup_destination_remove",
            BackupRegistrationMutation::Remove,
        ),
        (
            "system_backup_confirm_placement",
            BackupRegistrationMutation::Declare,
        ),
    ] {
        let mut body = native_body(backup_registration_body_schema(action));
        body["maxLength"] = json!(MAX_BACKUP_REGISTRATION_BODY);
        result.push(tool(name,"Current human CompanyAdmin: explicit inert registration maintenance or unverified owner transfer declaration. Exact16KiB variant JSON string with operationId and confirm:true; no action field (SDK injects it), actor/time/credentials/path/proof authority. Inner and outer confirmation required. Location is descriptive only: no files are transferred or deleted. Declaration never proves an external copy or recovery. Reconcile unknown outcomes with backup_registration_operation using the original ID; no automatic retries.",json!({"type":"object","required":["company","body","confirm"],"properties":{"company":company_schema(),"body":body,"confirm":{"const":true}},"additionalProperties":false}),false));
    }
    result
}
pub async fn call(
    api: &ApiClient,
    name: &str,
    args: Value,
) -> Result<Result<Value, Error>, &'static str> {
    if name == "system_backup_destination_list" {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct List {
            company: String,
        }
        let input: List = serde_json::from_value(args)
            .map_err(|_| "backup destination list accepts only company")?;
        return Ok(api.backup_registrations(&input.company).await);
    }
    if name == "backup_registration_operation" {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields, rename_all = "camelCase")]
        struct Inspect {
            company: String,
            operation_id: String,
        }
        let input: Inspect = serde_json::from_value(args)
            .map_err(|_| "backup registration operation accepts only company and operationId")?;
        return Ok(api
            .backup_registration_operation(&input.company, &input.operation_id)
            .await);
    }
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Write {
        company: String,
        body: String,
        confirm: bool,
    }
    let input: Write = serde_json::from_value(args)
        .map_err(|_| "backup registration mutation accepts only company/body/confirm:true")?;
    Ok(match name {
        "system_backup_destination_add" => {
            api.add_backup_destination(&input.company, input.body.into_bytes(), input.confirm)
                .await
        }
        "system_backup_destination_remove" => {
            api.remove_backup_destination(&input.company, input.body.into_bytes(), input.confirm)
                .await
        }
        "system_backup_confirm_placement" => {
            api.confirm_backup_placement(&input.company, input.body.into_bytes(), input.confirm)
                .await
        }
        _ => return Err("unknown backup registration tool"),
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_public_tool_scope_and_human_only() {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            assert_eq!(tools().len(), 5);
            let api =
                ApiClient::with_service_key("http://127.0.0.1:9", true, "synthetic-service".into())
                    .unwrap();
            assert!(matches!(
                call(
                    &api,
                    "system_backup_destination_list",
                    json!({"company":"synthetic"})
                )
                .await
                .unwrap(),
                Err(Error::InvalidInput(_))
            ));
            for field in ["actor", "asOf", "path", "body", "confirm"] {
                let mut args = json!({"company":"synthetic"});
                args[field] = json!("forged");
                assert!(
                    call(&api, "system_backup_destination_list", args)
                        .await
                        .is_err()
                )
            }
            let t = tools();
            assert_eq!(t[0]["annotations"]["readOnlyHint"], true);
            assert_eq!(t[2]["annotations"]["readOnlyHint"], false);
        });
    }
}
