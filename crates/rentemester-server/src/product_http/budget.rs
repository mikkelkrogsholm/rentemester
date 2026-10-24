use super::*;
use rentemester_application::budget::Action;
pub(super) async fn handle(State(state): State<Product>, request: Request) -> Response {
    // GET-backed HEAD must refuse before credentials, body parsing or mutation dispatch.
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
    if mutation && request.uri().query().is_some() {
        return refused(StatusCode::BAD_REQUEST, "invalid_query");
    }
    let dimensions = request.uri().path().ends_with("/budget-dimension-actuals");
    let mut year = None;
    let mut dimension = None;
    let mut member = None;
    for (key, value) in url::form_urlencoded::parse(request.uri().query().unwrap_or("").as_bytes())
    {
        let slot = match key.as_ref() {
            "year" => &mut year,
            "dimension" if dimensions => &mut dimension,
            "member" if dimensions => &mut member,
            _ => return refused(StatusCode::BAD_REQUEST, "invalid_query"),
        };
        if mutation || slot.is_some() || value.is_empty() || value.len() > 128 {
            return refused(StatusCode::BAD_REQUEST, "invalid_query");
        }
        *slot = Some(value.into_owned());
    }
    let compare = request.uri().path().ends_with("/budget-vs-actual");
    let key = if dimensions {
        "budgetDimensionActuals"
    } else if compare {
        "budgetVsActual"
    } else {
        "budget"
    };
    let input = if mutation {
        if request.headers().contains_key("content-encoding")
            || !one(request.headers(), "content-type")
                .is_some_and(|s| s.eq_ignore_ascii_case("application/json"))
        {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        }
        let Ok(bytes) = to_bytes(request.into_body(), 16 * 1024).await else {
            return refused(StatusCode::PAYLOAD_TOO_LARGE, "invalid_input");
        };
        match serde_json::from_slice::<rentemester_storage::budget::Set>(&bytes) {
            Ok(i) => Some(i),
            Err(_) => return refused(StatusCode::BAD_REQUEST, "invalid_input"),
        }
    } else {
        None
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
        let mut owner = state.native.lock_company(&company_slug).map_err(|_| ())?;
        let action = match &input {
            Some(i) => Action::Set(i),
            None if dimensions => Action::Dimensions {
                year: year.as_deref(),
                dimension: dimension.as_deref(),
                member: member.as_deref(),
            },
            None => Action::Read {
                year: year.as_deref(),
                compare,
            },
        };
        owner
            .budget(&cookie, &SystemClock, action, &today)
            .map_err(|_| ())
    })
    .await;
    match result {
        Ok(Ok(attempt)) => match super::documents::outcome(attempt) {
            Ok(value) => response(StatusCode::OK, json!({"ok":true,key:value})),
            Err(r) => *r,
        },
        _ => refused(StatusCode::SERVICE_UNAVAILABLE, "outcome_unconfirmed"),
    }
}
