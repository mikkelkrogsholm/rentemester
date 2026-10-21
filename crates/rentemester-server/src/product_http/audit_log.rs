//! Bounded company audit history; this does not perform full audit verification.
use super::*;
use rentemester_storage::audit_log::Query;
fn parse(raw: &str) -> Result<Query, ()> {
    if raw.len() > 4096 {
        return Err(());
    }
    let mut query = Query::default();
    let mut seen = std::collections::BTreeSet::new();
    for (key, value) in url::form_urlencoded::parse(raw.as_bytes()) {
        if !seen.insert(key.clone()) || value.contains('\u{fffd}') {
            return Err(());
        }
        let value = value.into_owned();
        match key.as_ref() {
            "limit" | "offset" => {
                if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
                    return Err(());
                }
                let number = value.parse().map_err(|_| ())?;
                if key == "limit" {
                    query.limit = Some(number);
                } else {
                    query.offset = Some(number);
                }
            }
            "fromDate" => query.from_date = Some(value),
            "toDate" => query.to_date = Some(value),
            "eventTypeLike" => query.event_type_like = Some(value),
            "actorLike" => query.actor_like = Some(value),
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
    let credential = if super::service_company::requested(&request) {
        match super::service_company::proof(&state, &request, false) {
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
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
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
    if !matches!(to_bytes(request.into_body(),0).await,Ok(bytes) if bytes.is_empty()) {
        return refused(StatusCode::BAD_REQUEST, "body_not_allowed");
    }
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let mut owner = state.native.lock_company(&slug).map_err(|_| ())?;
        Ok::<_, ()>(match credential {
            Credential::Human(cookie) => super::documents::outcome(
                owner
                    .audit_log(&cookie, &SystemClock, &query)
                    .map_err(|_| ())?,
            ),
            Credential::Service(key) => match owner
                .audit_log_service(&key, &SystemClock, &query)
                .map_err(|_| ())?
                .outcome
            {
                Ok(value) => Ok(value.result),
                Err(failure) => Err(Box::new(super::service_company::failure(failure))),
            },
        })
    })
    .await;
    match result {
        Ok(Ok(Ok(value))) => response(StatusCode::OK, value),
        Ok(Ok(Err(response))) => *response,
        _ => refused(StatusCode::SERVICE_UNAVAILABLE, "audit_log_unavailable"),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_known_unique_query_and_precise_numeric_inputs() {
        let query = parse(
            "limit=1&offset=2&actorLike=user%3Asynthetic&fromDate=2026-01-01&toDate=2026-01-02",
        )
        .unwrap();
        assert_eq!(query.limit, Some(1));
        assert_eq!(query.offset, Some(2));
        assert_eq!(query.actor_like.as_deref(), Some("user:synthetic"));
        for raw in [
            "limit=0",
            "limit=5001",
            "offset=-1",
            "offset=1.0",
            "limit=1&limit=2",
            "actorLike=a&actorLike=b",
            "company=other",
            "fromDate=2026-02-30",
            "fromDate=2026-02-02&toDate=2026-02-01",
            "actorLike=%00",
            "actorLike=%ff",
            "offset=9007199254740992",
        ] {
            assert!(parse(raw).is_err(), "{raw}");
        }
        assert!(parse(&format!("actorLike={}", "a".repeat(4096))).is_err());
    }
}
