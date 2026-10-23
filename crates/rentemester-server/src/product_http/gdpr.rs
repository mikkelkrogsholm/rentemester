use super::*;
use rentemester_application::gdpr::Action;
pub(super) async fn handle(State(state): State<Product>, request: Request) -> Response {
    let company_slug = request
        .uri()
        .path()
        .split('/')
        .nth(3)
        .unwrap_or("")
        .to_owned();
    let mutation = request.method() != axum::http::Method::GET;
    let operation = request
        .uri()
        .path()
        .rsplit('/')
        .next()
        .unwrap_or("")
        .to_owned();
    let erase = operation == "erase" || operation == "forget";
    let cookie = match proof(&state, &request, mutation) {
        Ok(c) => c,
        Err(r) => return *r,
    };
    if !selected(&state, &request) {
        return refused(StatusCode::NOT_FOUND, "company_not_found");
    }
    if request.uri().query().is_some() {
        return refused(StatusCode::BAD_REQUEST, "invalid_query");
    }
    let input = if mutation {
        if request.headers().contains_key("content-encoding")
            || !one(request.headers(), "content-type")
                .is_some_and(|v| v.eq_ignore_ascii_case("application/json"))
        {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        }
        let Ok(bytes) = to_bytes(request.into_body(), 16 * 1024).await else {
            return refused(StatusCode::PAYLOAD_TOO_LARGE, "invalid_input");
        };
        let Ok(value) = serde_json::from_slice::<Value>(&bytes) else {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        };
        if erase && value.get("asOf").is_some() {
            return refused(StatusCode::BAD_REQUEST, "erasure_does_not_accept_as_of");
        }
        Some(value)
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
    // Parse exact typed JSON before entering the worker; malformed requests are not unknown commits.
    if let Some(v) = &input {
        let valid = valid_input(&operation, v);
        if !valid {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        }
    }
    let envelope = match operation.as_str() {
        "retention" => "retention",
        "erase" | "forget" => "gdprErasure",
        "discover" => "gdprDiscovery",
        "legal-hold" => "gdprLegalHold",
        "audit-log" => "gdprAudit",
        _ => "gdpr",
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let mut owner = state.native.lock_company(&company_slug).map_err(|_| ())?;
        let subject;
        let forget;
        let hold;
        let audit;
        let action = match (operation.as_str(), &input) {
            ("erase", Some(v)) => {
                subject = serde_json::from_value(v.clone()).map_err(|_| ())?;
                Action::Erase(&subject)
            }
            ("export", Some(v)) => {
                subject = serde_json::from_value(v.clone()).map_err(|_| ())?;
                Action::Export(&subject)
            }
            ("discover", Some(v)) => {
                subject = serde_json::from_value(v.clone()).map_err(|_| ())?;
                Action::Discover(&subject)
            }
            ("forget", Some(v)) => {
                forget = serde_json::from_value(v.clone()).map_err(|_| ())?;
                Action::Forget(&forget)
            }
            ("legal-hold", Some(v)) => {
                hold = serde_json::from_value(v.clone()).map_err(|_| ())?;
                Action::LegalHold(&hold)
            }
            ("audit-log", Some(v)) => {
                audit = serde_json::from_value(v.clone()).map_err(|_| ())?;
                Action::AuditLog(&audit, state.native.backup_signer.as_ref().ok_or(())?)
            }
            ("retention", None) => Action::Retention,
            _ => return Err(()),
        };
        owner
            .gdpr(&cookie, &SystemClock, &today, action)
            .map_err(|_| ())
    })
    .await;
    match result {
        Ok(Ok(attempt)) => match super::documents::outcome(attempt) {
            Ok(value) => response(StatusCode::OK, json!({"ok":true,(envelope):value})),
            Err(r) => *r,
        },
        _ => refused(StatusCode::SERVICE_UNAVAILABLE, "outcome_unconfirmed"),
    }
}

fn valid_input(operation: &str, v: &Value) -> bool {
    use rentemester_storage::gdpr::*;
    if !matches!(operation, "export" | "erase") && v.get("confirm") != Some(&json!(true)) {
        return false;
    }
    match operation {
        "export" | "discover" => serde_json::from_value::<Input>(v.clone()).is_ok(),
        "erase" => v.get("asOf").is_none() && serde_json::from_value::<Input>(v.clone()).is_ok(),
        "forget" => serde_json::from_value::<ForgetInput>(v.clone()).is_ok(),
        "legal-hold" => serde_json::from_value::<LegalHoldInput>(v.clone()).is_ok(),
        "audit-log" => serde_json::from_value::<AuditInput>(v.clone()).is_ok(),
        _ => false,
    }
}
#[cfg(test)]
#[path = "gdpr_tests.rs"]
mod tests;
