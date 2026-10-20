//! Explicit sensitive workspace export only; no operator keys or filesystem authority.
use crate::tools::tool;
use rentemester_client::{ApiClient, Error};
use serde::Deserialize;
use serde_json::{Value, json};
pub fn tools() -> Vec<Value> {
    let mut result=vec![tool(
        "workspace_backup",
        "Explicitly confirm one POST of exact {confirm:true} to native /api/workspace/backup. Requires current WorkspaceManage and CompanyAdmin for every registered company. Returns exact native artifact bytes in {backup:{contentType,bytes}}, max96MiB. Sensitive full-workspace accounting/original evidence; unsupported coverage or unavailable operator signing configuration may refuse. No company/year/id/body/path/key selectors, local files, verification/restore claim, automatic retry or new accounting entries. Hold trusted public key separately and use operator server CLI for independent verification/fresh restore.",
        json!({"type":"object","required":["confirm"],"properties":{"confirm":{"const":true}},"additionalProperties":false}),
        false,
    )];
    result.push(tool("backup_destinations","List configured backup directories and human attestations; whole-workspace/current human admin required. No inferred offsite/cloud/freshness compliance.",json!({"type":"object","properties":{},"additionalProperties":false}),true));
    for (name,read) in [("place_backup",false),("verify_backup_placement",true)] {
        let mut schema=json!({"type":"object","required":["destinationId","operationId"],"properties":{"destinationId":{"type":"string","minLength":8,"maxLength":64},"operationId":{"type":"string","minLength":8,"maxLength":64}},"additionalProperties":false});
        if !read{schema["properties"]["confirm"]=json!({"const":true});schema["required"].as_array_mut().unwrap().push(json!("confirm"));}
        result.push(tool(name,if read{"Reread exact operation's immutable signed bytes from configured directory; no cloud sync/restore claim. Safe lost-reply reconciliation."}else{"Generate and publish one signed snapshot without overwrite. Current human workspace/all-company admin. Preserve operationId; never retry automatically after uncertainty, use verify_backup_placement. No client path, archive, key or authority input."},schema,read));
    }
    result
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    confirm: bool,
}
pub async fn call(api: &ApiClient, args: Value) -> Result<Result<Value, Error>, &'static str> {
    let input: Input = serde_json::from_value(args)
        .map_err(|_| "workspace_backup requires only confirm:true; no selectors/body/path/key")?;
    Ok(api.workspace_backup(input.confirm).await)
}

pub async fn destination_call(api:&ApiClient,name:&str,args:Value)->Result<Result<Value,Error>,&'static str>{
    if name=="backup_destinations" {if args.as_object().is_none_or(|v|!v.is_empty()){return Err("no destination list arguments");}return Ok(api.backup_destinations().await);}
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields,rename_all="camelCase")]
    struct Selection{destination_id:String,operation_id:String,confirm:Option<bool>}
    let input:Selection=serde_json::from_value(args).map_err(|_|"invalid backup destination arguments")?;
    if name=="place_backup" {if input.confirm!=Some(true){return Err("explicit backup placement confirmation required");}Ok(api.place_backup(&input.destination_id,&input.operation_id,true).await)}else{if input.confirm.is_some(){return Err("verification is read-only");}Ok(api.verify_backup_placement(&input.destination_id,&input.operation_id).await)}
}
