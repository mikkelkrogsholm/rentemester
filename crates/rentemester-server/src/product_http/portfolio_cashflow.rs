//! Native financial overview selectors and current-authorized reads.
use super::*;

fn parse(raw: &str, key: &str) -> Result<Option<String>, ()> {
    let bytes = raw.as_bytes();
    if bytes.len() > 1024 {
        return Err(());
    }
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
    for (name, value) in url::form_urlencoded::parse(bytes) {
        if name != key
            || selected.is_some()
            || value.is_empty()
            || value.len() > 32
            || value.contains('\u{fffd}')
            || value.chars().any(char::is_control)
        {
            return Err(());
        }
        if key == "asOf" {
            rentemester_engine::fiscal_year::Date::parse(&value).map_err(|_| ())?;
        }
        selected = Some(value.into_owned());
    }
    Ok(selected)
}

fn read_request(request: &Request) -> bool {
    request.method() == axum::http::Method::GET
        && !request.headers().contains_key("content-encoding")
        && !request.headers().contains_key("transfer-encoding")
        && !request.headers().contains_key("idempotency-key")
        && one(request.headers(), "content-length").is_none_or(|value| value == "0")
}

fn failure(code: &'static str) -> Response {
    let status = match code {
        "access_denied" | "invalid_credentials" => StatusCode::FORBIDDEN,
        "invalid_date" | "invalid_year" | "invalid_fiscal_year" | "cashflow_invalid" => {
            StatusCode::BAD_REQUEST
        }
        _ => StatusCode::SERVICE_UNAVAILABLE,
    };
    refused(status, code)
}

pub(super) async fn portfolio(State(state): State<Product>, request: Request) -> Response {
    let cookie = match proof(&state, &request, false) {
        Ok(c) => c,
        Err(r) => return *r,
    };
    if !read_request(&request) {
        return refused(StatusCode::BAD_REQUEST, "invalid_query");
    }
    let selected = match parse(request.uri().query().unwrap_or(""), "asOf") {
        Ok(s) => s,
        Err(()) => return refused(StatusCode::BAD_REQUEST, "invalid_query"),
    };
    let Ok(now) = rentemester_runtime::auth::Clock::now_unix_ms(&SystemClock) else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "clock_unavailable");
    };
    let Ok(today) = rentemester_runtime::posting_time::latest_date(now, 0) else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "clock_unavailable");
    };
    let as_of = selected.unwrap_or(today);
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
            .portfolio(&state.native.cookies, &SystemClock, &cookie, &as_of)
            .map_err(|_| ())
    })
    .await;
    match result {
        Ok(Ok(attempt)) => match attempt.outcome {
            Ok(value) => response(StatusCode::OK, value),
            Err(f) => failure(f.error.code),
        },
        _ => refused(
            StatusCode::SERVICE_UNAVAILABLE,
            "portfolio_source_unavailable",
        ),
    }
}

pub(super) async fn cashflow(State(state): State<Product>, request: Request) -> Response {
    let cookie = match proof(&state, &request, false) {
        Ok(c) => c,
        Err(r) => return *r,
    };
    if !read_request(&request) {
        return refused(StatusCode::BAD_REQUEST, "invalid_query");
    }
    let slug = request
        .uri()
        .path()
        .split('/')
        .nth(3)
        .unwrap_or("")
        .to_owned();
    let year = match parse(request.uri().query().unwrap_or(""), "year") {
        Ok(s) => s,
        Err(()) => return refused(StatusCode::BAD_REQUEST, "invalid_query"),
    };
    let Ok(now) = rentemester_runtime::auth::Clock::now_unix_ms(&SystemClock) else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "clock_unavailable");
    };
    let Ok(today) = rentemester_runtime::posting_time::latest_date(now, 0) else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "clock_unavailable");
    };
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
            .cashflow(
                &state.native.cookies,
                &SystemClock,
                &cookie,
                &slug,
                &today,
                year.as_deref(),
            )
            .map_err(|_| ())
    })
    .await;
    match result {
        Ok(Ok(attempt)) => match attempt.outcome {
            Ok(value) => response(StatusCode::OK, value),
            Err(f) => failure(f.error.code),
        },
        _ => refused(
            StatusCode::SERVICE_UNAVAILABLE,
            "cashflow_source_unavailable",
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_selectors_preserve_fiscal_labels_and_refuse_ambiguous_queries() {
        assert_eq!(
            parse("year=2025%2F26", "year").unwrap().as_deref(),
            Some("2025/26")
        );
        assert_eq!(
            parse("asOf=2026-02-28", "asOf").unwrap().as_deref(),
            Some("2026-02-28")
        );
        assert_eq!(parse("", "asOf").unwrap(), None);
        for raw in [
            "asOf=2026-02-30",
            "asOf=2026-01-01&asOf=2026-01-02",
            "year=2026",
            "asOf=%FF",
            "asOf=%xx",
            "asOf=",
            "asOf=2026-01-01%00",
            "actor=user:a",
        ] {
            assert!(parse(raw, "asOf").is_err(), "{raw}");
        }
    }
}
