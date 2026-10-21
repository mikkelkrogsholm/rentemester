//! Original juxtaposed MCP portfolio DTO; source collection belongs to the held Rust owner.
use super::*;
fn date(raw: &str, today_utc: &str) -> Result<String, ()> {
    if raw.len() > 256 {
        return Err(());
    }
    let bytes = raw.as_bytes();
    for (i, b) in bytes.iter().enumerate() {
        if *b == b'%'
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
    Ok(selected.unwrap_or_else(|| today_utc.to_owned()))
}
pub(super) async fn read(State(state): State<Product>, request: Request) -> Response {
    let cookie = match proof(&state, &request, false) {
        Ok(value) => value,
        Err(reply) => return *reply,
    };
    if request.method() != axum::http::Method::GET
        || request.headers().contains_key("content-encoding")
        || request.headers().contains_key("transfer-encoding")
        || request.headers().contains_key("idempotency-key")
        || one(request.headers(), "content-length").is_some_and(|v| v != "0")
    {
        return refused(StatusCode::BAD_REQUEST, "invalid_query");
    }
    let Ok(now) = rentemester_runtime::auth::Clock::now_unix_ms(&SystemClock) else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "clock_unavailable");
    };
    let Some(utc) = chrono::DateTime::<chrono::Utc>::from_timestamp_millis(now) else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "clock_unavailable");
    };
    let Ok(today) = rentemester_runtime::posting_time::latest_date(now, 0) else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "clock_unavailable");
    };
    let as_of = match date(
        request.uri().query().unwrap_or(""),
        &utc.format("%Y-%m-%d").to_string(),
    ) {
        Ok(value) => value,
        Err(()) => return refused(StatusCode::BAD_REQUEST, "invalid_query"),
    };
    if !matches!(to_bytes(request.into_body(),0).await,Ok(bytes)if bytes.is_empty()) {
        return refused(StatusCode::BAD_REQUEST, "body_not_allowed");
    }
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        state
            .native
            .owner
            .lock()
            .map_err(|_| ())?
            .portfolio_overview(&state.native.cookies, &SystemClock, &cookie, &as_of, &today)
            .map_err(|_| ())
    })
    .await;
    let value = match result {
        Ok(Ok(attempt)) => match attempt.outcome {
            Ok(value) => value,
            Err(failure) => {
                return refused(
                    match failure.error.code {
                        "invalid_credentials" => StatusCode::UNAUTHORIZED,
                        "access_denied" => StatusCode::FORBIDDEN,
                        "invalid_date" => StatusCode::BAD_REQUEST,
                        _ => StatusCode::SERVICE_UNAVAILABLE,
                    },
                    failure.error.code,
                );
            }
        },
        _ => {
            return refused(
                StatusCode::SERVICE_UNAVAILABLE,
                "portfolio_source_unavailable",
            );
        }
    };
    let body = json!({"ok":true,"overview":value}).to_string();
    if body.len() > 96 * 1024 * 1024 {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "portfolio_too_large");
    }
    (
        StatusCode::OK,
        [
            ("content-type", "application/json"),
            ("cache-control", "private, no-store"),
        ],
        body,
    )
        .into_response()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_utc_default_and_unique_explicit_date_selector() {
        assert_eq!(date("", "2026-01-01").unwrap(), "2026-01-01");
        assert_eq!(date("asOf=2025-12-31", "2026-01-01").unwrap(), "2025-12-31");
        for raw in [
            "asOf=",
            "asOf=2026-02-30",
            "asOf=%202026-01-01",
            "asOf=2026-01-01&asOf=2026-01-02",
            "workspace=/tmp",
            "company=hidden",
            "asOf=2026-01-01&source=archive",
            "asOf=%ff",
            "asOf=2026-01-01%",
        ] {
            assert!(date(raw, "2026-01-01").is_err(), "{raw}");
        }
    }
}
