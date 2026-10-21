//! Native journal headers across fiscal years, through the existing source/authority fence.
use super::*;
use rentemester_application::archive_reports::Action;
use rentemester_storage::journal_history::Query;

fn parse(raw: &str) -> Result<Query, ()> {
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
    let mut query = Query::default();
    let mut seen = std::collections::BTreeSet::new();
    for (key, value) in url::form_urlencoded::parse(bytes) {
        if !seen.insert(key.clone())
            || value.is_empty()
            || value.len() > 64
            || value.chars().any(|c| c.is_control() || c == '\u{fffd}')
        {
            return Err(());
        }
        match key.as_ref() {
            "from" => query.from = Some(value.into_owned()),
            "to" => query.to = Some(value.into_owned()),
            "status" => query.status = Some(value.into_owned()),
            "all" if value == "true" => query.all = true,
            "limit" | "offset" => {
                if !value.bytes().all(|byte| byte.is_ascii_digit()) {
                    return Err(());
                }
                let number = value.parse().map_err(|_| ())?;
                if key == "limit" {
                    query.limit = Some(number);
                } else {
                    query.offset = Some(number);
                }
            }
            _ => return Err(()),
        }
    }
    query.validate().map_err(|_| ())?;
    Ok(query)
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
    if request.method() != axum::http::Method::GET
        || request.headers().contains_key("content-encoding")
        || request.headers().contains_key("transfer-encoding")
        || request.headers().contains_key("idempotency-key")
        || one(request.headers(), "content-length").is_some_and(|value| value != "0")
    {
        return refused(StatusCode::BAD_REQUEST, "invalid_query");
    }
    let query = match parse(request.uri().query().unwrap_or("")) {
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
        let mut owner = state.native.lock_company(&slug).map_err(|_| ())?;
        Ok::<_, ()>(match credential {
            Credential::Human(cookie) => match owner
                .journal_history(&cookie, &SystemClock, query)
                .map_err(|_| ())?
                .outcome
            {
                Ok(value) => Ok(json!({"ok":true,"journalHistory":value.value})),
                Err(failure) => {
                    let code = failure.error.code;
                    let status = if code == "invalid_credentials" {
                        StatusCode::UNAUTHORIZED
                    } else if code == "access_denied" && failure.stage == FailureStage::NotCommitted {
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
                    Err(Box::new(response(status, json!({"ok":false,"code":code,"errors":[failure.error.message]}))))
                }
            },
            Credential::Service(key) => match owner
                .archive_reports_service(&key, &SystemClock, Action::JournalHistory(query))
                .map_err(|_| ())?
                .outcome
            {
                Ok(value) => Ok(json!({"ok":true,"journalHistory":value.result,"usage":value.usage,"business":value.business})),
                Err(failure) => Err(Box::new(service_company::failure(failure))),
            },
        })
    })
    .await;
    match result {
        Ok(Ok(Ok(value))) => {
            let body = value.to_string();
            if body.len() > 96 * 1024 * 1024 {
                return refused(StatusCode::SERVICE_UNAVAILABLE, "journal_history_too_large");
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
        _ => refused(
            StatusCode::SERVICE_UNAVAILABLE,
            "journal_history_unavailable",
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_known_selectors_and_literal_numeric_bounds() {
        let q = parse("from=2025-01-01&to=2026-06-30&status=reversed&limit=1&offset=2").unwrap();
        assert_eq!(q.from.as_deref(), Some("2025-01-01"));
        assert_eq!(q.to.as_deref(), Some("2026-06-30"));
        assert_eq!(q.status.as_deref(), Some("reversed"));
        assert_eq!((q.limit, q.offset, q.all), (Some(1), Some(2), false));
        assert!(parse("all=true").unwrap().all);
        assert!(parse("").is_ok());
        // Legacy inclusive SQL returns an empty set for a reversed interval.
        assert!(parse("from=2026-02-02&to=2026-02-01").is_ok());
        for raw in [
            "limit=0",
            "limit=5001",
            "offset=-1",
            "offset=1.0",
            "offset=9007199254740992",
            "limit=1&limit=2",
            "from=2026-01-01&from=2026-01-02",
            "all=true&offset=0",
            "all=true&limit=500",
            "all=false",
            "status=active",
            "from=2026-02-30",
            "to=",
            "company=other",
            "source=archive",
            "year=2025",
            "status=%ff",
            "status=%00",
            "status=posted%",
            "status=posted%0",
            "limit=%2B1",
            "limit=1e2",
            "all=true&%61ll=true",
            "path=%2Ftmp%2Fother",
        ] {
            assert!(parse(raw).is_err(), "{raw}");
        }
        assert!(parse(&"x".repeat(4097)).is_err());
    }
}
