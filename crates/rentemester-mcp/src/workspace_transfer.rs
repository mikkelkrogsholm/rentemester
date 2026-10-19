use crate::tools::{company_schema, native_body, tool};
use rentemester_client::{
    ApiClient, Error, MAX_WORKSPACE_TRANSFER_BODY, WorkspaceTransferMutation as Op,
    restored_receipt_body_schema, workspace_transfer_body_schema,
};
use serde::Deserialize;
use serde_json::{Value, json};
pub fn accepts(name: &str) -> bool {
    matches!(
        name,
        "workspace_transfer_status"
            | "workspace_transfer_freeze"
            | "workspace_transfer_cancel"
            | "workspace_transfer_export"
            | "workspace_transfer_release"
            | "workspace_transfer_activate"
            | "restored_receipts"
            | "reconcile_restored_receipt"
    )
}
pub fn tools() -> Vec<Value> {
    let mut out = vec![
        tool(
            "workspace_transfer_status",
            "Authorized current transfer state/hash/history/challenge/pending effects. GET never writes, freezes, activates or dispatches. No company/body/confirm selectors.",
            json!({"type":"object","properties":{},"additionalProperties":false}),
            true,
        ),
        tool(
            "restored_receipts",
            "Selected-company authorized retained imported journal receipt inventory. No posting, adoption, old authority or reconciliation on read.",
            json!({"type":"object","required":["company"],"properties":{"company":company_schema()},"additionalProperties":false}),
            true,
        ),
    ];
    for (name, op) in [
        ("workspace_transfer_freeze", Some(Op::Freeze)),
        ("workspace_transfer_cancel", Some(Op::Cancel)),
        ("workspace_transfer_export", Some(Op::Export)),
        ("workspace_transfer_release", Some(Op::Release)),
        ("workspace_transfer_activate", Some(Op::Activate)),
        ("reconcile_restored_receipt", None),
    ] {
        let mut body = native_body(
            op.map_or_else(restored_receipt_body_schema, workspace_transfer_body_schema),
        );
        body["maxLength"] = json!(op.map_or(MAX_WORKSPACE_TRANSFER_BODY, Op::max_body_bytes));
        let mut props = json!({"body":body,"confirm":{"const":true}});
        let mut required = vec!["body", "confirm"];
        if op.is_none() {
            props["company"] = company_schema();
            required.push("company");
        }
        out.push(tool(name,"Explicit current human reviewed operation with exact native JSON and inner+outer confirm. Workspace operations require current owner/all-company administration/fresh MFA; receipt reconcile is selected-company current new human only.16KiB except release/activate32KiB. Export returns native backup MIME/integer bytes<=96MiB; records offer before response, no files/verification/restore claim. Source release means moved authorization, not destination active; activation needs separate operator-trusted source public key and exact staged destination proof. No principal aliases, old credentials/grants, generated keys/hashes, automatic thaw/retry/dispatch. Known refusal codes/projections preserved; uncertain outcomes require canonical status before manual action.",json!({"type":"object","required":required,"properties":props,"additionalProperties":false}),false));
    }
    out
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Empty {}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Read {
    company: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Write {
    body: String,
    confirm: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    company: String,
    body: String,
    confirm: bool,
}
pub async fn call(
    api: &ApiClient,
    name: &str,
    args: Value,
) -> Result<Result<Value, Error>, &'static str> {
    if name == "workspace_transfer_status" {
        serde_json::from_value::<Empty>(args)
            .map_err(|_| "workspace transfer status accepts empty arguments only")?;
        return Ok(api.workspace_transfer_status().await);
    }
    if name == "restored_receipts" {
        let v: Read =
            serde_json::from_value(args).map_err(|_| "restored receipts accepts company only")?;
        return Ok(api.restored_receipts(&v.company).await);
    }
    if name == "reconcile_restored_receipt" {
        let v: Receipt = serde_json::from_value(args)
            .map_err(|_| "receipt reconcile accepts company/body/confirm only")?;
        if v.body.len() > MAX_WORKSPACE_TRANSFER_BODY {
            return Err("receipt reconcile body exceeds16KiB");
        }
        return Ok(api
            .reconcile_restored_receipt(&v.company, v.body.into_bytes(), v.confirm)
            .await);
    }
    let v: Write = serde_json::from_value(args)
        .map_err(|_| "workspace transfer write accepts body/confirm only")?;
    let op = match name {
        "workspace_transfer_freeze" => Op::Freeze,
        "workspace_transfer_cancel" => Op::Cancel,
        "workspace_transfer_export" => Op::Export,
        "workspace_transfer_release" => Op::Release,
        "workspace_transfer_activate" => Op::Activate,
        _ => return Err("unknown transfer operation"),
    };
    if v.body.len() > op.max_body_bytes() {
        return Err("transfer body exceeds native operation bound");
    }
    let bytes = v.body.into_bytes();
    Ok(match op {
        Op::Freeze => api.workspace_transfer_freeze(bytes, v.confirm).await,
        Op::Cancel => api.workspace_transfer_cancel(bytes, v.confirm).await,
        Op::Export => api.workspace_transfer_export(bytes, v.confirm).await,
        Op::Release => api.workspace_transfer_release(bytes, v.confirm).await,
        Op::Activate => api.workspace_transfer_activate(bytes, v.confirm).await,
    })
}
