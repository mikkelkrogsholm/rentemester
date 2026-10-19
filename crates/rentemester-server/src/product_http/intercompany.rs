//! Native explicit operator contracts; existing global reconciliation read stays compatible.
use super::*;
use rentemester_application::intercompany::{self as app, Action};
use rentemester_engine::intercompany::{Manifest, Mapping};
enum Command {
    Current,
    Read(String),
    Manifest(Manifest),
    Propose(Mapping),
    Approve(app::Exact),
    Revoke(app::Exact),
}
pub(super) async fn handle(
    State(state): State<Product>,
    axum::extract::Path(args): axum::extract::Path<std::collections::HashMap<String, String>>,
    request: Request,
) -> Response {
    dispatch(state, args.get("slug").cloned(), request).await
}
pub(super) async fn reconciliation(State(state): State<Product>, request: Request) -> Response {
    dispatch(state, None, request).await
}
async fn dispatch(state: Product, slug: Option<String>, request: Request) -> Response {
    let mutation = request.method() != axum::http::Method::GET;
    let cookie = match proof(&state, &request, mutation) {
        Ok(p) => p,
        Err(r) => return *r,
    };
    if slug.is_some() && !selected(&state, &request) {
        return refused(StatusCode::NOT_FOUND, "not_found");
    }
    let path = request.uri().path().to_owned();
    let command = if mutation {
        if request.uri().query().is_some()
            || request.headers().contains_key("content-encoding")
            || !one(request.headers(), "content-type")
                .is_some_and(|s| s.eq_ignore_ascii_case("application/json"))
        {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        }
        let Ok(bytes) = to_bytes(request.into_body(), 256 * 1024).await else {
            return refused(StatusCode::PAYLOAD_TOO_LARGE, "invalid_input");
        };
        let Ok(mut body) = serde_json::from_slice::<serde_json::Map<String, Value>>(&bytes) else {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        };
        if body.remove("confirm") != Some(json!(true)) {
            return refused(StatusCode::BAD_REQUEST, "confirmation_required");
        }
        let body = Value::Object(body);
        let parsed = match path.rsplit('/').next() {
            Some("manifest") => serde_json::from_value(body).map(Command::Manifest),
            Some("propose") => serde_json::from_value(body).map(Command::Propose),
            Some("approve") => serde_json::from_value(body).map(Command::Approve),
            Some("revoke") => serde_json::from_value(body).map(Command::Revoke),
            _ => return refused(StatusCode::NOT_FOUND, "not_found"),
        };
        match parsed {
            Ok(v) => v,
            Err(_) => return refused(StatusCode::BAD_REQUEST, "invalid_input"),
        }
    } else if path == "/api/group-reconciliation" {
        let pairs: Vec<_> =
            url::form_urlencoded::parse(request.uri().query().unwrap_or("").as_bytes()).collect();
        if pairs.len() != 1
            || pairs[0].0 != "asOf"
            || rentemester_engine::fiscal_year::Date::parse(&pairs[0].1).is_err()
        {
            return refused(StatusCode::BAD_REQUEST, "invalid_query");
        }
        Command::Read(pairs[0].1.to_string())
    } else {
        if request.uri().query().is_some() {
            return refused(StatusCode::BAD_REQUEST, "invalid_query");
        }
        Command::Current
    };
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let action = match &command {
            Command::Current => Action::Current,
            Command::Read(d) => Action::Reconciliation(d),
            Command::Manifest(m) => Action::Manifest(m),
            Command::Propose(m) => Action::Propose(m),
            Command::Approve(v) => Action::Approve(v),
            Command::Revoke(v) => Action::Revoke(v),
        };
        let mut owner = state.native.owner.lock().map_err(|_| ())?;
        if let Some(s) = &slug {
            owner.select_company(s).map_err(|_| ())?
        }
        owner
            .intercompany(
                &state.native.cookies,
                &SystemClock,
                &cookie,
                slug.as_deref(),
                action,
            )
            .map_err(|_| ())
    })
    .await;
    match result {
        Ok(Ok(a)) => match a.outcome {
            Ok(mut v) => {
                v["ok"] = json!(true);
                response(StatusCode::OK, v)
            }
            Err(f) => {
                use rentemester_application::password_login::FailureStage;
                let code = if f.stage == FailureStage::OutcomeUnknown {
                    "internal"
                } else {
                    f.error.code
                };
                refused(
                    if code == "internal" {
                        StatusCode::SERVICE_UNAVAILABLE
                    } else if matches!(code, "access_denied" | "invalid_credentials") {
                        StatusCode::FORBIDDEN
                    } else {
                        StatusCode::UNPROCESSABLE_ENTITY
                    },
                    code,
                )
            }
        },
        _ => refused(StatusCode::SERVICE_UNAVAILABLE, "internal"),
    }
}
