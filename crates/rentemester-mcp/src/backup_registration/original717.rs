use super::*;
pub fn tool(name: &str) -> Option<Value> {
    let mut schema = ApiClient::backup_views_original717_schema(name);
    if schema.is_null() {
        return None;
    }
    schema["properties"]["company"] = json!({"type":"string","minLength":1,"description":"Absolute path to the company directory, or a workspace slug."});
    schema["required"].as_array_mut()?.push(json!("company"));
    let read = matches!(
        name,
        "system_backup_destination_list" | "system_backup_governance"
    );
    Some(
        json!({"name":name,"title":match name{"system_backup_destination_list"=>"List backup destinations","system_backup_destination_add"=>"Add a backup destination","system_backup_destination_remove"=>"Remove a backup destination","system_backup_governance"=>"Backup governance status",_=>"Record an externally-performed backup placement"},"description":"Source backup governance and destination attestations. Attestations do not grant provider or filesystem authority; declarations are not verified copies.","inputSchema":schema,"outputSchema":serde_json::from_str::<Value>(crate::diagnostics_original118::OUTPUT_SCHEMA).expect("source output schema"),"annotations":{"readOnlyHint":read,"destructiveHint":false,"idempotentHint":read,"openWorldHint":false}}),
    )
}
pub fn tools() -> Vec<Value> {
    [
        "system_backup_destination_list",
        "system_backup_destination_add",
        "system_backup_destination_remove",
        "system_backup_confirm_placement",
    ]
    .into_iter()
    .filter_map(tool)
    .collect()
}
pub async fn call(
    api: &ApiClient,
    name: &str,
    args: Value,
) -> Result<Result<Value, Error>, &'static str> {
    let Some(mut fields) = args.as_object().cloned() else {
        return Err("backup tool arguments must be an object");
    };
    let company = fields
        .remove("company")
        .and_then(|v| v.as_str().map(str::to_owned))
        .filter(|s| !s.is_empty())
        .ok_or("company is required")?;
    let schema = ApiClient::backup_views_original717_schema(name);
    if fields.iter().any(|(k, v)| {
        let s = &schema["properties"][k];
        s.is_null()
            || s["enum"]
                .as_array()
                .is_some_and(|choices| !choices.contains(v))
            || (s["minLength"] == 1 && v.as_str().is_some_and(str::is_empty))
            || match s["type"].as_str() {
                Some("string") => !v.is_string(),
                Some("boolean") => !v.is_boolean(),
                Some("number") => !v.is_number(),
                _ => true,
            }
    }) || schema["required"].as_array().is_some_and(|a| {
        a.iter()
            .any(|k| !fields.contains_key(k.as_str().unwrap_or("")))
    }) {
        return Err("invalid source backup fields");
    }
    let read = matches!(
        name,
        "system_backup_destination_list" | "system_backup_governance"
    );
    if !read && fields.get("confirm") != Some(&json!(true)) {
        return Ok(Ok(crate::diagnostics_original118::result(
            json!({"ok":false,"code":"CONFIRM_REQUIRED","errors":[format!("confirm: true required for write tool {name}")]}),
        )));
    }
    let core = api
        .backup_views_original717(name, &company, Value::Object(fields))
        .await;
    Ok(core.map(|mut c| {
        if name == "system_backup_destination_list" && c["ok"] == true {
            c["errors"] = json!([]);
            c["appliedRules"] = json!([]);
        }
        crate::backup_original122::wrap(c)
    }))
}

/// Root protocol owner must dispatch source calls here, avoiding the native outer wrapper.
pub async fn source_call(api: &ApiClient, name: &str, args: Value) -> Result<Value, &'static str> {
    match call(api, name, args).await? {
        Ok(result) => Ok(result),
        Err(e) => Ok(crate::diagnostics_original118::failure(e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original717_backup_mcp_confirmation_shape_and_annotations() {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let api = ApiClient::with_service_key("http://127.0.0.1:9", true, "synthetic".into())
                .unwrap();
            for confirm in [None, Some(false)] {
                let mut args = json!({"company":"synthetic","id":"synthetic"});
                if let Some(c) = confirm {
                    args["confirm"] = json!(c);
                }
                let r = source_call(&api, "system_backup_destination_remove", args)
                    .await
                    .unwrap();
                assert_eq!(r["structuredContent"]["code"], "CONFIRM_REQUIRED");
            }
            assert!(
                source_call(
                    &api,
                    "system_backup_destination_remove",
                    json!({"company":"synthetic","id":"synthetic","confirm":"true"})
                )
                .await
                .is_err()
            );
            let t = tool("system_backup_destination_add").unwrap();
            assert_eq!(t["annotations"]["idempotentHint"], false);
            assert!(t["inputSchema"]["properties"].get("body").is_none());
            let t = tool("system_backup_governance").unwrap();
            assert_eq!(t["annotations"]["readOnlyHint"], true);
            assert_eq!(t["inputSchema"]["properties"]["asOf"]["type"], "string");
        });
    }
}
