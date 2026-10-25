//! Review-only Agent suggestion consumer, using the existing current authority fences.
use super::*;
use rentemester_application::exceptions::Action;
use rentemester_storage::exceptions::AgentDecision;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ResolveWire {
    #[serde(default, deserialize_with = "note")]
    note: Option<String>,
}
fn note<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    String::deserialize(d).map(Some)
}
fn success(value: Value, mutation: bool) -> Response {
    response(
        StatusCode::OK,
        if mutation {
            json!({"ok":true,"suggestion":value})
        } else {
            json!({"ok":true,"agentSuggestions":value})
        },
    )
}
fn business(error: rentemester_engine::Error, _: bool) -> Response {
    let status = if error.code == "agent_suggestion_stale" {
        StatusCode::CONFLICT
    } else {
        StatusCode::BAD_REQUEST
    };
    response(
        status,
        json!({"ok":false,"code":error.code,"errors":[error.message]}),
    )
}
fn known(code: &str) -> bool {
    matches!(
        code,
        "agent_suggestion_stale" | "agent_suggestion_wrong_type" | "agent_suggestion_invalid"
    )
}
pub(super) async fn handle(State(state): State<Product>, request: Request) -> Response {
    if request.method() == axum::http::Method::HEAD {
        return refused(StatusCode::METHOD_NOT_ALLOWED, "method_not_allowed");
    }
    let mutation = request.method() == axum::http::Method::POST;
    let decision = if request.uri().path().ends_with("/approve") {
        AgentDecision::Approve
    } else {
        AgentDecision::Reject
    };
    let service = service_company::requested(&request);
    let key = if service {
        match service_company::proof(&state, &request, mutation) {
            Ok(k) => Some(k),
            Err(r) => return *r,
        }
    } else {
        None
    };
    let cookie = if service {
        None
    } else {
        match proof(&state, &request, mutation) {
            Ok(c) => Some(c),
            Err(r) => return *r,
        }
    };
    if !selected(&state, &request) {
        return refused(StatusCode::NOT_FOUND, "company_not_found");
    }
    if request.headers().contains_key("idempotency-key")
        || request.headers().contains_key("content-encoding")
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
    let mut id = 0;
    let input = if mutation {
        id = match request
            .uri()
            .path()
            .rsplit('/')
            .nth(1)
            .and_then(|s| s.parse::<i64>().ok())
            .filter(|n| (1..=9_007_199_254_740_991).contains(n))
        {
            Some(id) => id,
            None => return refused(StatusCode::BAD_REQUEST, "invalid_input"),
        };
        if request.uri().query().is_some()
            || !one(request.headers(), "content-type")
                .is_some_and(|s| s.eq_ignore_ascii_case("application/json"))
        {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        }
        let Ok(bytes) = to_bytes(request.into_body(), 16 * 1024).await else {
            return refused(StatusCode::PAYLOAD_TOO_LARGE, "invalid_input");
        };
        let Ok(wire) = serde_json::from_slice::<ResolveWire>(&bytes) else {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        };
        if wire
            .note
            .as_ref()
            .is_some_and(|s| decision.note(Some(s)).is_err())
        {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        }
        Some(wire)
    } else {
        if request.uri().query().is_some() {
            return refused(StatusCode::BAD_REQUEST, "invalid_query");
        }
        if request.headers().contains_key("transfer-encoding")
            || one(request.headers(), "content-length").is_some_and(|s| s != "0")
            || !matches!(to_bytes(request.into_body(), 0).await, Ok(b) if b.is_empty())
        {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        }
        None
    };
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let mut owner = state.native.lock_company(&slug).map_err(|_| ())?;
        let action = match &input {
            Some(i) => Action::AgentDecision(id, decision, i.note.as_deref()),
            None => Action::AgentSuggestions,
        };
        let reply = if let Some(key) = key {
            let attempt = owner
                .exceptions_service(&key, &SystemClock, action)
                .map_err(|_| ())?;
            if attempt.retired {
                return Ok(refused(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "outcome_unconfirmed",
                ));
            }
            match attempt.outcome {
                Ok(v) => success(v.result, mutation),
                Err(f)
                    if f.usage
                        != rentemester_application::service_company::UsageStage::OutcomeUnknown
                        && f.business
                            == rentemester_application::service_company::BusinessStage::NotCommitted
                        && f.control_reusable
                        && f.company_reusable && known(&f.error.code) =>
                {
                    business(f.error, mutation)
                }
                Err(f) => service_company::failure(f),
            }
        } else {
            let attempt = owner
                .exceptions(cookie.as_ref().ok_or(())?, &SystemClock, action)
                .map_err(|_| ())?;
            if attempt.retired {
                return Ok(refused(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "outcome_unconfirmed",
                ));
            }
            match attempt.outcome {
                Ok(v) => success(v.value, mutation),
                Err(f)
                    if f.stage == FailureStage::NotCommitted
                        && f.control_reusable
                        && f.company_reusable
                        && known(&f.error.code) =>
                {
                    business(f.error, mutation)
                }
                Err(f)
                    if f.stage == FailureStage::NotCommitted
                        && f.control_reusable
                        && f.company_reusable
                        && f.error.code == "invalid_credentials" =>
                {
                    refused(StatusCode::UNAUTHORIZED, "unauthorized")
                }
                Err(f)
                    if f.stage == FailureStage::NotCommitted
                        && f.control_reusable
                        && f.company_reusable
                        && f.error.code == "access_denied" =>
                {
                    refused(StatusCode::FORBIDDEN, "access_denied")
                }
                Err(_) => refused(StatusCode::SERVICE_UNAVAILABLE, "outcome_unconfirmed"),
            }
        };
        Ok::<_, ()>(reply)
    })
    .await;
    match result {
        Ok(Ok(reply)) => reply,
        _ => refused(StatusCode::SERVICE_UNAVAILABLE, "outcome_unconfirmed"),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn agent_suggestions_strict_note_wire_and_known_refusals() {
        for body in [
            r#"{"note":null}"#,
            r#"{"actor":"user:x"}"#,
            r#"{"confirm":true}"#,
            r#"{"note":3}"#,
            r#"{"note":"a","note":"b"}"#,
            r#"{"expectedActor":"x"}"#,
        ] {
            assert!(serde_json::from_str::<ResolveWire>(body).is_err(), "{body}");
        }
        assert!(
            serde_json::from_str::<ResolveWire>("{}")
                .unwrap()
                .note
                .is_none()
        );
        assert_eq!(
            serde_json::from_str::<ResolveWire>(r#"{"note":"hello"}"#)
                .unwrap()
                .note
                .as_deref(),
            Some("hello")
        );
        assert!(known("agent_suggestion_stale"));
        assert!(!known("commit_outcome_unknown"));
        assert_eq!(
            business(
                rentemester_engine::Error::new("agent_suggestion_stale", "stale"),
                true
            )
            .status(),
            StatusCode::CONFLICT
        );
        assert_eq!(
            business(
                rentemester_engine::Error::new("agent_suggestion_wrong_type", "wrong type"),
                true
            )
            .status(),
            StatusCode::BAD_REQUEST
        );
    }
}
