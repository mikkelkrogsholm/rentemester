//! Additive source-shaped routes. Native keyed endpoints are intentionally untouched.
use super::*;
use rentemester_application::journal_original;

fn rejected(code: &str, business: Option<Value>) -> Response {
    let mut value = business.unwrap_or_else(|| json!({"ok":false,"errors":[code]}));
    // Native writer refusals may omit a code; preserve their actual errors and
    // rules while supplying the bounded source transport's required code.
    if value.get("code").is_none() { value["code"] = json!(code); }
    if value.get("appliedRules").is_none() { value["appliedRules"] = json!([]); }
    response(StatusCode::UNPROCESSABLE_ENTITY, value)
}

enum Principal {
    Human(rentemester_runtime::auth::VerifiedCookie),
    Service(rentemester_runtime::service_key::ServiceKey),
}
pub(super) async fn handle(State(state): State<Product>, request: Request) -> Response {
    let principal = if service_company::requested(&request) {
        match service_company::proof(&state, &request, true) {
            Ok(key) => Principal::Service(key),
            Err(r) => return *r,
        }
    } else {
        match proof(&state, &request, true) {
            Ok(cookie) => Principal::Human(cookie),
            Err(r) => return *r,
        }
    };
    if !selected(&state, &request) {
        return refused(StatusCode::NOT_FOUND, "company_not_found");
    }
    if request.uri().query().is_some()
        || request.headers().contains_key("content-encoding")
        || !one(request.headers(), "content-type").is_some_and(|s| {
            s.eq_ignore_ascii_case("application/json")
                || s.eq_ignore_ascii_case("application/json; charset=utf-8")
        })
    {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    }
    let slug = request
        .uri()
        .path()
        .split('/')
        .nth(3)
        .unwrap_or("")
        .to_owned();
    let operation = request
        .uri()
        .path()
        .rsplit('/')
        .next()
        .unwrap_or("")
        .to_owned();
    let bytes = match to_bytes(request.into_body(), 65_536).await {
        Ok(v) => v,
        Err(_) => return refused(StatusCode::PAYLOAD_TOO_LARGE, "body_too_large"),
    };
    let action = match journal_original::parse(&operation, &bytes) {
        Ok(v) => v,
        Err(e) if e.code == "journal_input_refused" => {
            return response(
                StatusCode::UNPROCESSABLE_ENTITY,
                json!({"ok":false,"code":e.code,"appliedRules":[],"errors":[e.message]}),
            );
        }
        Err(e) => return refused(StatusCode::BAD_REQUEST, e.code),
    };
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let dry_run = operation == "dry-run";
    let unknown = if dry_run {
        "dry_run_unconfirmed"
    } else {
        "outcome_unknown"
    };
    let unconfirmed = if dry_run {
        "dry_run_unconfirmed"
    } else {
        "outcome_unconfirmed"
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let mut owner = state.native.lock_company(&slug).map_err(|_| ())?;
        Ok::<_, ()>(match principal {
            Principal::Human(cookie) => match owner
                .journal_original_human(&cookie, &SystemClock, action)
                .map_err(|_| ())?
                .outcome
            {
                Ok(v) => response(StatusCode::OK, v.value),
                Err(f) if f.fence.stage == FailureStage::OutcomeUnknown => {
                    refused(StatusCode::SERVICE_UNAVAILABLE, unknown)
                }
                Err(f) if f.fence.error.code == "invalid_credentials" => {
                    refused(StatusCode::UNAUTHORIZED, "unauthorized")
                }
                Err(f) if f.fence.error.code == "access_denied" => {
                    refused(StatusCode::FORBIDDEN, "access_denied")
                }
                Err(f) => rejected(f.fence.error.code, f.business),
            },
            Principal::Service(key) => match owner
                .journal_original_service(&key, &SystemClock, action)
                .map_err(|_| ())?
                .outcome
            {
                Ok(v) => response(StatusCode::OK, v.result),
                Err(f) => {
                    use rentemester_application::service_company::{BusinessStage, UsageStage};
                    if f.usage == UsageStage::OutcomeUnknown
                        || f.business == BusinessStage::OutcomeUnknown
                    {
                        refused(StatusCode::SERVICE_UNAVAILABLE, unknown)
                    } else if f.error.code == "invalid_credentials" {
                        refused(StatusCode::UNAUTHORIZED, "unauthorized")
                    } else if f.error.code == "access_denied" {
                        refused(StatusCode::FORBIDDEN, "access_denied")
                    } else {
                        rejected(f.error.code, f.rejection)
                    }
                }
            },
        })
    })
    .await;
    match result {
        Ok(Ok(response)) => response,
        _ => refused(StatusCode::SERVICE_UNAVAILABLE, unconfirmed),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn journal_original_native_refusals_preserve_errors_rules_and_transport_code() {
        for (code, business, expected) in [
            ("business_rejection", Some(json!({"ok":false,"errors":["Closed period"],"appliedRules":["PERIOD_LOCK"]})), json!({"ok":false,"code":"business_rejection","errors":["Closed period"],"appliedRules":["PERIOD_LOCK"]})),
            ("period_closed", None, json!({"ok":false,"code":"period_closed","errors":["period_closed"],"appliedRules":[]})),
            ("business_rejection", Some(json!({"ok":false,"code":"journal_selection_usage","errors":["Ambiguous selector"],"appliedRules":[]})), json!({"ok":false,"code":"journal_selection_usage","errors":["Ambiguous selector"],"appliedRules":[]})),
        ] {
            let response = rejected(code, business);
            assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
            let bytes = to_bytes(response.into_body(), 65536).await.unwrap();
            assert_eq!(serde_json::from_slice::<Value>(&bytes).unwrap(), expected);
        }
    }
}
