//! Original flat exception contracts over the native queue and current authority fences.
use super::*;
use rentemester_application::exceptions::Action;
use rentemester_storage::exceptions::Resolve;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ResolveWire {
    #[serde(default, deserialize_with = "note")]
    note: Option<String>,
}
fn note<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    String::deserialize(d).map(Some)
}
fn selectors(query: Option<&str>) -> Result<(String, bool), ()> {
    let query = query.unwrap_or("");
    if query.len() > 256
        || query.bytes().enumerate().any(|(i, b)| {
            b == b'%'
                && (i + 2 >= query.len()
                    || !query.as_bytes()[i + 1].is_ascii_hexdigit()
                    || !query.as_bytes()[i + 2].is_ascii_hexdigit())
        })
    {
        return Err(());
    }
    let mut status = None;
    let mut archived = None;
    for (key, value) in url::form_urlencoded::parse(query.as_bytes()) {
        match key.as_ref() {
            "status"
                if status.is_none()
                    && !value.is_empty()
                    && value.len() <= 128
                    && value.trim() == value
                    && !value.chars().any(|c| c.is_control() || c == '\u{fffd}') =>
            {
                status = Some(value.into_owned())
            }
            "includeArchived" if archived.is_none() => {
                archived = Some(match value.as_ref() {
                    "true" => true,
                    "false" => false,
                    _ => return Err(()),
                })
            }
            _ => return Err(()),
        }
    }
    Ok((
        status.unwrap_or_else(|| "open".into()),
        archived.unwrap_or(false),
    ))
}
fn success(value: Value, mutation: bool) -> Response {
    let flat = if mutation {
        json!({"ok":true,"resolved":value["resolved"],"errors":[]})
    } else {
        json!({"ok":true,"count":value["count"],"rows":value["rows"],"errors":[]})
    };
    response(StatusCode::OK, flat)
}
fn business(error: rentemester_engine::Error, mutation: bool) -> Response {
    let message = if !mutation && error.code == "exception_review_refused" {
        "status must be open, resolved, or all when present".to_owned()
    } else {
        error.message
    };
    let value = if mutation {
        json!({"ok":false,"resolved":false,"code":error.code,"errors":[message]})
    } else {
        json!({"ok":false,"count":0,"rows":[],"code":error.code,"errors":[message]})
    };
    response(StatusCode::UNPROCESSABLE_ENTITY, value)
}
pub(super) async fn handle(State(state): State<Product>, request: Request) -> Response {
    let mutation = request.method() != axum::http::Method::GET;
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
    let (status, archived, input) = if mutation {
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
        if wire.note.as_ref().is_some_and(|s| s.len() > 4096) {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        }
        (String::new(), false, Some(Resolve { note: wire.note }))
    } else {
        let (status, archived) = match selectors(request.uri().query()) {
            Ok(s) => s,
            Err(()) => return refused(StatusCode::BAD_REQUEST, "invalid_query"),
        };
        if request.headers().contains_key("transfer-encoding")
            || one(request.headers(), "content-length").is_some_and(|s| s != "0")
            || !matches!(to_bytes(request.into_body(), 0).await, Ok(b) if b.is_empty())
        {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        }
        (status, archived, None)
    };
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let mut owner = state.native.lock_company(&slug).map_err(|_| ())?;
        let action = match &input {
            Some(i) => Action::Resolve(id, i),
            None => Action::ListWithArchived(&status, archived),
        };
        let reply = if let Some(key) = key {
            match owner
                .exceptions_service(&key, &SystemClock, action)
                .map_err(|_| ())?
                .outcome
            {
                Ok(v) => success(v.result, mutation),
                Err(f)
                    if f.business
                        == rentemester_application::service_company::BusinessStage::NotCommitted
                        && f.control_reusable
                        && f.company_reusable
                        && f.error.code == "exception_review_refused" =>
                {
                    business(f.error, mutation)
                }
                Err(f) => service_company::failure(f),
            }
        } else {
            match owner
                .exceptions(cookie.as_ref().ok_or(())?, &SystemClock, action)
                .map_err(|_| ())?
                .outcome
            {
                Ok(v) => success(v.value, mutation),
                Err(f)
                    if f.stage == FailureStage::NotCommitted
                        && f.control_reusable
                        && f.company_reusable
                        && f.error.code == "exception_review_refused" =>
                {
                    business(f.error, mutation)
                }
                Err(f) if f.error.code == "invalid_credentials" => {
                    refused(StatusCode::UNAUTHORIZED, "unauthorized")
                }
                Err(f) if f.error.code == "access_denied" => {
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
    fn original_exception_selectors_and_exact_note_wire() {
        assert_eq!(selectors(None), Ok(("open".into(), false)));
        assert_eq!(
            selectors(Some("status=ignored&includeArchived=true")),
            Ok(("ignored".into(), true))
        );
        for q in [
            "status=all&status=open",
            "includeArchived=1",
            "status=%FF",
            "status=%",
            "actor=x",
        ] {
            assert!(selectors(Some(q)).is_err());
        }
        for body in [
            r#"{"note":null}"#,
            r#"{"actor":"user:x"}"#,
            r#"{"confirm":true}"#,
            r#"{"note":3}"#,
        ] {
            assert!(serde_json::from_str::<ResolveWire>(body).is_err());
        }
        assert!(
            serde_json::from_str::<ResolveWire>("{}")
                .unwrap()
                .note
                .is_none()
        );
    }
}
