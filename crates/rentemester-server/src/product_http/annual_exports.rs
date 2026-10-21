use super::*;
pub(super) async fn read(State(state): State<Product>, request: Request) -> Response {
    let company_slug = request
        .uri()
        .path()
        .split('/')
        .nth(3)
        .unwrap_or("")
        .to_owned();
    let cookie = match proof(&state, &request, false) {
        Ok(c) => c,
        Err(r) => return *r,
    };
    if !selected(&state, &request) {
        return refused(StatusCode::NOT_FOUND, "company_not_found");
    }
    let mut start = None;
    let mut end = None;
    let mut kind = None;
    for (key, value) in url::form_urlencoded::parse(request.uri().query().unwrap_or("").as_bytes())
    {
        if key == "kind" {
            if kind.is_some() || !matches!(value.as_ref(), "tax" | "ixbrl") {
                return refused(StatusCode::BAD_REQUEST, "invalid_query");
            }
            kind = Some(value.into_owned());
            continue;
        }
        let slot = match key.as_ref() {
            "fiscalYearStart" => &mut start,
            "fiscalYearEnd" => &mut end,
            _ => return refused(StatusCode::BAD_REQUEST, "invalid_query"),
        };
        if slot.is_some() || rentemester_engine::fiscal_year::Date::parse(&value).is_err() {
            return refused(StatusCode::BAD_REQUEST, "invalid_query");
        }
        *slot = Some(value.into_owned());
    }
    let (Some(start), Some(end), Some(kind)) = (start, end, kind) else {
        return refused(StatusCode::BAD_REQUEST, "invalid_query");
    };
    if start > end || rentemester_engine::annual_report::prior_date(&start).is_err() {
        return refused(StatusCode::BAD_REQUEST, "invalid_query");
    }
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let mut owner = state.native.lock_company(&company_slug).map_err(|_| ())?;
        owner
            .annual_export(&cookie, &SystemClock, &start, &end, &kind)
            .map_err(|_| ())
    })
    .await;
    match result {
        Ok(Ok(attempt)) => match super::documents::outcome(attempt) {
            Ok(value) => response(StatusCode::OK, json!({"ok":true,"annualExport":value})),
            Err(r) => *r,
        },
        _ => refused(StatusCode::SERVICE_UNAVAILABLE, "read_unconfirmed"),
    }
}
