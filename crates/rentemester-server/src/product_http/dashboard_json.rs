//! Original dashboard JSON, composed under one current-authorized source read.
use super::*;

fn date(raw: &str, today: &str) -> Result<String, ()> {
    if raw.len() > 256 {
        return Err(());
    }
    let bytes = raw.as_bytes();
    for (i, byte) in bytes.iter().enumerate() {
        if *byte == b'%'
            && (i + 2 >= bytes.len()
                || !bytes[i + 1].is_ascii_hexdigit()
                || !bytes[i + 2].is_ascii_hexdigit())
        {
            return Err(());
        }
    }
    let mut selected = None;
    for (key, value) in url::form_urlencoded::parse(bytes) {
        if key != "asOf"
            || selected.is_some()
            || rentemester_engine::fiscal_year::Date::parse(&value).is_err()
        {
            return Err(());
        }
        selected = Some(value.into_owned());
    }
    Ok(selected.unwrap_or_else(|| today.to_owned()))
}
pub(super) async fn read(State(state): State<Product>, request: Request) -> Response {
    let cookie = match proof(&state, &request, false) {
        Ok(value) => value,
        Err(reply) => return *reply,
    };
    if !selected(&state, &request) {
        return refused(StatusCode::NOT_FOUND, "company_not_found");
    }
    if request.method() != axum::http::Method::GET
        || request.headers().contains_key("content-encoding")
        || request.headers().contains_key("transfer-encoding")
        || request.headers().contains_key("idempotency-key")
        || one(request.headers(), "content-length").is_some_and(|value| value != "0")
    {
        return refused(StatusCode::BAD_REQUEST, "invalid_query");
    }
    let Ok(now) = rentemester_runtime::auth::Clock::now_unix_ms(&SystemClock) else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "clock_unavailable");
    };
    let Ok(today) = rentemester_runtime::posting_time::latest_date(now, 0) else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "clock_unavailable");
    };
    let as_of = match date(request.uri().query().unwrap_or(""), &today) {
        Ok(value) => value,
        Err(()) => return refused(StatusCode::BAD_REQUEST, "invalid_query"),
    };
    let slug = request
        .uri()
        .path()
        .split('/')
        .nth(3)
        .unwrap_or("")
        .to_owned();
    if !matches!(to_bytes(request.into_body(), 0).await, Ok(bytes) if bytes.is_empty()) {
        return refused(StatusCode::BAD_REQUEST, "body_not_allowed");
    }
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        state
            .native
            .lock_company(&slug)
            .map_err(|_| ())?
            .dashboard_json(&cookie, &SystemClock, &as_of, &today)
            .map_err(|_| ())
    })
    .await;
    let dashboard = match result {
        Ok(Ok(attempt)) => match documents::outcome(attempt) {
            Ok(value) => value,
            Err(reply) => return *reply,
        },
        _ => return refused(StatusCode::SERVICE_UNAVAILABLE, "dashboard_unavailable"),
    };
    let body = json!({"ok":true,"dashboard":dashboard}).to_string();
    if body.len() > 96 * 1024 * 1024 {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "dashboard_too_large");
    }
    (
        StatusCode::OK,
        [
            ("content-type", "application/json"),
            ("cache-control", "private, no-store"),
            ("x-content-type-options", "nosniff"),
        ],
        body,
    )
        .into_response()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_date_selector_is_unique_explicit_and_defaults_to_trusted_date() {
        assert_eq!(date("", "2026-01-02").unwrap(), "2026-01-02");
        assert_eq!(date("asOf=2025-12-31", "2026-01-02").unwrap(), "2025-12-31");
        for raw in [
            "asOf=",
            "asOf=2026-02-30",
            "asOf=%202026-01-01%20",
            "asOf=2026-01-01&asOf=2026-01-02",
            "year=2026",
            "source=archive",
            "asOf=2026-01-01%",
            "asOf=%ff",
            "asOf=2026-01-01&path=x",
        ] {
            assert!(date(raw, "2026-01-02").is_err(), "{raw}");
        }
    }
}
