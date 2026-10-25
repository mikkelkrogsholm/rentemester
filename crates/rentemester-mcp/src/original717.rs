//! Work-Issue: #717. Original nine bindings; credentials alone provide authority.
use rentemester_client::{ApiClient, Error};
use serde_json::{Value, json};
pub fn tools() -> Vec<Value> {
    rentemester_client::original717_tools()
}
pub fn accepts(name: &str) -> bool {
    tools().iter().any(|t| t["name"] == name)
}
fn result(envelope: Value) -> Value {
    json!({"isError":envelope["ok"]!=true,"content":[{"type":"text","text":envelope.to_string()}],"structuredContent":envelope})
}
fn wrap(mut core: Value) -> Value {
    let ok = core["ok"] == true;
    let mut envelope = json!({"ok":ok,"errors":core.get("errors").cloned().unwrap_or(json!([]))});
    if let Some(rules) = core["appliedRules"].as_array().filter(|a| !a.is_empty()) {
        envelope["appliedRules"] = json!(
            rules
                .iter()
                .map(|v| v
                    .as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| v.to_string()))
                .collect::<Vec<_>>()
        );
    }
    if !ok {
        let errors = core["errors"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .unwrap_or_default()
            .to_lowercase();
        let code = if matches!(
            core["code"].as_str(),
            Some("backup_overdue" | "BACKUP_LOCKED")
        ) {
            Some("BACKUP_LOCKED")
        } else if errors.contains("falls in closed period")
            || errors.contains("falls in reported period")
            || errors.contains("perioden er lukket")
            || errors.contains("periode er lukket")
            || errors.contains("in a closed period")
        {
            Some("PERIOD_CLOSED")
        } else if errors.contains("forudsætning ikke opfyldt")
            || errors.contains("precondition not met")
            || errors.contains("precondition missing")
            || errors.contains("must be posted")
            || errors.contains("skal være bogf")
            || errors.contains("skal bogf")
        {
            Some("PRECONDITION_MISSING")
        } else if errors.contains("does not exist")
            || errors.contains("findes ikke")
            || errors.contains("no such")
            || errors.contains("kunne ikke findes")
            || errors
                .split_whitespace()
                .collect::<Vec<_>>()
                .windows(3)
                .any(|w| w[0] == "no" && w[2] == "found")
        {
            Some("NOT_FOUND")
        } else if errors.contains("ikke momsregistreret") {
            Some("NOT_VAT_REGISTERED")
        } else {
            None
        };
        if let Some(code) = code {
            envelope["code"] = json!(code);
        }
    }
    if ok {
        if let Some(o) = core.as_object_mut() {
            for key in ["ok", "errors", "appliedRules", "code"] {
                o.remove(key);
            }
        }
        envelope["data"] = core;
    }
    result(envelope)
}
pub async fn call(api: &ApiClient, name: &str, args: Value) -> Result<Value, &'static str> {
    if rentemester_client::validate_original717(name, &args).is_err() {
        return Ok(
            json!({"isError":true,"content":[{"type":"text","text":format!("MCP error -32602: Input validation error: Invalid arguments for tool {name}")}]}),
        );
    }
    if args["confirm"] != true {
        return Ok(result(
            json!({"ok":false,"errors":[format!("confirm: true required for {} tool {name}", if name=="system_restore_backup" {"destructive"} else {"write"})],"code":"CONFIRM_REQUIRED"}),
        ));
    }
    if name == "system_restore_backup" {
        let expected = format!("RESTORE {}", args["targetCompany"].as_str().unwrap());
        let provided = args["confirmText"].as_str().unwrap_or("");
        if provided != expected {
            return Ok(result(
                json!({"ok":false,"errors":[format!("confirmText must match '{expected}' exactly (got: '{provided}')")],"code":"CONFIRMTEXT_MISMATCH"}),
            ));
        }
    }
    Ok(match api.original717(name, &args).await {
        Ok(core) => wrap(core),
        Err(e) => {
            let code = match e {
                Error::InvalidInput(_) => "invalid_input",
                Error::Refused { .. } => "server_refused",
                Error::Transport => "transport",
                Error::Protocol => "protocol",
                Error::OutcomeUnknown => "outcome_unknown",
            };
            // Never include caller secret/config/file arguments in failures.
            result(json!({"ok":false,"errors":[e.to_string()],"code":code}))
        }
    })
}
#[cfg(test)]
#[path = "original717_tests.rs"]
mod tests;
