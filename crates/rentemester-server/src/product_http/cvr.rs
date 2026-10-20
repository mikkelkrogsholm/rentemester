//! Existing CVR forms: lazy operator lookup and explicit own-company sync.
use super::*;
use rentemester_application::cvr::Action;
use rentemester_runtime::contact_import::OperatorLookup;

fn selector(query: Option<&str>) -> Result<String, ()> {
    let query = query.ok_or(())?;
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
    let mut fields = url::form_urlencoded::parse(query.as_bytes());
    let (key, value) = fields.next().ok_or(())?;
    if key != "cvr" || value.trim().is_empty() || fields.next().is_some() {
        return Err(());
    }
    Ok(value.into_owned())
}
fn sync_body(bytes: &[u8]) -> bool {
    bytes.is_empty()
        || serde_json::from_slice::<Value>(bytes)
            .is_ok_and(|value| value.as_object().is_some_and(|object| object.is_empty()))
}
pub(super) async fn handle(State(state): State<Product>, request: Request) -> Response {
    let sync = request.method() == axum::http::Method::POST;
    let cookie = match proof(&state, &request, sync) {
        Ok(cookie) => cookie,
        Err(r) => return *r,
    };
    if !selected(&state, &request) {
        return refused(StatusCode::NOT_FOUND, "company_not_found");
    }
    if request.headers().contains_key("content-encoding")
        || request.headers().contains_key("idempotency-key")
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
    let cvr = if sync {
        if request.uri().query().is_some()
            || request.headers().contains_key("content-type")
                && !one(request.headers(), "content-type")
                    .is_some_and(|s| s.eq_ignore_ascii_case("application/json"))
        {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        }
        let Ok(bytes) = to_bytes(request.into_body(), 4096).await else {
            return refused(StatusCode::PAYLOAD_TOO_LARGE, "invalid_input");
        };
        if !sync_body(&bytes) {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        }
        None
    } else {
        let cvr = match selector(request.uri().query()) {
            Ok(cvr) => cvr,
            Err(()) => return refused(StatusCode::BAD_REQUEST, "invalid_query"),
        };
        if to_bytes(request.into_body(), 0).await.is_err() {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        }
        Some(cvr)
    };
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let mut owner = state.native.lock_company(&slug).map_err(|_| ())?;
        // Construction is inert: credentials/client/CA are read only after app authorization.
        let mut lookup = OperatorLookup::default();
        let action = cvr.as_deref().map_or(Action::Sync, Action::Lookup);
        owner
            .cvr(&cookie, &SystemClock, action, &mut lookup)
            .map_err(|_| ())
    })
    .await;
    match result {
        Ok(Ok(attempt)) => match super::documents::outcome(attempt) {
            Ok(value) => response(
                StatusCode::OK,
                if sync {
                    json!({"ok":true,"sync":value})
                } else {
                    json!({"ok":true,"cvr":value})
                },
            ),
            Err(r) => *r,
        },
        _ => refused(StatusCode::SERVICE_UNAVAILABLE, "outcome_unconfirmed"),
    }
}
fn configured(username: Option<&str>, password: Option<&str>) -> bool {
    username.is_some_and(|s| !s.is_empty()) && password.is_some_and(|s| !s.is_empty())
}
pub(super) async fn status(State(state): State<Product>, request: Request) -> Response {
    let cookie = match proof(&state, &request, false) {
        Ok(cookie) => cookie,
        Err(r) => return *r,
    };
    if request.uri().query().is_some() {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    }
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let attempt = state
            .native
            .owner
            .lock()
            .map_err(|_| ())?
            .current_session(&state.native.cookies, &SystemClock, &cookie)
            .map_err(|_| ())?;
        attempt.outcome.map_err(|_| ())?;
        let user = std::env::var("CVR_USERNAME").ok();
        let password = std::env::var("CVR_PASSWORD").ok();
        Ok::<_, ()>(configured(user.as_deref(), password.as_deref()))
    })
    .await;
    match result {
        Ok(Ok(configured)) => response(
            StatusCode::OK,
            json!({"ok":true,"cvrStatus":{"configured":configured}}),
        ),
        _ => refused(StatusCode::UNAUTHORIZED, "unauthorized"),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lookup_selectors_are_exact_but_normalization_stays_in_the_core() {
        assert_eq!(
            selector(Some("cvr=DK+12+34+56+78")).unwrap(),
            "DK 12 34 56 78"
        );
        assert_eq!(selector(Some("cvr=invalid")).unwrap(), "invalid");
        for q in [
            None,
            Some(""),
            Some("cvr="),
            Some("cvr=+"),
            Some("cvr=1&cvr=2"),
            Some("cvr=12345678&endpoint=x"),
            Some("cvr=%ZZ"),
        ] {
            assert!(selector(q).is_err());
        }
    }
    #[test]
    fn explicit_sync_body_cannot_supply_configuration_or_actor() {
        assert!(sync_body(b""));
        assert!(sync_body(b"{}"));
        for body in [
            b"null".as_slice(),
            b"[]",
            br#"{"confirm":true}"#,
            br#"{"actor":"user:x"}"#,
            br#"{"endpoint":"https://untrusted.invalid"}"#,
        ] {
            assert!(!sync_body(body));
        }
        assert!(!configured(None, None));
        assert!(!configured(Some("user"), Some("")));
        assert!(configured(Some("user"), Some("password")));
    }
}
