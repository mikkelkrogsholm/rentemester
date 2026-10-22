//! Original recurring tools keep the source envelope and derive authority from credentials.
use rentemester_client::{
    ApiClient, Error, OriginalRecurringKind as Kind, recurring_original_schema,
    validate_recurring_original,
};
use serde_json::{Value, json};
pub fn accepts(name: &str) -> bool {
    matches!(
        name,
        "recurring_invoice_create"
            | "recurring_invoice_generate"
            | "recurring_invoice_list"
            | "recurring_invoice_run_workspace"
    )
}
pub fn tools() -> Vec<Value> {
    let mut tools = vec![];
    for (name, kind) in [
        ("recurring_invoice_create", Some(Kind::Create)),
        ("recurring_invoice_generate", Some(Kind::Generate)),
        ("recurring_invoice_list", None),
    ] {
        let mut schema=kind.map(recurring_original_schema).unwrap_or_else(||json!({"type":"object","required":[],"properties":{"includeInactive":{"type":"boolean"}},"additionalProperties":false}));
        schema["properties"]["company"] = crate::tools::company_schema();
        schema["required"]
            .as_array_mut()
            .unwrap()
            .push(json!("company"));
        if kind.is_some() {
            schema["properties"]["confirm"] = json!({"type":"boolean"});
            schema["required"]
                .as_array_mut()
                .unwrap()
                .retain(|k| k != "confirm");
        }
        let mut tool = crate::tools::tool(
            name,
            "Original structured recurring template lifecycle. Read defaults to active templates; includeInactive preserves retired history. Create freezes original invoice parties, lines, totals and tax assertions; defaults intervalCount1/manual/paymentTermsDays30/issue_month. Generate derives dates and invoice number server-side and deduplicates template/period. Mutations require confirm:true and current CompanyDraftWrite; list CompanyRead. Template channel records manual/email/digisense selection; create/generate never deliver, post, schedule or retry. Unknown writes require canonical reads before further action. Explicit company slug and current cookie/service credentials; no actor or local path authority.",
            schema,
            kind.is_none(),
        );
        tool["annotations"] = json!({"readOnlyHint":kind.is_none(),"destructiveHint":false,"idempotentHint":kind!=Some(Kind::Create),"openWorldHint":false});
        tool["outputSchema"] = json!({"type":"object","required":["ok","errors"],"properties":{"ok":{"type":"boolean"},"data":{"type":"object"},"errors":{"type":"array","items":{"type":"string"}},"appliedRules":{"type":"array","items":{"type":"string"}},"code":{"type":"string"}},"additionalProperties":false});
        tools.push(tool);
    }
    let mut schema = rentemester_client::recurring_workspace_schema();
    schema["properties"]["confirm"] = json!({"type":"boolean"});
    schema["required"]
        .as_array_mut()
        .unwrap()
        .retain(|v| v != "confirm");
    let mut workspace = crate::tools::tool(
        "recurring_invoice_run_workspace",
        "Explicit capped run of configured workspace recurring invoices. Current human cookie required, never service impersonation. Workspace asserts server identity or exact configured absolute root, never arbitrary filesystem access. Server selects admitted companies, skips archived/uninitialized, retains per-company failures and continuation. Manual generation, configured email dryrun only, safe configured public delivery/status; no timer or automatic retry. confirm:true required. Unknown/partial outcomes require reconciliation before any further explicit action.",
        schema,
        false,
    );
    workspace["annotations"] = json!({"readOnlyHint":false,"destructiveHint":false,"idempotentHint":true,"openWorldHint":false});
    workspace["outputSchema"] = json!({"type":"object","required":["ok","errors"],"properties":{"ok":{"type":"boolean"},"data":{"type":"object"},"errors":{"type":"array","items":{"type":"string"}},"code":{"type":"string"}},"additionalProperties":false});
    tools.push(workspace);
    tools
}
fn result(value: Value) -> Value {
    json!({"content":[{"type":"text","text":value.to_string()}],"structuredContent":value,"isError":value["ok"]!=true})
}
fn wrap(mut core: Value) -> Value {
    let fields = core.as_object_mut().unwrap();
    let ok = fields.remove("ok").unwrap();
    let errors = fields.remove("errors").unwrap();
    let rules = fields.remove("appliedRules");
    let mut envelope = json!({"ok":ok,"errors":errors});
    if let Some(rules) = rules.filter(|v| v.as_array().is_some_and(|a| !a.is_empty())) {
        envelope["appliedRules"] = rules;
    }
    if ok == true {
        envelope["data"] = core;
    } else if let Some(code) = core.get("code") {
        envelope["code"] = code.clone();
    } else {
        let errors = envelope["errors"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase();
        if errors.contains("does not exist") || errors.contains("findes ikke") {
            envelope["code"] = json!("NOT_FOUND");
        } else if errors.contains("falls in closed period")
            || errors.contains("falls in reported period")
        {
            envelope["code"] = json!("PERIOD_CLOSED");
        }
    }
    result(envelope)
}
pub async fn call(api: &ApiClient, name: &str, args: Value) -> Result<Value, &'static str> {
    if name == "recurring_invoice_run_workspace" {
        return call_workspace(api, args).await;
    }
    let mut object = args
        .as_object()
        .ok_or("expected original recurring object")?
        .clone();
    let company = object
        .remove("company")
        .and_then(|v| v.as_str().filter(|s| !s.is_empty()).map(str::to_owned))
        .ok_or("company required")?;
    let outcome = if name == "recurring_invoice_list" {
        if object.keys().any(|k| k != "includeInactive") {
            return Err("only company/includeInactive allowed");
        }
        let include = match object.get("includeInactive") {
            None => false,
            Some(v) => v.as_bool().ok_or("includeInactive must be boolean")?,
        };
        api.recurring_original_list(&company, include).await
    } else {
        let kind = match name {
            "recurring_invoice_create" => Kind::Create,
            "recurring_invoice_generate" => Kind::Generate,
            _ => return Err("unknown recurring tool"),
        };
        if object.get("confirm").is_some_and(|v| !v.is_boolean()) {
            return Err("confirm must be boolean");
        }
        let confirmed = object.get("confirm") == Some(&json!(true));
        object.insert("confirm".into(), json!(true));
        let bytes = serde_json::to_vec(&object).map_err(|_| "recurring payload unavailable")?;
        validate_recurring_original(kind, &bytes)
            .map_err(|_| "original recurring input schema differs")?;
        if !confirmed {
            return Ok(result(
                json!({"ok":false,"code":"CONFIRM_REQUIRED","errors":[format!("confirm: true required for write tool {name}")]}),
            ));
        }
        api.recurring_original(&company, kind, bytes).await
    };
    Ok(match outcome {
        Ok(value) => wrap(value),
        Err(e) => {
            let code = match e {
                Error::InvalidInput(_) => "invalid_input",
                Error::Refused { .. } => "server_refused",
                Error::Transport => "transport",
                Error::Protocol => "protocol",
                Error::OutcomeUnknown => "outcome_unknown",
            };
            result(json!({"ok":false,"code":code,"errors":[e.to_string()]}))
        }
    })
}

