//! Source-compatible flat backup tools; native backup_status/backup_lock stay separate.
use crate::diagnostics_original118::{OUTPUT_SCHEMA, failure, result};
use rentemester_client::ApiClient;
use serde_json::{Value, json};

pub fn accepts(name: &str) -> bool {
    matches!(name, "system_backup_status" | "system_backup_lock")
}
pub fn tools() -> Vec<Value> {
    [false, true].into_iter().map(|lock| {
        let mut schema = json!({"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{"company":{"type":"string","minLength":1,"description":"Absolute path to the company directory, or a workspace slug."}},"required":["company"],"additionalProperties":false});
        if lock {
            schema["properties"]["enforced"] = json!({"type":"boolean","description":"Whether the lock is enforced (true) or just monitored (false). When enforced AND the weekly backup is overdue beyond graceDays, subsequent bookkeeping writes return code:'BACKUP_LOCKED'. Default: keep existing setting (or false on a fresh ledger)."});
            schema["properties"]["graceDays"] = json!({"type":"number","description":"Days of grace AFTER the weekly backup deadline before the lock activates. Integer ≥ 0. Default: keep existing setting (initial: 3)."});
            schema["properties"]["at"] = json!({"type":"string","description":"Optional ISO-8601 timestamp recorded as the change time (default: now UTC)."});
            schema["properties"]["confirm"] = json!({"type":"boolean","description":"Must be set to true to acknowledge the write side effects of this tool. Omitting it (or sending false) returns an { ok:false, errors:[...] } envelope rather than performing the write."});
        } else {
            schema["properties"]["asOf"] = json!({"type":"string","description":"Optional YYYY-MM-DD date to evaluate the backup compliance against (default: today UTC). Use this to ask 'were we compliant on 2026-03-31?'"});
        }
        json!({"name":if lock {"system_backup_lock"} else {"system_backup_status"},"title":if lock {"Configure the opt-in bookkeeping lock"} else {"Backup compliance status"},"description":if lock {"Konfigurerer den frivillige bogførings-lås. Slået til blokeres ny bogføring hvis den ugentlige backup (BEK 205/2024 § 4) er forsømt ud over grace-perioden. write-irreversible."} else {"Tjekker om backup-pligten er opfyldt. Read-only."},"inputSchema":schema,"outputSchema":serde_json::from_str::<Value>(OUTPUT_SCHEMA).expect("original envelope schema"),"annotations":{"readOnlyHint":!lock,"destructiveHint":false,"idempotentHint":!lock,"openWorldHint":false}})
    }).collect()
}
pub(crate) fn wrap(mut core: Value) -> Value {
    let fields = core.as_object_mut().expect("SDK validated original core");
    let ok = fields.remove("ok").unwrap();
    let errors = fields.remove("errors").unwrap();
    let rules = fields.remove("appliedRules").unwrap();
    let mut envelope = json!({"ok":ok,"errors":errors,"appliedRules":rules});
    if ok == true {
        envelope["data"] = core;
    }
    result(envelope)
}
pub async fn call(api: &ApiClient, name: &str, args: Value) -> Result<Value, &'static str> {
    let lock = name == "system_backup_lock";
    let valid = args.as_object().is_some_and(|o| {
        o.get("company")
            .is_some_and(|v| v.as_str().is_some_and(|s| !s.is_empty()))
            && o.iter().all(|(k, v)| match k.as_str() {
                "company" => v.is_string(),
                "asOf" if !lock => v.is_string(),
                "at" if lock => v.is_string(),
                "enforced" | "confirm" if lock => v.is_boolean(),
                "graceDays" if lock => v.is_number(),
                _ => false,
            })
    });
    if !valid {
        return Ok(json!({"isError":true,"content":[{"type":"text","text":format!("MCP error -32602: Input validation error: Invalid arguments for tool {name}")}]}));
    }
    if lock && args["confirm"] != true {
        return Ok(result(
            json!({"ok":false,"errors":["confirm: true required for write tool system_backup_lock"],"code":"CONFIRM_REQUIRED"}),
        ));
    }
    let company = args["company"].as_str().unwrap();
    let core = if lock {
        let mut body = args.clone();
        body.as_object_mut().unwrap().remove("company");
        api.system_backup_lock_original122(company, serde_json::to_vec(&body).unwrap(), true)
            .await
    } else {
        api.system_backup_status_original122(company, args.get("asOf").and_then(Value::as_str))
            .await
    };
    Ok(match core {
        Ok(core) => wrap(core),
        Err(error) => failure(error),
    })
}
