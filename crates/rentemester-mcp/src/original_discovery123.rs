//! Original discovery tools: metadata only, decorated with the actual running registry.
use rentemester_client::{ApiClient, Error};
use rentemester_engine::agent_discovery::original123;
use serde_json::{Value, json};

pub fn accepts(name: &str) -> bool {
    matches!(name, "meta_about" | "agent_capability_search" | "agent_workflow_describe")
}

pub fn tools() -> Vec<Value> {
    let schema = "https://json-schema.org/draft/2020-12/schema";
    [
        ("meta_about", "Server identification and contract pointers", META_DESCRIPTION,
         json!({"$schema":schema,"type":"object","properties":{},"additionalProperties":false})),
        ("agent_capability_search", "Search supported agent outcomes", "Read-only, paginated search of the versioned Rentemester outcome catalogue. Start with meta_about.",
         json!({"$schema":schema,"type":"object","properties":{"query":{"type":"string","maxLength":200},"cursor":{"type":"integer","minimum":0},"limit":{"type":"integer","minimum":1,"maximum":50}},"additionalProperties":false})),
        ("agent_workflow_describe", "Describe a supported agent workflow", "Read-only full workflow with live MCP operation resolution, annotations-derived safety and retry/read-back boundaries.",
         json!({"$schema":schema,"type":"object","properties":{"id":{"type":"string","minLength":1,"maxLength":100}},"required":["id"],"additionalProperties":false})),
    ].into_iter().map(|(name,title,description,input)| json!({
        "name":name,"title":title,"description":description,"inputSchema":input,
        "outputSchema":serde_json::from_str::<Value>(crate::diagnostics_original118::OUTPUT_SCHEMA).expect("original envelope"),
        "annotations":{"readOnlyHint":true,"destructiveHint":false,"idempotentHint":true,"openWorldHint":false}
    })).collect()
}

const META_DESCRIPTION: &str = "Returnerer server-navn, server-version, antallet af registrerede MCP-tools i denne kørende proces, den aktuelle rules-bundle-version, og repo-relative stier til kontrakt-dokumenterne. Bruges af en agent der lige har connectet til serveren og vil verificere identitet/version før den begynder at kalde andre tools. Read-only, idempotent, ingen `company`-parameter.\n\nenvelope.data: { serverName, serverVersion, toolCount, ruleBundleVersion, contractDocs: string[] }. Brug `contractDocs`-stierne til at fetche de fulde kontrakt-dokumenter via det filesystem-aware adapter agenten kører i.";

fn invalid(name: &str) -> Value {
    json!({"isError":true,"content":[{"type":"text","text":format!("MCP error -32602: Input validation error: Invalid arguments for tool {name}")}]})
}

pub(crate) fn valid(name: &str, args: &Value) -> bool {
    let Some(object) = args.as_object() else { return false; };
    match name {
        "meta_about" => object.is_empty(),
        "agent_capability_search" => object.iter().all(|(key,value)| match key.as_str() {
            "query" => value.as_str().is_some_and(|s| s.encode_utf16().count() <= 200),
            "cursor" => value.as_u64().is_some(),
            "limit" => value.as_u64().is_some_and(|n| (1..=50).contains(&n)),
            _ => false,
        }),
        "agent_workflow_describe" => object.len() == 1 && args["id"].as_str().is_some_and(|id| !id.is_empty() && id.encode_utf16().count() <= 100),
        _ => false,
    }
}

pub(crate) fn success(mut core: Value, name: &str, tools: &[Value]) -> Value {
    core = original123::decorate_mcp(core, tools);
    if name == "meta_about" { core["toolCount"] = json!(tools.len()); }
    crate::diagnostics_original118::result(json!({"ok":true,"errors":[],"data":core}))
}

pub async fn call(api: &ApiClient, name: &str, args: Value) -> Result<Value, &'static str> {
    if !valid(name, &args) { return Ok(invalid(name)); }
    // Original describeWorkflow performs a local exact lookup before any transport.
    // Unknown IDs carry no authority and never need URL or filesystem interpretation.
    if name=="agent_workflow_describe" && original123::describe(args["id"].as_str().unwrap(),&[],false).is_none(){
        return Ok(crate::diagnostics_original118::result(json!({"ok":false,"errors":["UNKNOWN_WORKFLOW"]})));
    }
    let core = match name {
        "meta_about" => api.original_agent_about123().await,
        "agent_capability_search" => api.original_agent_capabilities123(
            args.get("query").and_then(Value::as_str).unwrap_or(""),
            args.get("cursor").and_then(Value::as_u64).unwrap_or(0),
            args.get("limit").and_then(Value::as_u64).unwrap_or(10) as usize,
        ).await,
        "agent_workflow_describe" => api.original_agent_workflow123(args["id"].as_str().unwrap()).await,
        _ => return Err("unknown tool"),
    };
    Ok(match core {
        Ok(core) if core.is_object() => {
            let registry = crate::tools::list();
            success(core, name, registry["tools"].as_array().expect("actual tools"))
        }
        Ok(_) => crate::diagnostics_original118::failure(Error::Protocol),
        Err(Error::Refused { status:404 }) if name == "agent_workflow_describe" =>
            crate::diagnostics_original118::result(json!({"ok":false,"errors":["UNKNOWN_WORKFLOW"]})),
        Err(error) => crate::diagnostics_original118::failure(error),
    })
}
