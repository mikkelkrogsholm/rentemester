//! Full accounting verification; successful transport can carry a failed audit.
use super::*;
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
    if request.uri().query().is_some() {
        return refused(StatusCode::BAD_REQUEST, "invalid_query");
    }
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
                owner.audit_verify(&cookie, &SystemClock).map_err(|_| ())?,
            ),
            Credential::Service(key) => match owner
                .audit_verify_service(&key, &SystemClock)
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
        _ => refused(
            StatusCode::SERVICE_UNAVAILABLE,
            "audit_verification_unavailable",
        ),
    }
}
