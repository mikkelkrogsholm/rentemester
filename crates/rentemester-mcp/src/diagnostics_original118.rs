//! Original flat diagnostic tools, distinct from native retention/aggregate health.
use rentemester_client::{ApiClient, Error};
use serde_json::{Value, json};

const OUTPUT_SCHEMA: &str = r###"{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{"ok":{"type":"boolean","description":"true ⇒ kaldet lykkedes og `data` er sat; false ⇒ se `errors`."},"data":{"description":"Kerne-resultatet ved ok=true. Den præcise feltliste er per-tool og dokumenteret i docs/mcp-tool-surface.md. Udeladt ved ok=false.","type":"object","properties":{},"additionalProperties":{}},"errors":{"type":"array","items":{"type":"string"},"description":"Menneskelæsbare fejl-/forudsætnings-strenge. Tom ved ok=true; ikke-tom ved ok=false."},"appliedRules":{"description":"Regel-id'er der fyrede for denne handling (sporbarhed). Sættes for bogførings-tools; udeladt ellers.","type":"array","items":{"type":"string"}},"code":{"description":"Stabil maskinlæsbar fejl-markør for cross-cutting preconditions (fx \"BACKUP_LOCKED\"). Sættes kun ved ok=false og kun for fejl der lever uden for det enkelte tools forretningslogik — agenten kan branche på `code` uden at parse fri-tekst i `errors[]`. Udeladt ved per-tool forretningsfejl.","type":"string"}},"required":["ok","errors"],"additionalProperties":false}"###;
pub fn accepts(name: &str) -> bool {
    matches!(name, "retention_status" | "system_healthcheck")
}
pub fn tools() -> Vec<Value> {
    [("retention_status", "Retention status report", "Viser opbevaringsfrister og udløbet materiale. Read-only."),
     ("system_healthcheck", "Company directory healthcheck", "Tjekker at virksomhedsmappen og kernefilerne findes — company_root, data_dir, ledger, documents, config. Returnerer envelope.data.checks med { name, ok } pr. tjek og envelope.data.missing med navnene på dem der mangler. Read-only.")]
        .into_iter().map(|(name,title,description)| {
            let mut schema = json!({"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{"company":{"type":"string","minLength":1,"description":"Absolute path to the company directory, or a workspace slug."}},"required":["company"],"additionalProperties":false});
            if name == "retention_status" { schema["properties"]["asOf"] = json!({"type":"string"}); }
            json!({"name":name,"title":title,"description":description,"inputSchema":schema,"outputSchema":serde_json::from_str::<Value>(OUTPUT_SCHEMA).expect("original envelope schema"),"annotations":{"readOnlyHint":true,"destructiveHint":false,"idempotentHint":true,"openWorldHint":false}})
        }).collect()
}
fn result(envelope: Value) -> Value {
    json!({"isError":envelope["ok"] != true,"content":[{"type":"text","text":envelope.to_string()}],"structuredContent":envelope})
}
fn failure(error: Error) -> Value {
    let code = match error {
        Error::InvalidInput(_) => "invalid_input",
        Error::Refused { .. } => "server_refused",
        Error::Transport => "transport",
        Error::Protocol => "protocol",
        Error::OutcomeUnknown => "outcome_unknown",
    };
    result(json!({"ok":false,"errors":[error.to_string()],"code":code}))
}
pub async fn call(api: &ApiClient, name: &str, args: Value) -> Result<Value, &'static str> {
    let retention = name == "retention_status";
    let valid = args.as_object().is_some_and(|o| {
        o.get("company")
            .is_some_and(|v| v.as_str().is_some_and(|s| !s.is_empty()))
            && o.iter()
                .all(|(k, v)| (k == "company" || retention && k == "asOf") && v.is_string())
    });
    if !valid {
        return Ok(
            json!({"isError":true,"content":[{"type":"text","text":format!("MCP error -32602: Input validation error: Invalid arguments for tool {name}")}]}),
        );
    }
    let core = if retention {
        api.retention_status_original118(
            args["company"].as_str().unwrap(),
            args.get("asOf").and_then(Value::as_str),
        )
        .await
    } else {
        api.system_healthcheck_original118(args["company"].as_str().unwrap())
            .await
    };
    Ok(match core {
        Err(error) => failure(error),
        Ok(mut core) => {
            let mut envelope = json!({"ok":core["ok"],"errors":core["errors"]});
            core.as_object_mut().unwrap().remove("errors");
            if retention {
                core.as_object_mut().unwrap().remove("ok");
                if let Some(rules) = core.as_object_mut().unwrap().remove("appliedRules") {
                    if rules.as_array().is_some_and(|a| !a.is_empty()) {
                        envelope["appliedRules"] = rules;
                    }
                }
                if envelope["ok"] == true {
                    envelope["data"] = core;
                }
            } else {
                // Health diagnostic false intentionally retains all meaningful evidence.
                envelope["data"] = core;
            }
            result(envelope)
        }
    })
}
