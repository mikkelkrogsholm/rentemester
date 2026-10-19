//! Exact native batch HTTP scopes and current authorized read-first recovery.
use super::*;
use rentemester_application::bookkeeping_batch::Action;
use rentemester_engine::bookkeeping_batch::{Exact, Persist, Scope, WorkbenchInput};
enum Input {
    Plan(Scope),
    Persist(Persist),
    Approve(Exact),
    Apply(Exact),
    Status(i64),
    StatusByKey(String),
    Workbench(WorkbenchInput),
}
fn query(request: &Request) -> Result<serde_json::Map<String, Value>, ()> {
    let mut map = serde_json::Map::new();
    for (k, v) in url::form_urlencoded::parse(request.uri().query().unwrap_or("").as_bytes()) {
        if map.insert(k.into_owned(), json!(v)).is_some() {
            return Err(());
        }
    }
    Ok(map)
}
fn query_input<T: serde::de::DeserializeOwned>(
    mut map: serde_json::Map<String, Value>,
    numbers: &[&str],
) -> Result<T, ()> {
    for key in numbers {
        if let Some(v) = map.get_mut(*key) {
            let s = v.as_str().ok_or(())?;
            if s.is_empty()
                || !s.bytes().all(|b| b.is_ascii_digit())
                || (s.len() > 1 && s.starts_with('0'))
            {
                return Err(());
            }
            *v = json!(s.parse::<u64>().map_err(|_| ())?);
        }
    }
    serde_json::from_value(Value::Object(map)).map_err(|_| ())
}
pub(super) async fn handle(State(state): State<Product>, request: Request) -> Response {
    let slug = request
        .uri()
        .path()
        .split('/')
        .nth(3)
        .unwrap_or("")
        .to_owned();
    let path = request
        .uri()
        .path()
        .split('/')
        .skip(4)
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let mutation = request.method() != axum::http::Method::GET;
    let cookie = match proof(&state, &request, mutation) {
        Ok(c) => c,
        Err(r) => return *r,
    };
    if !selected(&state, &request) {
        return refused(StatusCode::NOT_FOUND, "company_not_found");
    }
    let input = if !mutation {
        let Ok(q) = query(&request) else {
            return refused(StatusCode::BAD_REQUEST, "invalid_query");
        };
        match path
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .as_slice()
        {
            ["bookkeeping-batch"] => query_input(q, &["companyId"]).map(Input::Plan),
            ["bookkeeping-workbench"] => {
                query_input(q, &["bankAccountId", "limit", "cursor"]).map(Input::Workbench)
            }
            ["bookkeeping-batch", "runs"] => {
                if q.len() != 1 {
                    Err(())
                } else {
                    q.get("runKey")
                        .and_then(Value::as_str)
                        .filter(|s| {
                            !s.trim().is_empty()
                                && s.len() <= 1024
                                && !s.chars().any(char::is_control)
                        })
                        .map(|s| Input::StatusByKey(s.into()))
                        .ok_or(())
                }
            }
            ["bookkeeping-batch", "runs", id] if q.is_empty() => id
                .parse::<i64>()
                .ok()
                .filter(|n| *n > 0 && n.to_string() == *id && *n <= 9_007_199_254_740_991)
                .map(Input::Status)
                .ok_or(()),
            _ => Err(()),
        }
    } else {
        if request.method() != axum::http::Method::POST
            || request.uri().query().is_some()
            || request.headers().contains_key("content-encoding")
            || !one(request.headers(), "content-type")
                .is_some_and(|s| s.eq_ignore_ascii_case("application/json"))
        {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        }
        let Ok(bytes) = to_bytes(request.into_body(), 32 * 1024).await else {
            return refused(StatusCode::PAYLOAD_TOO_LARGE, "invalid_input");
        };
        match path
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .as_slice()
        {
            ["bookkeeping-batch", "persist"] => serde_json::from_slice(&bytes)
                .map(Input::Persist)
                .map_err(|_| ()),
            ["bookkeeping-batch", "approve"] => serde_json::from_slice(&bytes)
                .map(Input::Approve)
                .map_err(|_| ()),
            ["bookkeeping-batch", "apply"] => serde_json::from_slice(&bytes)
                .map(Input::Apply)
                .map_err(|_| ()),
            _ => Err(()),
        }
    };
    let Ok(input) = input else {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    };
    let valid = match &input {
        Input::Plan(i) => i.validate(),
        Input::Persist(i) => i.validate(),
        Input::Approve(i) | Input::Apply(i) => i.validate(),
        Input::Workbench(i) => i.validate(),
        _ => Ok(()),
    };
    if valid.is_err() {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    }
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let mut owner = state.native.lock_company(&slug).map_err(|_| ())?;
        let action = match &input {
            Input::Plan(i) => Action::Plan(i),
            Input::Persist(i) => Action::Persist(i),
            Input::Approve(i) => Action::Approve(i),
            Input::Apply(i) => Action::Apply(i),
            Input::Status(i) => Action::Status(*i),
            Input::StatusByKey(s) => Action::StatusByKey(s),
            Input::Workbench(i) => Action::Workbench(i),
        };
        owner
            .bookkeeping_batch(&cookie, &SystemClock, action)
            .map_err(|_| ())
    })
    .await;
    match result {
        Ok(Ok(attempt)) => {
            if let Err(f) = &attempt.outcome {
                if f.stage == FailureStage::NotCommitted
                    && f.error.code == "bookkeeping_batch_not_found"
                {
                    return refused(StatusCode::NOT_FOUND, "bookkeeping_batch_not_found");
                }
                if f.stage == FailureStage::NotCommitted && f.error.code == "access_denied" {
                    return refused(StatusCode::FORBIDDEN, "access_denied");
                }
            }
            match super::documents::outcome(attempt) {
                Ok(value) => response(StatusCode::OK, value),
                Err(r) => *r,
            }
        }
        _ => response(
            StatusCode::SERVICE_UNAVAILABLE,
            json!({"ok":false,"code":"internal","outcome":"unknown","errors":["Read canonical batch status before another action"]}),
        ),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn strict_query_and_canonical_scope_contract() {
        let req = Request::builder()
            .uri("/api/companies/a/bookkeeping-batch?bankFrom=2026-01-01&bankFrom=2026-01-02")
            .body(axum::body::Body::empty())
            .unwrap();
        assert!(query(&req).is_err());
        let req=Request::builder().uri("/api/companies/a/bookkeeping-batch?accountingFrom=2026-01-01&accountingTo=2026-12-31&bankFrom=2026-01-01&bankTo=2026-12-31&companyId=01").body(axum::body::Body::empty()).unwrap();
        assert!(query_input::<Scope>(query(&req).unwrap(), &["companyId"]).is_err());
        let q=serde_json::from_value(json!({"accountingFrom":"2026-01-01","accountingTo":"2026-12-31","bankFrom":"2026-01-01","bankTo":"2026-12-31","actor":"user:forged"})).unwrap();
        assert!(query_input::<Scope>(q, &["companyId"]).is_err());
        assert!(
            serde_json::from_value::<Exact>(
                json!({"runId":1,"planHash":"a".repeat(64),"confirm":false})
            )
            .unwrap()
            .validate()
            .is_err()
        );
    }
}
