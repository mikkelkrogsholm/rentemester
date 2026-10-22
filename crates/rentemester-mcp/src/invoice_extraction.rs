//! The two existing original extraction tools; no native registration aliases.
use crate::tools::{company_schema, tool};
use rentemester_client::{ApiClient, Error, original_invoice_extraction_surface_schema};
use serde_json::{Value, json};

fn object(p: Value, r: &[&str]) -> Value {
    json!({"type":"object","properties":p,"required":r,"additionalProperties":false})
}
pub fn tools() -> Vec<Value> {
    [("documents_invoice_extraction",true),("documents_extract_invoice",false)].into_iter().map(|(name,read)| {
        let mut p=json!({"company":company_schema(),"documentId":{"type":"integer","minimum":1,"maximum":9007199254740991u64}});
        if !read {p["confirm"]=json!({"type":"boolean"});}
        let mut t=tool(name,if read {"Read retained original invoice extraction evidence; absent evidence is null. Never calls a provider or changes originals, metadata or books."} else {"Explicitly extract a stored PDF with confirm:true and current company master-data authority. No inferred metadata or posting. Reconcile outcome_unknown by reading; never automatically retry."},object(p,&["company","documentId"]),read);
        t["annotations"]=json!({"readOnlyHint":read,"destructiveHint":false,"idempotentHint":true,"openWorldHint":false});
        let errors=json!({"type":"array","items":{"type":"string"}});
        t["outputSchema"]=json!({"oneOf":[
            object(json!({"ok":{"const":true},"errors":errors,"data":object(json!({"extraction":original_invoice_extraction_surface_schema()}),&["extraction"])}),&["ok","errors","data"]),
            object(json!({"ok":{"const":false},"errors":errors,"code":{"type":"string"}}),&["ok","errors"])
        ]});
        t
    }).collect()
}
fn result(e: Value) -> Value {
    json!({"isError":e["ok"]!=true,"content":[{"type":"text","text":e.to_string()}],"structuredContent":e})
}
pub async fn call(api: &ApiClient, name: &str, args: Value) -> Result<Value, &'static str> {
    let read = name == "documents_invoice_extraction";
    if !read && name != "documents_extract_invoice" {
        return Err("unknown extraction tool");
    }
    let o = args.as_object().ok_or("expected extraction arguments")?;
    if o.keys()
        .any(|k| !["company", "documentId"].contains(&k.as_str()) && (read || k != "confirm"))
        || args.get("confirm").is_some_and(|v| !v.is_boolean())
    {
        return Err("unknown or malformed extraction arguments");
    }
    let company = args["company"]
        .as_str()
        .filter(|s| {
            !s.is_empty()
                && s.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        })
        .ok_or("expected company slug")?;
    let id = args["documentId"]
        .as_i64()
        .filter(|n| (1..=9_007_199_254_740_991).contains(n))
        .ok_or("expected positive exact documentId")?;
    if !read && args["confirm"] != true {
        return Ok(result(
            json!({"ok":false,"errors":[format!("confirm: true required for write tool {name}")],"code":"CONFIRM_REQUIRED"}),
        ));
    }
    let outcome = if read {
        api.original_invoice_extraction(company, id).await
    } else {
        api.original_extract_invoice(company, id, true, None).await
    };
    Ok(result(match outcome {
        Ok(core) if core["ok"] == true => {
            json!({"ok":true,"errors":[],"data":{"extraction":core["extraction"]}})
        }
        Ok(core) => core,
        Err(error) => {
            let code = match error {
                Error::InvalidInput(_) => "invalid_input",
                Error::Refused { .. } => "server_refused",
                Error::Transport => "transport",
                Error::Protocol => "protocol",
                Error::OutcomeUnknown => "outcome_unknown",
            };
            json!({"ok":false,"code":code,"errors":[error.to_string()]})
        }
    }))
}
