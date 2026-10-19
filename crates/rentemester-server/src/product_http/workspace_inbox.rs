//! Manual workspace inbox adapters; anchor is access scope, never implicit destination.
use super::*;
use base64::{Engine, engine::general_purpose::STANDARD};
use rentemester_application::workspace_inbox::{self as app, Action};
use rentemester_runtime::document::Prepared;
enum Command {
    List(usize, usize),
    Inspect(String),
    Ingest(app::Ingest),
    Assign(String, app::Destination),
    Complete(String, app::Destination),
}
pub(super) async fn handle(
    State(state): State<Product>,
    axum::extract::Path(parameters): axum::extract::Path<std::collections::HashMap<String, String>>,
    request: Request,
) -> Response {
    let path = request.uri().path().to_owned();
    let parts = path.split('/').collect::<Vec<_>>();
    let anchor = parameters.get("slug").cloned().unwrap_or_default();
    let source = parameters.get("source").cloned().unwrap_or_default();
    let mutation = request.method() != axum::http::Method::GET;
    let cookie = match proof(&state, &request, mutation) {
        Ok(p) => p,
        Err(r) => return *r,
    };
    if !selected(&state, &request) {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    }
    let command = if mutation {
        if request.uri().query().is_some()
            || request.headers().contains_key("content-encoding")
            || !one(request.headers(), "content-type")
                .is_some_and(|s| s.eq_ignore_ascii_case("application/json"))
        {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        }
        let Ok(bytes) = to_bytes(request.into_body(), 35 * 1024 * 1024).await else {
            return refused(StatusCode::PAYLOAD_TOO_LARGE, "upload_too_large");
        };
        let Ok(mut body) = serde_json::from_slice::<serde_json::Map<String, Value>>(&bytes) else {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        };
        if body.remove("confirm") != Some(json!(true)) {
            return refused(StatusCode::BAD_REQUEST, "confirmation_required");
        }
        let body = Value::Object(body);
        match (parts.len(), parts.last().copied()) {
            (5, Some("workspace-inbox")) => match serde_json::from_value(body) {
                Ok(i) => Command::Ingest(i),
                Err(_) => return refused(StatusCode::BAD_REQUEST, "invalid_input"),
            },
            (7, Some(action @ ("assign" | "complete"))) => {
                let Ok(d) = serde_json::from_value(body) else {
                    return refused(StatusCode::BAD_REQUEST, "invalid_input");
                };
                if action == "assign" {
                    Command::Assign(source.clone(), d)
                } else {
                    Command::Complete(source.clone(), d)
                }
            }
            _ => return refused(StatusCode::NOT_FOUND, "not_found"),
        }
    } else {
        if parts.len() == 6 {
            if request.uri().query().is_some() {
                return refused(StatusCode::BAD_REQUEST, "invalid_input");
            }
            Command::Inspect(source)
        } else {
            let (mut cursor, mut limit) = (None, None);
            if let Some(query) = request.uri().query() {
                for (k, v) in url::form_urlencoded::parse(query.as_bytes()) {
                    let value = match v.parse::<usize>() {
                        Ok(n) if n.to_string() == v => n,
                        _ => return refused(StatusCode::BAD_REQUEST, "invalid_input"),
                    };
                    match k.as_ref() {
                        "cursor" if cursor.is_none() => cursor = Some(value),
                        "limit" if limit.is_none() && (1..=100).contains(&value) => {
                            limit = Some(value)
                        }
                        _ => return refused(StatusCode::BAD_REQUEST, "invalid_input"),
                    }
                }
            }
            Command::List(cursor.unwrap_or(0), limit.unwrap_or(25))
        }
    };
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let result = tokio::task::spawn_blocking(move || -> Result<Response, Box<Response>> {
        let _permit = permit;
        let prepared = if let Command::Ingest(i) = &command {
            let bytes = STANDARD.decode(&i.bytes_base64)
                .map_err(|_| Box::new(refused(StatusCode::BAD_REQUEST,"invalid_base64")))?;
            Some(Prepared::new(i.filename.clone(), bytes).map_err(|message|
                Box::new(response(StatusCode::UNPROCESSABLE_ENTITY,json!({"ok":false,"code":"invalid_document_file","errors":[message]}))))?)
        } else { None };
        let selected = if let Command::Complete(_, d) = &command { d.company_slug.as_str() } else { anchor.as_str() };
        let mut owner = state.native.lock_company(selected)
            .map_err(|_| Box::new(refused(StatusCode::SERVICE_UNAVAILABLE,"owner_unavailable")))?;
        let action = match &command {
            Command::List(c,l) => Action::List { cursor:*c, limit:*l },
            Command::Inspect(id) => Action::Inspect(id),
            Command::Ingest(i) => Action::Ingest(i,prepared.as_ref().unwrap()),
            Command::Assign(id,d) => Action::Assign(id,d),
            Command::Complete(id,d) => Action::Complete(id,d),
        };
        let attempt = owner.workspace_inbox(&state.native.cookies,&cookie,&SystemClock,&anchor,action)
            .map_err(|_| Box::new(refused(StatusCode::SERVICE_UNAVAILABLE,"internal")))?;
        match attempt.outcome {
            Ok(mut v) => { v.value["ok"] = json!(true); Ok(response(if matches!(command,Command::Ingest(_)) {StatusCode::CREATED} else {StatusCode::OK},v.value)) },
            Err(f) => {
                let unknown = f.stage == FailureStage::OutcomeUnknown;
                let status = if unknown { StatusCode::SERVICE_UNAVAILABLE }
                    else if matches!(f.error.code,"access_denied"|"invalid_credentials") {StatusCode::FORBIDDEN}
                    else if f.error.code=="workspace_inbox_not_found" {StatusCode::NOT_FOUND}
                    else {StatusCode::UNPROCESSABLE_ENTITY};
                Err(Box::new(response(status,json!({"ok":false,"code":if unknown {"internal"} else {f.error.code},"errors":[f.error.message]}))))
            }
        }
    }).await;
    match result {
        Ok(Ok(r)) => r,
        Ok(Err(r)) => *r,
        _ => refused(StatusCode::SERVICE_UNAVAILABLE, "internal"),
    }
}
