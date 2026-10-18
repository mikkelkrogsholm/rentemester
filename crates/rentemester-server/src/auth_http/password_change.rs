//! Current-password self change; request cannot select a subject or write a session token.
use super::*;
use rentemester_application::password_change::Outcome;
pub type PasswordChangeHandler =
    Box<dyn Fn(PasswordChangeInput) -> Result<Outcome, ContinuationFailure> + Send + Sync>;
pub struct PasswordChangeInput {
    proof: VerifiedCookie,
    current_password: Zeroizing<String>,
    new_password: Zeroizing<String>,
    revoke_other_sessions: bool,
}
impl PasswordChangeInput {
    pub fn proof(&self) -> &VerifiedCookie {
        &self.proof
    }
    pub fn current_password(&self) -> &str {
        &self.current_password
    }
    pub fn new_password(&self) -> &str {
        &self.new_password
    }
    pub fn revoke_other_sessions(&self) -> bool {
        self.revoke_other_sessions
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Wire {
    current_password: String,
    new_password: String,
    #[serde(default, deserialize_with = "boolean")]
    revoke_other_sessions: bool,
}
pub(super) fn routes(router: Router<Arc<StateData>>) -> Router<Arc<StateData>> {
    router.route("/api/auth/change-password", post(change))
}
async fn change(State(state): State<Arc<StateData>>, request: Request) -> Response {
    if auth(&state).password_change.is_none() {
        return refused(StatusCode::NOT_FOUND);
    }
    if let Err(s) = gates(&request, &state, Operation::PasswordChange) {
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
    if bytes.iter().find(|b| !b.is_ascii_whitespace()) != Some(&b'{') {
        return refused(StatusCode::BAD_REQUEST);
    }
    let wire: Wire = match serde_json::from_slice(&bytes) {
        Ok(w) => w,
        Err(_) => return refused(StatusCode::BAD_REQUEST),
    };
    if wire.current_password.len() > rentemester_runtime::password::INPUT_BYTE_LIMIT {
        return refused(StatusCode::BAD_REQUEST);
    }
    let length = wire.new_password.encode_utf16().count();
    if length < rentemester_application::password_reset::MIN_PASSWORD_UTF16 {
        return response(
            StatusCode::BAD_REQUEST,
            r#"{"message":"Password too short","code":"PASSWORD_TOO_SHORT"}"#,
        );
    }
    if length > rentemester_application::password_reset::MAX_PASSWORD_UTF16 {
        return response(
            StatusCode::BAD_REQUEST,
            r#"{"message":"Password too long","code":"PASSWORD_TOO_LONG"}"#,
        );
    }
    let rotate = wire.revoke_other_sessions;
    let input = PasswordChangeInput {
        proof,
        current_password: Zeroizing::new(wire.current_password),
        new_password: Zeroizing::new(wire.new_password),
        revoke_other_sessions: rotate,
    };
    let owner = state.clone();
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        (auth(&owner)
            .password_change
            .as_ref()
            .expect("registered password change"))(input)
    })
    .await;
    match result {
        Ok(Ok(outcome)) => {
            if outcome.cookie.is_some() != rotate || outcome.user_json.len() > 65_536 {
                return uncertain();
            }
            let user: serde_json::Value =
                match serde_json::from_str::<serde_json::Value>(&outcome.user_json) {
                    Ok(v) if v.is_object() => v,
                    _ => return uncertain(),
                };
            let body = Zeroizing::new(
                serde_json::json!({"status":true,"token":null,"user":user}).to_string(),
            );
            if let Some(cookie) = outcome.cookie {
                if auth(&state)
                    .cookies
                    .verify(Some(cookie.request_header()), None)
                    .is_err()
                {
                    return uncertain();
                }
                cookie_response(&state, StatusCode::OK, &body, Some(&cookie))
            } else {
                let mut r = response(StatusCode::OK, "");
                *r.body_mut() = axum::body::Body::from(body.to_string());
                r
            }
        }
        Ok(Err(e)) => native_failure(e),
        _ => uncertain(),
    }
}
