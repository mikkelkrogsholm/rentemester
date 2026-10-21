//! Original numeric CLI report projections; fiscal-year web/export routes are unchanged.
use super::*;
use rentemester_application::archive_reports::Action;
use rentemester_storage::dated_reports::{Input, Kind};

fn kind(path: &str) -> Option<Kind> {
    match path.rsplit('/').next()? {
        "trial-balance" => Some(Kind::TrialBalance),
        "profit-loss" => Some(Kind::ProfitLoss),
        "balance" => Some(Kind::Balance),
        _ => None,
    }
}
fn parse(kind: Kind, raw: &str) -> Result<Input, ()> {
    if raw.len() > 4096 {
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
    let mut input = Input {
        kind,
        from: None,
        to: None,
        as_of: None,
    };
    let mut seen = std::collections::BTreeSet::new();
    for (key, value) in url::form_urlencoded::parse(bytes) {
        if !seen.insert(key.clone())
            || value.is_empty()
            || value.len() > 128
            || value.chars().any(|c| c.is_control() || c == '\u{fffd}')
        {
            return Err(());
        }
        match key.as_ref() {
            "from" => input.from = Some(value.into_owned()),
            "to" => input.to = Some(value.into_owned()),
            "asOf" => input.as_of = Some(value.into_owned()),
            _ => return Err(()),
        }
    }
    // Calendar/range validity is a business result in the original report contract.
    input.validate_shape().map_err(|_| ())?;
    Ok(input)
}
enum Credential {
    Human(rentemester_runtime::auth::VerifiedCookie),
    Service(rentemester_runtime::service_key::ServiceKey),
}
pub(super) async fn handle(State(state): State<Product>, request: Request) -> Response {
    let credential = if service_company::requested(&request) {
        match service_company::proof(&state, &request, false) {
            Ok(value) => Credential::Service(value),
            Err(response) => return *response,
        }
    } else {
        match proof(&state, &request, false) {
            Ok(value) => Credential::Human(value),
            Err(response) => return *response,
        }
    };
    if !selected(&state, &request) {
        return refused(StatusCode::NOT_FOUND, "company_not_found");
    }
    let Some(kind) = kind(request.uri().path()) else {
        return refused(StatusCode::NOT_FOUND, "not_found");
    };
    if request.method() != axum::http::Method::GET
        || request.headers().contains_key("content-encoding")
        || request.headers().contains_key("transfer-encoding")
        || request.headers().contains_key("idempotency-key")
        || one(request.headers(), "content-length").is_some_and(|value| value != "0")
    {
        return refused(StatusCode::BAD_REQUEST, "invalid_query");
    }
    let input = match parse(kind, request.uri().query().unwrap_or("")) {
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
    if !matches!(to_bytes(request.into_body(),0).await,Ok(bytes)if bytes.is_empty()) {
        return refused(StatusCode::BAD_REQUEST, "body_not_allowed");
    }
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let mut owner = state.native.lock_company(&slug).map_err(|_| ())?;
        Ok::<_, ()>(match credential {
            Credential::Human(cookie) => match owner
                .dated_report(&cookie, &SystemClock, input)
                .map_err(|_| ())?
                .outcome
            {
                Ok(value) => Ok(value.value),
                Err(failure) => {
                    let code = failure.error.code;
                    let status = if code == "invalid_credentials" {
                        StatusCode::UNAUTHORIZED
                    } else if code == "access_denied" && failure.stage == FailureStage::NotCommitted
                    {
                        StatusCode::FORBIDDEN
                    } else if failure.stage != FailureStage::NotCommitted
                        || code.contains("unavailable")
                        || !failure.control_reusable
                        || !failure.company_reusable
                    {
                        StatusCode::SERVICE_UNAVAILABLE
                    } else {
                        StatusCode::UNPROCESSABLE_ENTITY
                    };
                    Err(Box::new(response(
                        status,
                        json!({"ok":false,"code":code,"errors":[failure.error.message]}),
                    )))
                }
            },
            Credential::Service(key) => match owner
                .archive_reports_service(&key, &SystemClock, Action::DatedReport(input))
                .map_err(|_| ())?
                .outcome
            {
                Ok(value) => Ok(value.result),
                Err(failure) => Err(Box::new(service_company::failure(failure))),
            },
        })
    })
    .await;
    match result {
        Ok(Ok(Ok(value))) => {
            let body = value.to_string();
            if body.len() > 96 * 1024 * 1024 {
                return refused(StatusCode::SERVICE_UNAVAILABLE, "dated_report_too_large");
            }
            (
                StatusCode::OK,
                [
                    ("content-type", "application/json"),
                    ("cache-control", "no-store"),
                ],
                body,
            )
                .into_response()
        }
        Ok(Ok(Err(response))) => *response,
        _ => refused(StatusCode::SERVICE_UNAVAILABLE, "dated_report_unavailable"),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selectors_are_explicit_unique_and_do_not_reclassify_date_business_failures() {
        let q = parse(Kind::TrialBalance, "from=2025-12-31&to=2026-01-01").unwrap();
        assert_eq!(q.from.as_deref(), Some("2025-12-31"));
        assert_eq!(q.to.as_deref(), Some("2026-01-01"));
        assert!(parse(Kind::ProfitLoss, "from=not-date&to=2026-02-30").is_ok());
        assert!(parse(Kind::TrialBalance, "from=2026-02-02&to=2026-02-01").is_ok());
        assert!(parse(Kind::Balance, "asOf=not-date").is_ok());
        assert!(parse(Kind::Balance, "asOf=%202026-01-01%20").is_ok());
        for raw in [
            "",
            "from=2026-01-01",
            "from=2026-01-01&to=",
            "asOf=2026-01-01",
            "from=a&to=b&year=2026",
            "from=a&to=b&source=archive",
            "from=a&to=b&path=%2Ftmp",
            "from=a&to=b&from=c",
            "from=a&to=b&asOf=c",
            "from=%ff&to=b",
            "from=%00&to=b",
            "from=a%&to=b",
        ] {
            assert!(parse(Kind::TrialBalance, raw).is_err(), "{raw}");
        }
        assert!(parse(Kind::Balance, "asOf=2026-01-01&from=2026-01-01").is_err());
        assert!(parse(Kind::Balance, "asOf=2026-01-01&asOf=2026-01-02").is_err());
        assert!(kind("/api/companies/example/reports/other").is_none());
    }
}
