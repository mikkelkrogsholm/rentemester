//! Native own-session contract: IDs are revocation targets; bearers never leave the server.
use super::*;
use rentemester_application::session_control::Operation as NativeOperation;

pub enum SessionAction {
    List,
    RevokeOne(String),
    RevokeOthers,
    RevokeAll,
}
pub struct SessionControlInput {
    proof: VerifiedCookie,
    action: SessionAction,
}
impl SessionControlInput {
    pub fn proof(&self) -> &VerifiedCookie {
        &self.proof
    }
    pub fn operation(&self) -> NativeOperation<'_> {
        match &self.action {
            SessionAction::List => NativeOperation::List,
            SessionAction::RevokeOne(id) => NativeOperation::RevokeOne(id),
            SessionAction::RevokeOthers => NativeOperation::RevokeOthers,
            SessionAction::RevokeAll => NativeOperation::RevokeAll,
        }
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Target {
    session_id: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Empty {}
pub(super) fn routes(router: Router<Arc<StateData>>) -> Router<Arc<StateData>> {
    router
        .route(
            "/api/auth/list-sessions",
            get(list).head(|| async {
                let mut r = refused(StatusCode::METHOD_NOT_ALLOWED);
                r.headers_mut()
                    .insert("allow", HeaderValue::from_static("GET"));
                r
            }),
        )
        .route("/api/auth/revoke-session", post(one_session))
        .route("/api/auth/revoke-other-sessions", post(others))
        .route("/api/auth/revoke-sessions", post(all))
}
async fn list(state: State<Arc<StateData>>, r: Request) -> Response {
    execute(state, r, SessionAction::List).await
}
async fn one_session(state: State<Arc<StateData>>, r: Request) -> Response {
    // The target is decoded only after browser and cookie admission.
    execute(state, r, SessionAction::RevokeOne(String::new())).await
}
async fn others(state: State<Arc<StateData>>, r: Request) -> Response {
    execute(state, r, SessionAction::RevokeOthers).await
}
async fn all(state: State<Arc<StateData>>, r: Request) -> Response {
    execute(state, r, SessionAction::RevokeAll).await
}
async fn execute(
    State(state): State<Arc<StateData>>,
    request: Request,
    mut action: SessionAction,
) -> Response {
    if auth(&state).session_control.is_none() {
        return refused(StatusCode::NOT_FOUND);
    }
    let listing = matches!(action, SessionAction::List);
    let op = if listing {
        Operation::SessionList
    } else {
        Operation::SessionRevoke
    };
    if let Err(s) = gates(&request, &state, op) {
        return refused(s);
    }
    let proof = match auth(&state)
        .cookies
        .verify_optional(one(request.headers(), "cookie"), None)
    {
        Ok(Some(p)) => p,
        _ => return denied(),
    };
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE);
    };
    let bytes = match body_bytes(request).await {
        Ok(b) => b,
        Err(s) => return refused(s),
    };
    // Serde struct decoding also accepts positional arrays; this wire contract requires an object.
    if !listing
        && !bytes.is_empty()
        && bytes.iter().find(|b| !b.is_ascii_whitespace()) != Some(&b'{')
    {
        return refused(StatusCode::BAD_REQUEST);
    }
    if listing {
        if !bytes.is_empty() {
            return refused(StatusCode::BAD_REQUEST);
        }
    } else if matches!(action, SessionAction::RevokeOne(_)) {
        let target: Target = match serde_json::from_slice(&bytes) {
            Ok(t) => t,
            Err(_) => return refused(StatusCode::BAD_REQUEST),
        };
        if target.session_id.trim().is_empty()
            || target.session_id.len() > 4096
            || target.session_id.chars().any(char::is_control)
        {
            return refused(StatusCode::BAD_REQUEST);
        }
        action = SessionAction::RevokeOne(target.session_id);
    } else if serde_json::from_slice::<Empty>(if bytes.is_empty() { b"{}" } else { &bytes })
        .is_err()
    {
        return refused(StatusCode::BAD_REQUEST);
    }
    let may_clear = matches!(
        action,
        SessionAction::RevokeOne(_) | SessionAction::RevokeAll
    );
    let must_clear = matches!(action, SessionAction::RevokeAll);
    let owner = state.clone();
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        (auth(&owner)
            .session_control
            .as_ref()
            .expect("registered sessions"))(SessionControlInput { proof, action })
    })
    .await;
    match result {
        Ok(Ok(outcome)) => {
            let clear = !outcome.clear_cookies.is_empty();
            if (clear
                && (!may_clear
                    || outcome.clear_cookies
                        != auth(&state).cookies.clear_native_session_cookies()))
                || (must_clear && !clear)
                || (listing && outcome.deleted != 0)
                || (!listing && !outcome.sessions.is_empty())
            {
                return uncertain();
            }
            if listing {
                match serde_json::to_string(&outcome.sessions) {
                    Ok(body) if body.len() <= 1_048_576 => {
                        let mut r = response(StatusCode::OK, "");
                        *r.body_mut() = axum::body::Body::from(body);
                        r
                    }
                    _ => uncertain(),
                }
            } else if clear {
                cookie_response(&state, StatusCode::OK, r#"{"status":true}"#, None)
            } else {
                response(StatusCode::OK, r#"{"status":true}"#)
            }
        }
        Ok(Err(e)) => native_failure(e),
        _ => uncertain(),
    }
}
