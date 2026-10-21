//! Typed human-only whole-workspace remote verification; no provider authority inputs.
use crate::tools::tool;
use rentemester_client::{ApiClient, Error};
use serde::Deserialize;
use serde_json::{Value, json};
pub fn accepts(name: &str) -> bool {
    matches!(name, "verify_backup_remote" | "backup_remote_verification")
}
pub fn tools() -> Vec<Value> {
    let id = json!({"type":"string","minLength":8,"maxLength":64,"pattern":"^[a-z0-9-]{8,64}$"});
    vec![
        tool(
            "verify_backup_remote",
            "Explicit current human MFA workspace/all-company admin verification of an existing placement against an operator-bound Google Drive object. Reads metadata and hashes downloaded bytes, then retains the exact receipt. No caller provider URL/token/folder/path/key/archive claims. Preserve operationId; after an unknown result read backup_remote_verification with the same destination/operation. Never automatically POST again. This does not assert independent attestation or current offsite availability.",
            json!({"type":"object","required":["destinationId","operationId","placementOperationId","remoteObjectId","confirm"],"properties":{"destinationId":id.clone(),"operationId":id.clone(),"placementOperationId":id.clone(),"remoteObjectId":{"type":"string","minLength":1,"maxLength":160,"pattern":"^[A-Za-z0-9_-]{1,160}$"},"confirm":{"const":true}},"additionalProperties":false}),
            false,
        ),
        tool(
            "backup_remote_verification",
            "Read an exact retained remote-verification operation under current human MFA workspace/all-company admin authority. Returns the original timestamped verification receipt or null. No provider call, retry, local path, restore claim or implication the remote copy is still available.",
            json!({"type":"object","required":["destinationId","operationId"],"properties":{"destinationId":id.clone(),"operationId":id},"additionalProperties":false}),
            true,
        ),
    ]
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Verify {
    destination_id: String,
    operation_id: String,
    placement_operation_id: String,
    remote_object_id: String,
    confirm: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Inspect {
    destination_id: String,
    operation_id: String,
}
pub async fn call(
    api: &ApiClient,
    name: &str,
    args: Value,
) -> Result<Result<Value, Error>, &'static str> {
    match name {
        "verify_backup_remote" => {
            let input:Verify=serde_json::from_value(args).map_err(|_|"verify_backup_remote requires only destinationId/operationId/placementOperationId/remoteObjectId/confirm:true")?;
            if !input.confirm {
                return Err("explicit remote verification confirmation required");
            }
            Ok(api
                .verify_backup_remote(
                    &input.destination_id,
                    &input.operation_id,
                    &input.placement_operation_id,
                    &input.remote_object_id,
                    true,
                )
                .await)
        }
        "backup_remote_verification" => {
            let input: Inspect = serde_json::from_value(args)
                .map_err(|_| "backup_remote_verification accepts only destinationId/operationId")?;
            Ok(api
                .backup_remote_verification(&input.destination_id, &input.operation_id)
                .await)
        }
        _ => Err("unknown backup remote verification tool"),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn two_typed_tools_have_precise_read_and_confirmation_contracts() {
        let list = tools();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0]["annotations"]["readOnlyHint"], false);
        assert_eq!(list[1]["annotations"]["readOnlyHint"], true);
        for item in &list {
            assert_eq!(item["inputSchema"]["additionalProperties"], false);
            assert!(item["inputSchema"]["properties"].get("company").is_none());
            assert!(item["inputSchema"]["properties"].get("body").is_none());
        }
        assert_eq!(
            list[0]["inputSchema"]["properties"]["confirm"]["const"],
            true
        );
        assert!(
            list[1]["inputSchema"]["properties"]
                .get("confirm")
                .is_none()
        );
    }
    #[test]
    fn unknown_fields_and_unconfirmed_or_service_calls_never_send() {
        tokio::runtime::Runtime::new().unwrap().block_on(async{
   let api=ApiClient::new("http://127.0.0.1:9",true,Some("session=synthetic".into())).unwrap();
   let valid=json!({"destinationId":"drive-0001","operationId":"verify-0001","placementOperationId":"placed-0001","remoteObjectId":"Drive_Object-1","confirm":true});
   for field in ["company","body","providerUrl","folderId","bearerToken","archiveSha256","key","actor"]{let mut args=valid.clone();args[field]=json!("forged");assert!(call(&api,"verify_backup_remote",args).await.is_err());}
   let mut unconfirmed=valid.clone();unconfirmed["confirm"]=json!(false);assert!(call(&api,"verify_backup_remote",unconfirmed).await.is_err());
   for args in [json!({"destinationId":"drive-0001","operationId":"verify-0001","confirm":true}),json!({"destinationId":"drive-0001","operationId":"verify-0001","company":"synthetic"})]{assert!(call(&api,"backup_remote_verification",args).await.is_err());}
   let service=ApiClient::with_service_key("http://127.0.0.1:9",true,"synthetic-service".into()).unwrap();assert!(matches!(call(&service,"verify_backup_remote",valid).await.unwrap(),Err(Error::InvalidInput(_))));
   assert!(matches!(call(&api,"backup_remote_verification",json!({"destinationId":"drive-0001","operationId":"BAD"})).await.unwrap(),Err(Error::InvalidInput(_))));
  });
    }
}
