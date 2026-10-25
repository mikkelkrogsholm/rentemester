//! Inert ORIGINAL metadata handlers, isolated from native discovery semantics.
use super::{Request, Response, State, StatusCode, Value, json, response, to_bytes};

fn decode(input: &str, plus: bool) -> Option<String> {
    let mut bytes = Vec::new();
    let mut it = input.bytes();
    while let Some(b) = it.next() {
        if b == b'%' {
            let hi = (it.next()? as char).to_digit(16)?;
            let lo = (it.next()? as char).to_digit(16)?;
            bytes.push((hi * 16 + lo) as u8);
        } else {
            bytes.push(if plus && b == b'+' { b' ' } else { b });
        }
    }
    String::from_utf8(bytes).ok()
}
fn fields(query: &str) -> Option<std::collections::BTreeMap<String, String>> {
    let mut fields = std::collections::BTreeMap::new();
    if query.is_empty() {
        return Some(fields);
    }
    for pair in query.split('&') {
        let (key, value) = pair.split_once('=')?;
        let key = decode(key, true)?;
        let value = decode(value, true)?;
        if !["query", "cursor", "limit"].contains(&key.as_str())
            || fields.insert(key, value).is_some()
        {
            return None;
        }
    }
    Some(fields)
}
fn unsigned<T: std::str::FromStr>(s: &str) -> Option<T> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s.parse().ok()
}
fn routes() -> Vec<Value> {
    let manifest: Value = serde_json::from_str(include_str!("../../../../protocol/http-v1.json"))
        .expect("checked actual protocol manifest");
    manifest["operations"].as_array().unwrap().clone()
}
pub(super) async fn handle(State(connected): State<bool>, request: Request) -> Response {
    let invalid = || {
        response(
            StatusCode::BAD_REQUEST,
            json!({"ok":false,"code":"invalid_input"}),
        )
    };
    let path = request.uri().path().to_owned();
    let query = request.uri().query().unwrap_or("").to_owned();
    if query.len() > 4096
        || request.headers().contains_key("content-encoding")
        || to_bytes(request.into_body(), 0).await.is_err()
    {
        return invalid();
    }
    let routes = routes();
    let mut value = if path == "/api/original/agent-about" {
        if !query.is_empty() {
            return invalid();
        }
        rentemester_engine::agent_discovery::original123::about()
    } else if path == "/api/original/agent-capabilities" {
        let Some(fields) = fields(&query) else {
            return invalid();
        };
        let cursor = match fields.get("cursor") {
            Some(s) => match unsigned::<u64>(s) {
                Some(n) => n,
                None => return invalid(),
            },
            None => 0,
        };
        let limit = match fields.get("limit") {
            Some(s) => match unsigned::<usize>(s) {
                Some(n) => n,
                None => return invalid(),
            },
            None => 10,
        };
        match rentemester_engine::agent_discovery::original123::search(
            fields.get("query").map_or("", String::as_str),
            cursor,
            limit,
            &routes,
            connected,
        ) {
            Ok(v) => v,
            Err(_) => return invalid(),
        }
    } else {
        if !query.is_empty() {
            return invalid();
        }
        let Some(id) = path
            .strip_prefix("/api/original/agent-workflows/")
            .and_then(|s| decode(s, false))
        else {
            return invalid();
        };
        if id.is_empty() || id.encode_utf16().count() > 100 {
            return invalid();
        }
        match rentemester_engine::agent_discovery::original123::describe(&id, &routes, connected) {
            Some(v) => v,
            None => {
                return response(
                    StatusCode::NOT_FOUND,
                    json!({"ok":false,"code":"UNKNOWN_WORKFLOW","errors":["UNKNOWN_WORKFLOW"]}),
                );
            }
        }
    };
    value["workspaceConnected"] = json!(connected);
    value["runtimeResolutionNote"] = json!(
        "Actual native protocol route registration and current original CLI keys only; not authorization, provider configuration or parity. Resolve MCP references against actual tools/list."
    );
    response(StatusCode::OK, value)
}
#[cfg(test)]
#[path = "original123_tests.rs"]
mod tests;
