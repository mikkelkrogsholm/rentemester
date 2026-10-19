use rentemester_client::{
    ApiClient, MAX_WORKSPACE_TRANSFER_BODY, WorkspaceTransferMutation as Op,
    restored_receipt_body_schema, workspace_transfer_body_schema,
};
use rentemester_engine::Error;
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::Read};
type Failure = (u8, Error);
fn invalid(message: &str) -> Failure {
    (2, Error::new("invalid_input", message))
}
pub fn accepts(command: &str) -> bool {
    matches!(
        command,
        "workspace-transfer-status"
            | "workspace-transfer-freeze"
            | "workspace-transfer-cancel"
            | "workspace-transfer-export"
            | "workspace-transfer-release"
            | "workspace-transfer-activate"
            | "restored-receipts"
            | "reconcile-restored-receipt"
    )
}
fn action(command: &str) -> Option<Op> {
    match command {
        "workspace-transfer-freeze" => Some(Op::Freeze),
        "workspace-transfer-cancel" => Some(Op::Cancel),
        "workspace-transfer-export" => Some(Op::Export),
        "workspace-transfer-release" => Some(Op::Release),
        "workspace-transfer-activate" => Some(Op::Activate),
        _ => None,
    }
}
pub fn help() -> Value {
    let mut schemas = json!({"reconcile-restored-receipt":restored_receipt_body_schema()});
    for (name, op) in [
        ("workspace-transfer-freeze", Op::Freeze),
        ("workspace-transfer-cancel", Op::Cancel),
        ("workspace-transfer-export", Op::Export),
        ("workspace-transfer-release", Op::Release),
        ("workspace-transfer-activate", Op::Activate),
    ] {
        schemas[name] = workspace_transfer_body_schema(op);
    }
    json!({"commands":["workspace-transfer-status","workspace-transfer-freeze","workspace-transfer-cancel","workspace-transfer-export","workspace-transfer-release","workspace-transfer-activate","restored-receipts","reconcile-restored-receipt"],"policy":"Workspace operations have no company selector; restored-receipts and reconcile-restored-receipt require --company. Status/inventory GETs never activate/write/dispatch and reject --confirm. All writes use exact stdin JSON with inner+outer confirm and current expectedStateHash:16KiB except release/activate32KiB. Export returns backup.contentType/exact integer bytes<=96MiB, creates no files or verification claim; an offered archive is recorded before download. Source release is permanent moved authorization, not proof destination activated. Target activation requires separately operator-trusted source key and exact destination proof, never incoming key trust. Receipt reconciliation maps only current new human, no global identity alias or posting. No operator/config secrets, generated IDs/hashes/defaults, implicit dispatch/rollback/thaw or automatic retry. Network/5xx outcome_unknown must be reconciled through canonical status before manual action; known codes/projections remain intact.","bodySchemas":schemas})
}
pub fn run(
    command: &str,
    flags: &BTreeMap<&str, &str>,
    allow: bool,
    confirmed: bool,
    preview: bool,
) -> Result<Value, Failure> {
    let read = matches!(command, "workspace-transfer-status" | "restored-receipts");
    let company_scope = matches!(command, "restored-receipts" | "reconcile-restored-receipt");
    if preview
        || confirmed == read
        || flags.keys().any(|k| {
            !(["--server", "--cookie-file"].contains(k) || company_scope && *k == "--company")
        })
    {
        return Err(invalid(
            "command-specific transfer scope/selectors and explicit write confirmation required",
        ));
    }
    let company = if company_scope {
        Some(
            *flags
                .get("--company")
                .ok_or_else(|| invalid("explicit --company required for receipt scope"))?,
        )
    } else {
        None
    };
    let server = flags
        .get("--server")
        .ok_or_else(|| invalid("explicit --server required"))?;
    let path = flags
        .get("--cookie-file")
        .ok_or_else(|| invalid("private current human MFA --cookie-file required"))?;
    let cookie = rentemester_client::read_cookie_file(std::path::Path::new(path))
        .map_err(crate::remote::client_error)?;
    let api = ApiClient::new(server, allow, Some(cookie)).map_err(crate::remote::client_error)?;
    let mut bytes = vec![];
    if !read {
        std::io::stdin()
            .take(
                action(command).map_or(MAX_WORKSPACE_TRANSFER_BODY, Op::max_body_bytes) as u64 + 1,
            )
            .read_to_end(&mut bytes)
            .map_err(|_| invalid("could not read exact bounded native transfer body"))?;
    }
    let rt = tokio::runtime::Runtime::new().map_err(|_| invalid("could not start HTTP runtime"))?;
    let value = rt
        .block_on(async {
            match command {
                "workspace-transfer-status" => api.workspace_transfer_status().await,
                "workspace-transfer-freeze" => {
                    api.workspace_transfer_freeze(bytes, confirmed).await
                }
                "workspace-transfer-cancel" => {
                    api.workspace_transfer_cancel(bytes, confirmed).await
                }
                "workspace-transfer-export" => {
                    api.workspace_transfer_export(bytes, confirmed).await
                }
                "workspace-transfer-release" => {
                    api.workspace_transfer_release(bytes, confirmed).await
                }
                "workspace-transfer-activate" => {
                    api.workspace_transfer_activate(bytes, confirmed).await
                }
                "restored-receipts" => api.restored_receipts(company.unwrap()).await,
                "reconcile-restored-receipt" => {
                    api.reconcile_restored_receipt(company.unwrap(), bytes, confirmed)
                        .await
                }
                _ => Err(rentemester_client::Error::InvalidInput(
                    "unknown transfer operation",
                )),
            }
        })
        .map_err(crate::remote::client_error)?;
    Ok(json!({"ok":value.get("ok").and_then(Value::as_bool).unwrap_or(false),"data":value}))
}