async fn call_workspace(api: &ApiClient, args: Value) -> Result<Value, &'static str> {
    let mut object = args
        .as_object()
        .ok_or("expected workspace recurring object")?
        .clone();
    if object.get("confirm").is_some_and(|v| !v.is_boolean()) {
        return Err("confirm must be boolean");
    }
    let confirmed = object.get("confirm") == Some(&json!(true));
    object.insert("confirm".into(), json!(true));
    let bytes = serde_json::to_vec(&object).map_err(|_| "workspace body unavailable")?;
    rentemester_client::validate_recurring_workspace(&bytes)
        .map_err(|_| "workspace recurring schema differs")?;
    if !confirmed {
        return Ok(result(
            json!({"ok":false,"code":"CONFIRM_REQUIRED","errors":["confirm: true is required"]}),
        ));
    }
    Ok(match api.recurring_run_workspace(bytes).await {
        Ok(core) => {
            if core["ok"] == true {
                result(json!({"ok":true,"data":core,"errors":[]}))
            } else {
                let mut data = core.as_object().unwrap().clone();
                let errors = data.remove("errors").unwrap();
                data.remove("ok");
                result(json!({"ok":false,"data":data,"errors":errors}))
            }
        }
        Err(e) => {
            let code = match e {
                Error::InvalidInput(_) => "invalid_input",
                Error::Refused { .. } => "server_refused",
                Error::Transport => "transport",
                Error::Protocol => "protocol",
                Error::OutcomeUnknown => "outcome_unknown",
            };
            result(json!({"ok":false,"code":code,"errors":[e.to_string()]}))
        }
    })
}
