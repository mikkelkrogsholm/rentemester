//! Manual tasks use the same verified control owner for browser, CLI and MCP.
use super::*;
use rentemester_application::tasks::{Action, Authority};
enum Credentials {
    Cookie(VerifiedCookie),
    Service(rentemester_runtime::service_key::ServiceKey),
}
enum Command {
    List(Value),
    Get(String),
    History(String),
    Receipt(String),
    Create(Value),
    Update(String, Value),
    Move(String, Value),
    Complete(String, Value),
    Reopen(String, Value),
}
fn query(raw: &str) -> Result<Value, ()> {
    let mut out = serde_json::Map::new();
    let mut companies = Vec::<String>::new();
    let mut selection = false;
    for (k, v) in url::form_urlencoded::parse(raw.as_bytes()) {
        match k.as_ref() {
            "companySlugs" if v.is_empty() => {
                selection = true;
            }
            "companySlug" | "companySlugs" => {
                selection = true;
                if !companies.iter().any(|s| s == v.as_ref()) {
                    companies.push(v.into_owned());
                }
            }
            "includeArchived" | "undated" | "unassigned" | "showDone"
                if !out.contains_key(k.as_ref()) =>
            {
                let b = match v.as_ref() {
                    "true" => true,
                    "false" => false,
                    _ => return Err(()),
                };
                out.insert(k.into_owned(), json!(b));
            }
            "status" | "type" | "search" | "assigneeId" | "from" | "to"
                if !out.contains_key(k.as_ref()) =>
            {
                out.insert(k.into_owned(), json!(v));
            }
            _ => return Err(()),
        }
    }
    if selection {
        out.insert("companySlugs".into(), json!(companies));
    }
    Ok(Value::Object(out))
}
fn decode(raw: &str) -> String {
    url::form_urlencoded::parse(format!("id={}", raw.replace('+', "%2B")).as_bytes())
        .next()
        .unwrap()
        .1
        .into_owned()
}
fn command(
    path: &str,
    body: Option<Value>,
    raw_query: Option<&str>,
) -> Result<(Command, Option<String>, Option<u64>), &'static str> {
    let parts: Vec<_> = path.split('/').collect();
    if let Some(mut body) = body {
        if raw_query.is_some() {
            return Err("invalid_input");
        }
        let object = body.as_object_mut().ok_or("invalid_input")?;
        if object.remove("confirm") != Some(json!(true)) {
            return Err("confirmation_required");
        }
        let key = object
            .remove("idempotencyKey")
            .and_then(|v| v.as_str().map(str::to_owned))
            .ok_or("idempotency_key_required")?;
        rentemester_engine::tasks::id(&json!(key)).map_err(|_| "invalid_input")?;
        let version = object.remove("expectedVersion");
        let expected = version
            .as_ref()
            .map(|v| {
                v.as_u64()
                    .filter(|n| (1..=9_007_199_254_740_991).contains(n))
                    .ok_or("version_required")
            })
            .transpose()?;
        let operation = if parts.len() == 3 && parts[2] == "tasks" {
            if expected.is_some() {
                return Err("invalid_input");
            }
            Command::Create(body)
        } else {
            if expected.is_none() {
                return Err("version_required");
            }
            let id = decode(parts.get(3).ok_or("not_found")?);
            if parts.len() != 5 {
                return Err("not_found");
            }
            match parts[4] {
                "update" => {
                    let patch = object.remove("patch").ok_or("invalid_input")?;
                    if !object.is_empty() {
                        return Err("invalid_input");
                    }
                    Command::Update(id, patch)
                }
                "move" => Command::Move(id, body),
                "complete" => Command::Complete(id, body),
                "reopen" => {
                    let reason = object.remove("reason").ok_or("invalid_input")?;
                    if !object.is_empty() {
                        return Err("invalid_input");
                    }
                    Command::Reopen(id, reason)
                }
                _ => return Err("not_found"),
            }
        };
        Ok((operation, Some(key), expected))
    } else {
        let cmd = match parts.as_slice() {
            ["", "api", "tasks"] => {
                Command::List(query(raw_query.unwrap_or("")).map_err(|_| "invalid_input")?)
            }
            ["", "api", "tasks", id] if raw_query.is_none() => Command::Get(decode(id)),
            ["", "api", "tasks", "operations", key] if raw_query.is_none() => {
                Command::Receipt(decode(key))
            }
            ["", "api", "tasks", id, "history"] if raw_query.is_none() => {
                Command::History(decode(id))
            }
            _ => return Err("invalid_input"),
        };
        Ok((cmd, None, None))
    }
}
pub(super) async fn handle(State(state): State<Product>, request: Request) -> Response {
    if request.method() == axum::http::Method::HEAD {
        let mut r = refused(StatusCode::METHOD_NOT_ALLOWED, "method_not_allowed");
        r.headers_mut()
            .insert("allow", axum::http::HeaderValue::from_static("GET"));
        return r;
    }
    let mutation = request.method() == axum::http::Method::POST;
    let credentials = if service_company::requested(&request) {
        match service_company::proof(&state, &request, mutation) {
            Ok(p) => Credentials::Service(p),
            Err(r) => return *r,
        }
    } else {
        match proof(&state, &request, mutation) {
            Ok(p) => Credentials::Cookie(p),
            Err(r) => return *r,
        }
    };
    let path = request.uri().path().to_owned();
    let raw_query = request.uri().query().map(str::to_owned);
    if request.headers().contains_key("content-encoding")
        || request.headers().contains_key("idempotency-key")
        || !mutation
            && (request.headers().contains_key("transfer-encoding")
                || one(request.headers(), "content-length").is_some_and(|s| s != "0"))
    {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    }
    let body = if mutation {
        if !one(request.headers(), "content-type").is_some_and(|s| {
            s.eq_ignore_ascii_case("application/json")
                || s.eq_ignore_ascii_case("application/json; charset=utf-8")
        }) {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        }
        let bytes = match to_bytes(request.into_body(), 128 * 1024).await {
            Ok(b) => b,
            Err(_) => return refused(StatusCode::PAYLOAD_TOO_LARGE, "body_too_large"),
        };
        match serde_json::from_slice::<Value>(&bytes) {
            Ok(v) => Some(v),
            Err(_) => return refused(StatusCode::BAD_REQUEST, "invalid_input"),
        }
    } else {
        if !matches!(to_bytes(request.into_body(),0).await,Ok(b) if b.is_empty()) {
            return refused(StatusCode::BAD_REQUEST, "body_not_allowed");
        }
        None
    };
    let (command, key, expected) = match command(&path, body, raw_query.as_deref()) {
        Ok(c) => c,
        Err(e) => return refused(StatusCode::BAD_REQUEST, e),
    };
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let mut owner = state.native.owner.lock().map_err(|_| ())?;
        let authority = match &credentials {
            Credentials::Cookie(p) => Authority::Cookie {
                cookies: &state.native.cookies,
                proof: p,
            },
            Credentials::Service(k) => Authority::Service(k),
        };
        let action = match &command {
            Command::List(q) => Action::List(q),
            Command::Get(id) => Action::Get(id),
            Command::History(id) => Action::History(id),
            Command::Receipt(k) => Action::Receipt(k),
            Command::Create(v) => Action::Create(v),
            Command::Update(id, v) => Action::Update { id, patch: v },
            Command::Move(id, v) => Action::Move { id, input: v },
            Command::Complete(id, v) => Action::Complete { id, input: v },
            Command::Reopen(id, v) => Action::Reopen { id, reason: v },
        };
        owner
            .tasks(&SystemClock, authority, action, key.as_deref(), expected)
            .map_err(|_| ())
    })
    .await;
    match result {
        Ok(Ok(a)) => match a.outcome {
            Ok(mut v) => {
                v["ok"] = json!(true);
                response(StatusCode::OK, v)
            }
            Err(f) => {
                let unknown = f.stage
                    == rentemester_application::password_login::FailureStage::OutcomeUnknown
                    || !f.connection_reusable;
                let status = if unknown {
                    StatusCode::SERVICE_UNAVAILABLE
                } else {
                    match f.error.code {
                        "invalid_credentials" => StatusCode::UNAUTHORIZED,
                        "access_denied" => StatusCode::FORBIDDEN,
                        "not_found" => StatusCode::NOT_FOUND,
                        "version_conflict" | "idempotency_conflict" | "task_source_stale" => {
                            StatusCode::CONFLICT
                        }
                        _ => StatusCode::UNPROCESSABLE_ENTITY,
                    }
                };
                refused(status, if unknown { "internal" } else { f.error.code })
            }
        },
        _ => refused(StatusCode::SERVICE_UNAVAILABLE, "internal"),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_public_envelopes_and_repeated_company_query() {
        assert_eq!(
            query("companySlug=a&companySlugs=b&companySlug=a&showDone=false").unwrap(),
            json!({"companySlugs":["a","b"],"showDone":false})
        );
        assert!(
            matches!(command("/api/tasks/operations/history",None,None).unwrap().0,Command::Receipt(key) if key=="history")
        );
        assert_eq!(query("companySlugs=").unwrap(), json!({"companySlugs":[]}));
        assert!(query("showDone=1").is_err());
        assert!(query("status=open&status=done").is_err());
        assert!(
            command(
                "/api/tasks",
                Some(json!({"title":"Task","confirm":false,"idempotencyKey":"k"})),
                None
            )
            .is_err()
        );
        assert!(command("/api/tasks/one/update",Some(json!({"confirm":true,"idempotencyKey":"k","expectedVersion":1,"patch":{},"actor":"user:owner"})),None).is_err());
        assert!(
            command(
                "/api/tasks/one/move",
                Some(json!({"confirm":true,"idempotencyKey":"k","status":"done"})),
                None
            )
            .is_err()
        );
        assert!(matches!(command("/api/tasks/one/reopen",Some(json!({"confirm":true,"idempotencyKey":"k","expectedVersion":1,"reason":"Reviewed"})),None).unwrap().0,Command::Reopen(..)));
    }
}
