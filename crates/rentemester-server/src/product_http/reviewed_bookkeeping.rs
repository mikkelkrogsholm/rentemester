use super::*;
use rentemester_application::reviewed_bookkeeping::Action;
use rentemester_storage::{accounting_drafts, bookkeeping_queue, posting_rules};
enum Input {
    Draft(accounting_drafts::Input),
    Rule(Box<posting_rules::Input>),
    Bank(bookkeeping_queue::Create),
    Read,
}
pub(super) async fn handle(State(state): State<Product>, request: Request) -> Response {
    // A GET-backed HEAD must never select a draft creation or rule mutation.
    if request.method() == axum::http::Method::HEAD {
        let mut response = refused(StatusCode::METHOD_NOT_ALLOWED, "method_not_allowed");
        response.headers_mut().insert("allow", axum::http::HeaderValue::from_static("GET"));
        return response;
    }
    let company_slug=request.uri().path().split('/').nth(3).unwrap_or("").to_owned();
    let mutation = request.method() != axum::http::Method::GET;
    let cookie = match proof(&state, &request, mutation) {
        Ok(c) => c,
        Err(r) => return *r,
    };
    if !selected(&state, &request) {
        return refused(StatusCode::NOT_FOUND, "company_not_found");
    }
    if request.uri().query().is_some() {
        return refused(StatusCode::BAD_REQUEST, "invalid_query");
    }
    let parts = request
        .uri()
        .path()
        .split('/')
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let kind = parts.get(4).cloned().unwrap_or_default();
    let id = parts.get(5).cloned().unwrap_or_default();
    let action = parts.get(6).cloned().unwrap_or_default();
    let input = if mutation {
        if request.headers().contains_key("content-encoding")
            || !one(request.headers(), "content-type")
                .is_some_and(|s| s.eq_ignore_ascii_case("application/json"))
        {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        }
        let Ok(bytes) = to_bytes(request.into_body(), 270 * 1024).await else {
            return refused(StatusCode::PAYLOAD_TOO_LARGE, "invalid_input");
        };
        let parsed = match kind.as_str() {
            "accounting-drafts" => serde_json::from_slice(&bytes).map(Input::Draft),
            "posting-rules" => serde_json::from_slice(&bytes).map(|v| Input::Rule(Box::new(v))),
            "reviewed-bank" => serde_json::from_slice(&bytes).map(Input::Bank),
            _ => return refused(StatusCode::NOT_FOUND, "not_found"),
        };
        match parsed {
            Ok(v) => v,
            Err(_) => return refused(StatusCode::BAD_REQUEST, "invalid_input"),
        }
    } else {
        Input::Read
    };
    let key = match (&input, kind.as_str()) {
        (Input::Read, "accounting-drafts") => "accountingDrafts",
        (Input::Read, "posting-rules") => "postingRules",
        (Input::Read, _) => "queue",
        (Input::Rule(_), _) => "postingRule",
        _ => "accountingDraft",
    };
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let mut owner = state.native.lock_company(&company_slug).map_err(|_| ())?;
        let operation = match &input {
            Input::Read => match kind.as_str() {
                "accounting-drafts" => Action::Drafts,
                "posting-rules" => Action::Rules,
                _ => Action::Queue,
            },
            Input::Draft(i) => Action::Draft {
                id: if id.is_empty() {
                    i.draft_id.as_deref().unwrap_or("")
                } else {
                    &id
                },
                action: if id.is_empty() { "create" } else { &action },
                input: i,
            },
            Input::Rule(i) => Action::Rule {
                action: &id,
                input: i,
            },
            Input::Bank(i) => Action::FromBank(i),
        };
        owner
            .reviewed_bookkeeping(&cookie, &SystemClock, operation)
            .map_err(|_| ())
    })
    .await;
    match result {
        Ok(Ok(attempt)) => {
            if matches!(&attempt.outcome, Err(f) if f.stage == FailureStage::OutcomeUnknown) {
                return response(
                    StatusCode::SERVICE_UNAVAILABLE,
                    json!({"ok":false,"code":"internal","errors":["Resultatet af handlingen kan ikke bekræftes. Kontrollér kladden før en ny handling."],"outcome":"unknown"}),
                );
            }
            match super::documents::outcome(attempt) {
                Ok(value) => response(StatusCode::OK, json!({"ok":true,key:value})),
                Err(r) => *r,
            }
        }
        _ => response(
            StatusCode::SERVICE_UNAVAILABLE,
            json!({"ok":false,"code":"internal","errors":["Resultatet kan ikke bekræftes. Kontrollér kladden før en ny handling."],"outcome":"unknown"}),
        ),
    }
}
