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
    Review(Review),
    Close(CloseInput),
    Reopen(Reopen),
}
fn date(value: &str) -> bool {
    rentemester_engine::fiscal_year::Date::parse(value).is_ok()
}
fn bounded(value: &str) -> bool {
    !value.trim().is_empty()
        && value.len() <= 4096
        && !value.chars().any(char::is_control)
}
fn id(value: i64) -> bool {
    (1..=9_007_199_254_740_991).contains(&value)
}
fn kind(value: Option<&str>) -> bool {
    value.is_none_or(|s| matches!(s, "vat_period" | "vat_quarter" | "fiscal_year" | "custom"))
}
pub(super) async fn handle(State(state): State<Product>, request: Request) -> Response {
    let mutation = request.method() != axum::http::Method::GET;
    let cookie = match proof(&state, &request, mutation) {
        Ok(c) => c,
        Err(r) => return *r,
    };
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
        let Ok(bytes) = to_bytes(request.into_body(), 16_384).await else {
            return refused(StatusCode::PAYLOAD_TOO_LARGE, "invalid_input");
        };
        let command = match operation.as_str() {
            "review" => serde_json::from_slice::<Review>(&bytes)
                .ok()
                .filter(|i| {
                    i.confirm
                        && date(&i.period_start)
                        && date(&i.period_end)
                        && hash_valid(&i.packet_hash)
                })
                .map(Command::Review),
            "close" => serde_json::from_slice::<CloseInput>(&bytes)
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
                .map(Command::Close),
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
            Command::Readiness(a, b) => Action::Readiness(a, b),
            Command::Status(id) => Action::OriginalStatus(*id),
            Command::Review(i) => Action::OriginalReview(i),
            Command::Close(i) => Action::OriginalClose(i),
            Command::Reopen(i) => Action::OriginalReopen(i),
        };
        owner
            .periods(&cookie, &SystemClock, action, &today)
            .map_err(|_| ())
    })
    .await;
    match result {
        Ok(Ok(attempt)) => match super::documents::outcome(attempt) {
            Ok(value) => response(
                StatusCode::OK,
                if readiness {
                    json!({"ok":true,"packet":value})
                } else {
                    value
                },
            ),
            Err(r) => *r,
        },
        _ => refused(StatusCode::SERVICE_UNAVAILABLE, "outcome_unconfirmed"),
    }
}
