//! Exact read selectors and canonical outer envelopes for ordinary source-selected views.
use super::*;
use rentemester_application::archive_reports::Request as ReportRequest;
use rentemester_storage::archive_reports::Resource;
#[derive(Default)]
pub(super) struct Query {
    pub(super) year: Option<String>,
    pub(super) source: Option<String>,
    pub(super) account: Option<String>,
    pub(super) as_of: Option<String>,
}
pub(super) fn parse(raw: &str, resource: Option<Resource>) -> Result<Query, ()> {
    if raw.len() > 16 * 1024 {
        return Err(());
    }
    let b = raw.as_bytes();
    for (i, c) in b.iter().enumerate() {
        if *c == b'%'
            && (i + 2 >= b.len() || !b[i + 1].is_ascii_hexdigit() || !b[i + 2].is_ascii_hexdigit())
        {
            return Err(());
        }
    }
    let mut result = Query::default();
    for (key, value) in url::form_urlencoded::parse(b) {
        if value.is_empty()
            || value.len() > 128
            || value.trim() != value
            || value.chars().any(|c| c.is_control() || c == '\u{fffd}')
        {
            return Err(());
        }
        let slot = match key.as_ref() {
            "year" => &mut result.year,
            "source" => &mut result.source,
            "account" => &mut result.account,
            "asOf" => &mut result.as_of,
            _ => return Err(()),
        };
        if slot.replace(value.into_owned()).is_some() {
            return Err(());
        }
    }
    if result
        .source
        .as_deref()
        .is_some_and(|s| !matches!(s, "live" | "archive"))
        || result.source.as_deref() == Some("archive") && result.year.is_none()
    {
        return Err(());
    }
    if result.account.is_some() && !matches!(resource, Some(Resource::Journal))
        || result.as_of.is_some() && !matches!(resource, Some(Resource::Overview))
    {
        return Err(());
    }
    if result
        .as_of
        .as_deref()
        .is_some_and(|d| rentemester_engine::fiscal_year::Date::parse(d).is_err())
    {
        return Err(());
    }
    if resource.is_none()
        && (result.year.is_some()
            || result.source.is_some()
            || result.account.is_some()
            || result.as_of.is_some())
    {
        return Err(());
    }
    Ok(result)
}
pub(super) fn resource(path: &str) -> Option<(Option<Resource>, &'static str)> {
    match path.rsplit('/').next()? {
        "journal" => Some((Some(Resource::Journal), "journal")),
        "trial-balance" => Some((Some(Resource::TrialBalance), "trialBalance")),
        "income-statement" => Some((Some(Resource::IncomeStatement), "incomeStatement")),
        "balance" => Some((Some(Resource::Balance), "balance")),
        "overview" => Some((Some(Resource::Overview), "overview")),
        "fiscal-years" => Some((None, "fiscalYears")),
        _ => None,
    }
}
pub(super) async fn read(State(state): State<Product>, request: Request) -> Response {
    if service_company::requested(&request) { return service_company::read(state, request).await; }
    let cookie = match proof(&state, &request, false) {
        Ok(p) => p,
        Err(r) => return *r,
    };
    if !selected(&state, &request) {
        return refused(StatusCode::NOT_FOUND, "company_not_found");
    }
    if request.method() != axum::http::Method::GET
        || request.headers().contains_key("content-encoding")
        || request.headers().contains_key("transfer-encoding")
        || request.headers().contains_key("idempotency-key")
        || one(request.headers(), "content-length").is_some_and(|v| v != "0")
    {
        return refused(StatusCode::BAD_REQUEST, "invalid_query");
    }
    let Some((resource, key)) = resource(request.uri().path()) else {
        return refused(StatusCode::NOT_FOUND, "not_found");
    };
    let query = match parse(request.uri().query().unwrap_or(""), resource) {
        Ok(q) => q,
        Err(()) => return refused(StatusCode::BAD_REQUEST, "invalid_query"),
    };
    let slug = request
        .uri()
        .path()
        .split('/')
        .nth(3)
        .unwrap_or("")
        .to_owned();
    let today = match rentemester_runtime::auth::Clock::now_unix_ms(&SystemClock)
        .ok()
        .and_then(|now| rentemester_runtime::posting_time::latest_date(now, 0).ok())
    {
        Some(today) => today,
        None => return refused(StatusCode::SERVICE_UNAVAILABLE, "clock_unavailable"),
    };
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let mut owner = state.native.lock_company(&slug).map_err(|_| ())?;
        match resource {
            Some(resource) => owner.archive_reports(
                &cookie,
                &SystemClock,
                ReportRequest {
                    today: &today,
                    year: query.year.as_deref(),
                    source: query.source.as_deref(),
                    account: query.account.as_deref(),
                    as_of: query.as_of.as_deref(),
                    resource,
                },
            ),
            None => owner.archive_report_years(&cookie, &SystemClock, &today),
        }
        .map_err(|_| ())
    })
    .await;
    match result {
        Ok(Ok(attempt)) => match attempt.outcome {
            Ok(value) => response(StatusCode::OK, json!({"ok":true,key:value.value})),
            Err(f) => {
                let code = f.error.code;
                let status = if matches!(code, "access_denied" | "invalid_credentials") {
                    StatusCode::FORBIDDEN
                } else if matches!(
                    code,
                    "invalid_report_query" | "invalid_year" | "invalid_account"
                ) {
                    StatusCode::BAD_REQUEST
                } else if f.stage != FailureStage::NotCommitted
                    || code.contains("unavailable")
                    || !f.control_reusable
                    || !f.company_reusable
                {
                    StatusCode::SERVICE_UNAVAILABLE
                } else {
                    StatusCode::UNPROCESSABLE_ENTITY
                };
                response(
                    status,
                    json!({"ok":false,"code":code,"errors":[f.error.message]}),
                )
            }
        },
        _ => refused(StatusCode::SERVICE_UNAVAILABLE, "report_source_unavailable"),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_selectors_strict_and_readonly_resources_unambiguous() {
        assert!(
            parse(
                "year=2024&source=archive&account=1000",
                Some(Resource::Journal)
            )
            .is_ok()
        );
        assert!(
            parse(
                "year=2024&source=live&asOf=2024-06-01",
                Some(Resource::Overview)
            )
            .is_ok()
        );
        assert!(parse("", None).is_ok());
        assert!(parse("source=live", Some(Resource::Journal)).is_ok());
        for query in [
            "source=archive",
            "year=2024&source=legacy",
            "year=2024&year=2025",
            "year=2024&source=archive&source=live",
            "year=%FF",
            "year=%XX",
            "year=2024&actor=user:guessed",
        ] {
            assert!(parse(query, Some(Resource::Journal)).is_err(), "{query}");
        }
        assert!(parse("year=2024&account=1000", Some(Resource::Balance)).is_err());
        assert!(parse("year=2024&asOf=2024-02-30", Some(Resource::Overview)).is_err());
        assert!(parse("year=2024", None).is_err());
        assert_eq!(
            resource("/api/companies/a/income-statement").unwrap().1,
            "incomeStatement"
        );
        assert!(resource("/api/companies/a/journal/export").is_none());
    }
}
