//! Exact native reviewed elimination commands. Plan is read-only despite POST transport.
use super::*;
use rentemester_application::eliminations::{self as app, Action};
use rentemester_engine::eliminations::Input;
enum Command {
    Current,
    Overview(String),
    Plan(Input),
    Propose(app::Proposal),
    Approve(app::Exact),
    Reject(app::Exact),
    Apply(app::Exact),
    Reverse(app::Exact),
}
pub(super) async fn handle(
    State(state): State<Product>,
    axum::extract::Path(args): axum::extract::Path<std::collections::HashMap<String, String>>,
    request: Request,
) -> Response {
    dispatch(state, args.get("slug").cloned(), request).await
}
pub(super) async fn overview(State(state): State<Product>, request: Request) -> Response {
    dispatch(state, None, request).await
}
async fn dispatch(state: Product, slug: Option<String>, request: Request) -> Response {
    let post = request.method() != axum::http::Method::GET;
    let proof = match proof(&state, &request, post) {
        Ok(v) => v,
        Err(r) => return *r,
    };
    if slug.is_some() && !selected(&state, &request) {
        return refused(StatusCode::NOT_FOUND, "not_found");
    }
    let path = request.uri().path().to_owned();
    let command = if post {
        if request.uri().query().is_some()
            || request.headers().contains_key("content-encoding")
            || !one(request.headers(), "content-type")
                .is_some_and(|v| v.eq_ignore_ascii_case("application/json"))
        {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        }
        let Ok(bytes) = to_bytes(request.into_body(), 256 * 1024).await else {
            return refused(StatusCode::PAYLOAD_TOO_LARGE, "invalid_input");
        };
        let Ok(mut body) = serde_json::from_slice::<serde_json::Map<String, Value>>(&bytes) else {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        };
        let action = path.rsplit('/').next();
        if action != Some("plan") && body.remove("confirm") != Some(json!(true)) {
            return refused(StatusCode::BAD_REQUEST, "confirmation_required");
        }
        let body = Value::Object(body);
        let parsed = match action {
            Some("plan") => serde_json::from_value(body).map(Command::Plan),
            Some("propose") => serde_json::from_value(body).map(Command::Propose),
            Some("approve") => serde_json::from_value(body).map(Command::Approve),
            Some("reject") => serde_json::from_value(body).map(Command::Reject),
            Some("apply") => serde_json::from_value(body).map(Command::Apply),
            Some("reverse") => serde_json::from_value(body).map(Command::Reverse),
            _ => return refused(StatusCode::NOT_FOUND, "not_found"),
        };
        match parsed {
            Ok(c) => c,
            Err(_) => return refused(StatusCode::BAD_REQUEST, "invalid_input"),
        }
    } else if path == "/api/group-eliminations" {
        let pairs: Vec<_> =
            url::form_urlencoded::parse(request.uri().query().unwrap_or("").as_bytes()).collect();
        if pairs.len() != 1
            || pairs[0].0 != "asOf"
            || rentemester_engine::fiscal_year::Date::parse(&pairs[0].1).is_err()
        {
            return refused(StatusCode::BAD_REQUEST, "invalid_query");
        }
        Command::Overview(pairs[0].1.to_string())
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
            Command::Overview(s) => Action::Overview(s),
            Command::Plan(i) => Action::Plan(i),
            Command::Propose(i) => Action::Propose(i),
            Command::Approve(i) => Action::Approve(i),
            Command::Reject(i) => Action::Reject(i),
            Command::Apply(i) => Action::Apply(i),
            Command::Reverse(i) => Action::Reverse(i),
        };
        let mut owner = state.native.owner.lock().map_err(|_| ())?;
        if let Some(s) = &slug {
            owner.select_company(s).map_err(|_| ())?
        }
        owner
            .eliminations(
                &state.native.cookies,
                &SystemClock,
                &proof,
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
