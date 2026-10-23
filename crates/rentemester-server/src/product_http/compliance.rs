//! Static repository data grants no authority. Company report uses current CompanyRead.
use super::*;
pub(super) async fn coverage() -> Response {
    static_report(false)
}
pub(super) async fn citations() -> Response {
    static_report(true)
}
fn static_report(citations: bool) -> Response {
    match rentemester_engine::regulatory::bundled_coverage() {
        Ok(coverage) => {
            let ok = coverage["ok"] == true;
            if citations {
                match rentemester_engine::regulatory::render_citations(&coverage) {
                    Ok(report) => response(
                        StatusCode::OK,
                        json!({"ok":ok,"regulatoryCitations":{"report":report,"coverage":coverage}}),
                    ),
                    Err(_) => refused(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "regulatory_render_failed",
                    ),
                }
            } else {
                response(
                    StatusCode::OK,
                    json!({"ok":ok,"regulatoryCoverage":coverage}),
                )
            }
        }
        Err(error) => response(
            StatusCode::INTERNAL_SERVER_ERROR,
            json!({"ok":false,"errors":[{"code":error.code,"error":error.message}]}),
        ),
    }
}
fn as_of(query: Option<&str>) -> Result<Option<&str>, ()> {
    let Some(query) = query else { return Ok(None) };
    let date = query.strip_prefix("asOf=").ok_or(())?;
    if !rentemester_engine::calendar::valid_iso_date(date) {
        return Err(());
    }
    Ok(Some(date))
}
pub(super) async fn handle(State(state): State<Product>, request: Request) -> Response {
    let slug = request
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
    let date = match as_of(request.uri().query()) {
        Ok(Some(d)) => d.to_owned(),
        Ok(None) => {
            let Ok(now) = rentemester_runtime::auth::Clock::now_unix_ms(&SystemClock) else {
                return refused(StatusCode::SERVICE_UNAVAILABLE, "clock_unavailable");
            };
            let Ok(d) = rentemester_runtime::posting_time::latest_date(now, 0) else {
                return refused(StatusCode::SERVICE_UNAVAILABLE, "clock_unavailable");
            };
            d
        }
        Err(()) => return refused(StatusCode::BAD_REQUEST, "invalid_query"),
    };
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let mut owner = state.native.lock_company(&slug).map_err(|_| ())?;
        owner
            .compliance(&cookie, &SystemClock, &date)
            .map_err(|_| ())
    })
    .await;
    match result {
        Ok(Ok(attempt)) => match super::documents::outcome(attempt) {
            Ok(value) => response(StatusCode::OK, json!({"ok":true,"complianceReport":value})),
            Err(r) => *r,
        },
        _ => refused(StatusCode::SERVICE_UNAVAILABLE, "outcome_unconfirmed"),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn strict_single_date() {
        assert_eq!(as_of(None), Ok(None));
        assert_eq!(as_of(Some("asOf=2026-01-01")), Ok(Some("2026-01-01")));
        for q in [
            "",
            "asOf=2026-02-30",
            "asOf=2026-01-01&asOf=2026-01-02",
            "actor=user:x",
            "asOf=2026-01-01&actor=x",
            "asOf=null",
            "asOf=%32%30%32%36-01-01",
        ] {
            assert!(as_of(Some(q)).is_err(), "{q}");
        }
    }
}
