//! Flat original CLI transport; authority remains the native company owner.
use super::*;
use rentemester_application::periods::Action;
use rentemester_storage::periods::{
    Reopen,
    original::{CloseInput, Review, hash_valid},
};
enum Command {
    Readiness(String, String),
    Status(i64),
    List,
    ReviewCurrent(rentemester_storage::periods::Bounds),
    McpClose(CloseInput),
    Review(Review),
    Close(CloseInput),
    Reopen(Reopen),
}
fn date(value: &str) -> bool {
    rentemester_engine::fiscal_year::Date::parse(value).is_ok()
}
fn bounded(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 4096 && !value.chars().any(char::is_control)
}
fn id(value: i64) -> bool {
    (1..=9_007_199_254_740_991).contains(&value)
}
fn kind(value: Option<&str>) -> bool {
    value.is_none_or(|s| matches!(s, "vat_period" | "vat_quarter" | "fiscal_year" | "custom"))
}
pub(super) async fn handle(State(state): State<Product>, request: Request) -> Response {
    let mutation = request.method() != axum::http::Method::GET;
    let service = service_company::requested(&request);
    let operation_name = request.uri().path().rsplit('/').next().unwrap_or("");
    if service
        && !matches!(
            operation_name,
            "readiness" | "status" | "list" | "review-current" | "mcp-close"
        )
    {
        return refused(StatusCode::FORBIDDEN, "access_denied");
    }
    let key = if service {
        match service_company::proof(&state, &request, mutation) {
            Ok(k) => Some(k),
            Err(r) => return *r,
        }
    } else {
        None
    };
    let cookie = if service {
        None
    } else {
        match proof(&state, &request, mutation) {
            Ok(c) => Some(c),
            Err(r) => return *r,
        }
    };
    if request.headers().contains_key("idempotency-key") {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    }
    if !selected(&state, &request) {
        return refused(StatusCode::NOT_FOUND, "company_not_found");
    }
    let slug = request
        .uri()
        .path()
        .split('/')
        .nth(3)
        .unwrap_or("")
        .to_owned();
    let operation = request
        .uri()
        .path()
        .rsplit('/')
        .next()
        .unwrap_or("")
        .to_owned();
    let command = if mutation {
        if request.uri().query().is_some()
            || request.headers().contains_key("content-encoding")
            || !one(request.headers(), "content-type")
                .is_some_and(|s| s.eq_ignore_ascii_case("application/json"))
        {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        }
        let Ok(bytes) = to_bytes(request.into_body(), 65_536).await else {
            return refused(StatusCode::PAYLOAD_TOO_LARGE, "invalid_input");
        };
        let command = match operation.as_str() {
            "review-current" => {
                serde_json::from_slice::<rentemester_storage::periods::Bounds>(&bytes)
                    .ok()
                    .filter(|i| i.confirm && date(&i.period_start) && date(&i.period_end))
                    .map(Command::ReviewCurrent)
            }
            "review" => serde_json::from_slice::<Review>(&bytes)
                .ok()
                .filter(|i| {
                    i.confirm
                        && date(&i.period_start)
                        && date(&i.period_end)
                        && hash_valid(&i.packet_hash)
                })
                .map(Command::Review),
            "close" | "mcp-close" => serde_json::from_slice::<CloseInput>(&bytes)
                .ok()
                .filter(|i| {
                    i.confirm
                        && date(&i.period_start)
                        && date(&i.period_end)
                        && kind(i.kind.as_deref())
                        && i.status
                            .as_deref()
                            .is_none_or(|s| matches!(s, "closed" | "reported"))
                        && i.packet_hash.as_deref().is_none_or(hash_valid)
                        && i.review_id.is_none_or(id)
                        && i.reference.as_deref().is_none_or(bounded)
                        && i.reason.as_deref().is_none_or(bounded)
                })
                .map(|i| {
                    if operation == "mcp-close" {
                        Command::McpClose(i)
                    } else {
                        Command::Close(i)
                    }
                }),
            "reopen" => serde_json::from_slice::<Reopen>(&bytes)
                .ok()
                .filter(|i| {
                    i.confirm
                        && date(&i.period_start)
                        && date(&i.period_end)
                        && kind(i.kind.as_deref())
                        && bounded(&i.reason)
                })
                .map(Command::Reopen),
            _ => None,
        };
        let Some(command) = command else {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        };
        command
    } else {
        let mut from = None;
        let mut to = None;
        let mut review_id = None;
        for (key, value) in
            url::form_urlencoded::parse(request.uri().query().unwrap_or("").as_bytes())
        {
            let slot = match (operation.as_str(), key.as_ref()) {
                ("readiness", "from") => &mut from,
                ("readiness", "to") => &mut to,
                ("status", "reviewId") => &mut review_id,
                _ => return refused(StatusCode::BAD_REQUEST, "invalid_query"),
            };
            if slot.is_some() || value.len() > 32 {
                return refused(StatusCode::BAD_REQUEST, "invalid_query");
            }
            *slot = Some(value.into_owned());
        }
        if operation == "readiness" {
            match (from, to) {
                (Some(a), Some(b)) if date(&a) && date(&b) => Command::Readiness(a, b),
                _ => return refused(StatusCode::BAD_REQUEST, "invalid_query"),
            }
        } else if operation == "list" {
            Command::List
        } else {
            match review_id
                .and_then(|s| s.parse::<i64>().ok())
                .filter(|n| id(*n))
            {
                Some(n) => Command::Status(n),
                None => return refused(StatusCode::BAD_REQUEST, "invalid_query"),
            }
        }
    };
    let readiness = matches!(command, Command::Readiness(..));
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let today = chrono::DateTime::<chrono::Utc>::from(std::time::SystemTime::now())
        .format("%Y-%m-%d")
        .to_string();
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let mut owner = state.native.lock_company(&slug).map_err(|_| ())?;
        let action = match &command {
            Command::Readiness(a, b) => Action::OriginalReadiness(a, b),
            Command::List => Action::OriginalList,
            Command::ReviewCurrent(i) => Action::McpReview(i),
            Command::McpClose(i) => Action::McpClose(i),
            Command::Status(id) => Action::OriginalStatus(*id),
            Command::Review(i) => Action::OriginalReview(i),
            Command::Close(i) => Action::OriginalClose(i),
            Command::Reopen(i) => Action::OriginalReopen(i),
        };
        if let Some(key) = key {
            let attempt = owner
                .periods_service(&key, &SystemClock, action, &today)
                .map_err(|_| ())?;
            Ok::<_, ()>(match attempt.outcome {
                Ok(v) => period_response(v.result, readiness),
                Err(f) => service_company::failure(f),
            })
        } else {
            let attempt = owner
                .periods(cookie.as_ref().ok_or(())?, &SystemClock, action, &today)
                .map_err(|_| ())?;
            Ok(match super::documents::outcome(attempt) {
                Ok(value) => period_response(value, readiness),
                Err(r) => *r,
            })
        }
    })
    .await;
    match result {
        Ok(Ok(response)) => response,
        _ => refused(StatusCode::SERVICE_UNAVAILABLE, "outcome_unconfirmed"),
    }
}
fn period_response(mut value: serde_json::Value, readiness: bool) -> Response {
    if readiness {
        return response(StatusCode::OK, json!({"ok":true,"packet":value}));
    }
    if value["ok"] == false {
        if value["code"].as_str().is_none_or(|s| s.trim().is_empty()) {
            let code = value["errors"]
                .as_array()
                .and_then(|a| a.first())
                .and_then(|v| v.as_str())
                .unwrap_or("PERIOD_REFUSED")
                .split(':')
                .next()
                .unwrap_or("PERIOD_REFUSED")
                .to_owned();
            value["code"] = json!(code);
        }
        response(StatusCode::UNPROCESSABLE_ENTITY, value)
    } else {
        response(StatusCode::OK, value)
    }
}
