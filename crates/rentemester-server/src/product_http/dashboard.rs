use super::*;
pub(super) async fn read(State(state): State<Product>, request: Request) -> Response {
    let cookie = match proof(&state, &request, false) {
        Ok(c) => c,
        Err(r) => return *r,
    };
    if !selected(&state, &request) {
        return refused(StatusCode::NOT_FOUND, "company_not_found");
    }
    let Ok(now) = rentemester_runtime::auth::Clock::now_unix_ms(&SystemClock) else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "clock_unavailable");
    };
    let Ok(today) = rentemester_runtime::posting_time::latest_date(now, 0) else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "clock_unavailable");
    };
    let mut year = None;
    let mut as_of = None;
    for (key, value) in url::form_urlencoded::parse(request.uri().query().unwrap_or("").as_bytes())
    {
        match key.as_ref() {
            "year" if year.is_none() && !value.is_empty() && value.len() <= 16 => {
                year = Some(value.into_owned())
            }
            "asOf"
                if as_of.is_none()
                    && rentemester_engine::fiscal_year::Date::parse(&value).is_ok() =>
            {
                as_of = Some(value.into_owned())
            }
            _ => return refused(StatusCode::BAD_REQUEST, "invalid_query"),
        }
    }
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let mut owner = state.native.owner.lock().map_err(|_| ())?;
        owner
            .dashboard(
                &cookie,
                &SystemClock,
                &today,
                year.as_deref(),
                as_of.as_deref(),
            )
            .map_err(|_| ())
    })
    .await;
    match result {
        Ok(Ok(attempt)) => match super::documents::outcome(attempt) {
            Ok(value) => response(StatusCode::OK, json!({"ok":true,"overview":value})),
            Err(r) => *r,
        },
        _ => refused(StatusCode::SERVICE_UNAVAILABLE, "read_unconfirmed"),
    }
}
