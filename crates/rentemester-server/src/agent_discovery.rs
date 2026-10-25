//! Public, inert metadata. Accounting steps still require normal company authority.
use axum::{
    Router,
    body::to_bytes,
    extract::{Request, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};
use serde_json::{Value, json};

pub fn router(connected: bool) -> Router {
    Router::new()
        .route("/api/agent-about", get(handle))
        .route("/api/agent-capabilities", get(handle))
        .route("/api/agent-workflows/{id}", get(handle))
        .with_state(connected)
}
fn response(status: StatusCode, body: Value) -> Response {
    (
        status,
        [
            ("content-type", "application/json"),
            ("cache-control", "no-store"),
        ],
        body.to_string(),
    )
        .into_response()
}
async fn handle(State(connected): State<bool>, request: Request) -> Response {
    let invalid = || {
        response(
            StatusCode::BAD_REQUEST,
            json!({"ok":false,"code":"invalid_input"}),
        )
    };
    let path = request.uri().path().to_owned();
    let query = request.uri().query().unwrap_or("").to_owned();
    if query.len() > 2048
        || request.headers().contains_key("content-encoding")
        || to_bytes(request.into_body(), 0).await.is_err()
    {
        return invalid();
    }
    let result = if path == "/api/agent-about" {
        if !query.is_empty() {
            return invalid();
        }
        rentemester_engine::agent_discovery::about()
    } else if path == "/api/agent-capabilities" {
        let mut fields = std::collections::BTreeMap::new();
        if query.bytes().enumerate().any(|(i, b)| {
            b == b'%'
                && (i + 2 >= query.len()
                    || !query.as_bytes()[i + 1].is_ascii_hexdigit()
                    || !query.as_bytes()[i + 2].is_ascii_hexdigit())
        }) {
            return invalid();
        }
        for (key, value) in url::form_urlencoded::parse(query.as_bytes()) {
            if !["query", "cursor", "limit"].contains(&key.as_ref())
                || fields
                    .insert(key.into_owned(), value.into_owned())
                    .is_some()
            {
                return invalid();
            }
        }
        let cursor = match fields.get("cursor").map(|v| v.parse::<usize>()).transpose() {
            Ok(v) => v.unwrap_or(0),
            Err(_) => return invalid(),
        };
        let limit = match fields.get("limit").map(|v| v.parse::<usize>()).transpose() {
            Ok(v) => v.unwrap_or(10),
            Err(_) => return invalid(),
        };
        match rentemester_engine::agent_discovery::search(
            fields.get("query").map_or("", String::as_str),
            cursor,
            limit,
        ) {
            Ok(v) => v,
            Err(_) => return invalid(),
        }
    } else {
        if !query.is_empty() {
            return invalid();
        }
        let id = path.strip_prefix("/api/agent-workflows/").unwrap_or("");
        match rentemester_engine::agent_discovery::describe(id) {
            Some(v) => v,
            None => {
                return response(
                    StatusCode::NOT_FOUND,
                    json!({"ok":false,"code":"workflow_not_found"}),
                );
            }
        }
    };
    response(StatusCode::OK, resolve(result, connected))
}
fn resolve(mut result: Value, connected: bool) -> Value {
    result["workspaceConnected"] = json!(connected);
    if result["serverName"] == "rentemester" {
        result["originalAgentRun"] = json!({"surface":"http","method":"POST","pattern":"/api/companies/:slug/agent/original/run","resolved":connected,"cli":"agent run","requiresConfirmation":true,"policy":"Trusted per-company agentPolicies; empty supplier rules by default","actor":"Current authenticated user or service account","phases":rentemester_engine::agent_run::PHASES,"retryClass":"unsafe-read-back","canonicalReadBack":["documents","bank transactions","journal entries","payables","exceptions"],"scope":"Existing original explicit bookkeeping run; no background job or automatic retry"});
    }
    // Only concrete registrations from product_http::router are attested.
    // Accounting requests still require their ordinary current authority.
    if let Some(steps) = result
        .pointer_mut("/workflow/steps")
        .and_then(Value::as_array_mut)
    {
        for step in steps {
            for key in ["operation", "uncertainOutcomeReadBack"] {
                let Some(operation) = step.get_mut(key) else {
                    continue;
                };
                let registered = matches!(
                    (operation["method"].as_str(), operation["pattern"].as_str()),
                    (Some("GET" | "POST"), Some("/api/companies/:slug/payables"))
                        | (Some("POST"), Some("/api/companies/:slug/payables/:id/pay"))
                        | (
                            Some("GET"),
                            Some(
                                "/api/companies/:slug/bank-accounts"
                                    | "/api/companies/:slug/bank"
                                    | "/api/companies/:slug/bank/match-suggestions"
                                    | "/api/companies/:slug/reviewed-bank"
                                    | "/api/companies/:slug/journal"
                                    | "/api/companies/:slug/bookkeeping-batch"
                                    | "/api/companies/:slug/bookkeeping-batch/runs"
                                    | "/api/companies/:slug/bookkeeping-batch/runs/:runId"
                            )
                        )
                        | (
                            Some("POST"),
                            Some(
                                "/api/companies/:slug/agent/original/run"
                                    | "/api/companies/:slug/bank/import"
                                    | "/api/companies/:slug/bookkeeping-batch/persist"
                                    | "/api/companies/:slug/bookkeeping-batch/approve"
                                    | "/api/companies/:slug/bookkeeping-batch/apply"
                            )
                        )
                );
                operation["resolved"] = json!(connected && registered);
                operation["reason"] = if connected && registered {
                    Value::Null
                } else {
                    json!("Native workspace or operation unavailable; do not execute.")
                };
            }
        }
        let unresolved: Vec<Value> = result["workflow"]["steps"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|step| {
                ["operation", "uncertainOutcomeReadBack"]
                    .into_iter()
                    .filter_map(move |key| step.get(key))
            })
            .filter(|operation| operation["resolved"] != true)
            .map(|operation| operation["id"].clone())
            .collect();
        result["workflow"]["live"] = json!(unresolved.is_empty());
        result["workflow"]["unresolvedOperations"] = json!(unresolved);
        result["workflow"]["resolutionReason"] = json!(
            "HTTP registration availability only; resolve aliases against tools/list or remote --help. Every accounting request still requires current company authority and confirmation."
        );
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bank_and_supplier_workflows_resolve_only_on_connected_registered_routes() {
        for id in ["supplier-payable-handling", "bank-reconciliation-batch"] {
            let description = rentemester_engine::agent_discovery::describe(id).unwrap();
            let connected = resolve(description.clone(), true);
            assert_eq!(connected["workflow"]["live"], true);
            assert_eq!(connected["workflow"]["unresolvedOperations"], json!([]));
            let disconnected = resolve(description, false);
            assert_eq!(disconnected["workflow"]["live"], false);
            assert!(
                !disconnected["workflow"]["unresolvedOperations"]
                    .as_array()
                    .unwrap()
                    .is_empty()
            );
            for step in disconnected["workflow"]["steps"].as_array().unwrap() {
                assert_eq!(step["operation"]["resolved"], false);
            }
        }
    }
    #[test]
    fn unknown_operation_or_recovery_route_never_advertises_a_live_workflow() {
        for key in ["operation", "uncertainOutcomeReadBack"] {
            let mut description =
                rentemester_engine::agent_discovery::describe("supplier-payable-handling").unwrap();
            description["workflow"]["steps"][0][key] = json!({"surface":"http","id":"not-registered","method":"POST","pattern":"/api/companies/:slug/not-registered"});
            let resolved = resolve(description, true);
            assert_eq!(resolved["workflow"]["live"], false);
            assert!(
                resolved["workflow"]["unresolvedOperations"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("not-registered"))
            );
        }
    }
}
