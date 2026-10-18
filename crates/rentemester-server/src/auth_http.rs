//! Human-only enrolled login. Trusted handlers alone own admitted native connections/lifetimes.
use super::*;
mod context;
pub use context::WorkspaceContextHandler;
mod backup_management;
pub use backup_management::{BackupManagementHandler, BackupManagementInput};
mod password_change;
pub use password_change::{PasswordChangeHandler, PasswordChangeInput};
mod enrollment;
mod sessions;
pub use sessions::{SessionAction, SessionControlInput};
mod verification;
pub use enrollment::EnrollmentHandlers;
use rentemester_application::{
    mfa_login::{BeginOutcome, Rejection, VerifyOutcome},
    password_login::FailureStage,
    session_control::Outcome,
    session_read::SessionView,
};
use rentemester_runtime::auth::{CookieVerifier, IssuedCookie, VerifiedChallenge, VerifiedCookie};
pub use verification::{VerificationCompletion, VerificationHandlers, VerificationRequest};
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct LoginWire {
    email: String,
    password: String,
    #[serde(default, deserialize_with = "boolean")]
    remember_me: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct FactorWire {
    code: String,
    #[serde(default, deserialize_with = "boolean")]
    trust_device: bool,
    #[serde(default, deserialize_with = "boolean")]
    disable_session: bool,
}
fn boolean<'de, D: serde::Deserializer<'de>>(d: D) -> Result<bool, D::Error> {
    bool::deserialize(d)
}
pub struct LoginInput {
    email: Zeroizing<String>,
    password: Zeroizing<String>,
}
impl LoginInput {
    pub fn email(&self) -> &str {
        &self.email
    }
    pub fn password(&self) -> &str {
        &self.password
    }
}
pub struct FactorInput {
    challenge: VerifiedChallenge,
    code: Zeroizing<String>,
}
impl FactorInput {
    pub fn challenge(&self) -> &VerifiedChallenge {
        &self.challenge
    }
    pub fn code(&self) -> &str {
        &self.code
    }
}
/// All outcomes must come from known native transaction results. No handler is supplied by HTTP.
pub type SessionControlHandler =
    Box<dyn Fn(SessionControlInput) -> Result<Outcome, ContinuationFailure> + Send + Sync>;
pub struct AuthHandlers {
    pub cookies: Arc<CookieVerifier>,
    pub enrollment: Option<EnrollmentHandlers>,
    pub verification: Option<VerificationHandlers>,
    pub backup_management: Option<BackupManagementHandler>,
    pub password_change: Option<PasswordChangeHandler>,
    pub workspace_context: Option<WorkspaceContextHandler>,
    pub session_control: Option<SessionControlHandler>,
    pub sign_in: Box<dyn Fn(LoginInput) -> Result<BeginOutcome, ContinuationFailure> + Send + Sync>,
    pub totp: Box<dyn Fn(FactorInput) -> Result<VerifyOutcome, ContinuationFailure> + Send + Sync>,
    pub backup:
        Box<dyn Fn(FactorInput) -> Result<VerifyOutcome, ContinuationFailure> + Send + Sync>,
    pub session:
        Box<dyn Fn(VerifiedCookie) -> Result<SessionView, ContinuationFailure> + Send + Sync>,
    pub sign_out: Box<dyn Fn(VerifiedCookie) -> Result<Outcome, ContinuationFailure> + Send + Sync>,
}
pub(super) fn routes(router: Router<Arc<StateData>>, enrollment: bool) -> Router<Arc<StateData>> {
    let router = router
        .route("/api/auth/sign-in/email", post(sign_in))
        .route("/api/auth/two-factor/verify-totp", post(totp))
        .route("/api/auth/two-factor/verify-backup-code", post(backup))
        .route(
            "/api/auth/get-session",
            get(session).head(|| async {
                let mut r = refused(StatusCode::METHOD_NOT_ALLOWED);
                r.headers_mut()
                    .insert("allow", HeaderValue::from_static("GET"));
                r
            }),
        )
        .route("/api/auth/sign-out", post(sign_out));
    let router = if enrollment {
        self::enrollment::routes(router)
    } else {
        router
    };
    let router = self::context::routes(self::password_change::routes(router));
    // Each route owns the canonical path; there is no legacy auth-handler fallback.
    self::verification::routes(self::sessions::routes(self::backup_management::routes(
        router,
    )))
}
fn denied() -> Response {
    response(
        StatusCode::UNAUTHORIZED,
        r#"{"message":"Invalid credentials","code":"INVALID_CREDENTIALS"}"#,
    )
}
fn uncertain() -> Response {
    response(
        StatusCode::SERVICE_UNAVAILABLE,
        r#"{"message":"Authentication outcome could not be confirmed","code":"AUTH_UNCONFIRMED"}"#,
    )
}
fn native_failure(e: ContinuationFailure) -> Response {
    if e.stage == FailureStage::NotCommitted && e.error.code == "invalid_credentials" {
        denied()
    } else {
        uncertain()
    }
}
fn gates(request: &Request, state: &StateData, op: Operation) -> Result<(), StatusCode> {
    if request.uri().query().is_some() {
        return Err(StatusCode::BAD_REQUEST);
    }
    admission_for(request, state, op)?;
    let h = request.headers();
    if h.contains_key("authorization") || h.contains_key("x-rentemester-service-key") {
        return Err(StatusCode::UNAUTHORIZED);
    }
    if h.contains_key("cookie") && one(h, "cookie").is_none_or(|s| s.len() > 8192) {
        return Err(StatusCode::BAD_REQUEST);
    }
    Ok(())
}

#[cfg(test)]
mod tests;
fn auth(state: &StateData) -> &AuthHandlers {
    state.auth.as_ref().expect("registered identity")
}
fn cookie_response(
    state: &StateData,
    status: StatusCode,
    body: &str,
    new: Option<&IssuedCookie>,
) -> Response {
    let clear = auth(state).cookies.clear_native_session_cookies();
    let mut headers = Vec::new();
    for value in clear
        .iter()
        .map(String::as_str)
        .chain(new.map(IssuedCookie::set_cookie))
    {
        let Ok(mut value) = HeaderValue::from_str(value) else {
            return uncertain();
        };
        value.set_sensitive(true);
        headers.push(value);
    }
    let mut r = response(status, "");
    *r.body_mut() = axum::body::Body::from(body.to_owned());
    for value in headers {
        r.headers_mut().append("set-cookie", value);
    }
    r
}
async fn sign_in(State(state): State<Arc<StateData>>, request: Request) -> Response {
    if let Err(s) = gates(&request, &state, Operation::SignIn) {
        return refused(s);
    }
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE);
    };
    let bytes = match body_bytes(request).await {
        Ok(v) => v,
        Err(s) => return refused(s),
    };
    let wire: LoginWire = match serde_json::from_slice(&bytes) {
        Ok(w) => w,
        Err(_) => return refused(StatusCode::BAD_REQUEST),
    };
    if wire.remember_me || wire.password.len() > 4096 {
        return refused(StatusCode::BAD_REQUEST);
    }
    let email = match rentemester_storage::password_credential::lookup_email(&wire.email) {
        Ok(e) => e,
        Err(_) => return denied(),
    };
    let input = LoginInput {
        email: Zeroizing::new(email),
        password: Zeroizing::new(wire.password),
    };
    let owner = state.clone();
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        (auth(&owner).sign_in)(input)
    })
    .await;
    match result {
        Ok(Ok(BeginOutcome::Challenge(cookie))) => {
            if auth(&state)
                .cookies
                .verify_challenge(Some(cookie.request_header()), None)
                .is_err()
            {
                return uncertain();
            }
            cookie_response(
                &state,
                StatusCode::OK,
                r#"{"twoFactorRedirect":true,"twoFactorMethods":["totp"]}"#,
                Some(&cookie),
            )
        }
        Ok(Ok(BeginOutcome::EnrollmentRequired)) => cookie_response(
            &state,
            StatusCode::CONFLICT,
            r#"{"message":"MFA enrollment required","code":"MFA_ENROLLMENT_REQUIRED"}"#,
            None,
        ),
        Ok(Err(e)) => native_failure(e),
        _ => uncertain(),
    }
}
async fn totp(state: State<Arc<StateData>>, r: Request) -> Response {
    factor(state, r, Operation::Totp).await
}
async fn backup(state: State<Arc<StateData>>, r: Request) -> Response {
    factor(state, r, Operation::Backup).await
}
async fn factor(State(state): State<Arc<StateData>>, request: Request, op: Operation) -> Response {
    if let Err(s) = gates(&request, &state, op) {
        return refused(s);
    }
    let proof = match auth(&state)
        .cookies
        .verify_challenge(one(request.headers(), "cookie"), None)
    {
        Ok(p) => p,
        Err(_) => return denied(),
    };
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE);
    };
    let bytes = match body_bytes(request).await {
        Ok(v) => v,
        Err(s) => return refused(s),
    };
    let wire: FactorWire = match serde_json::from_slice(&bytes) {
        Ok(w) => w,
        Err(_) => return refused(StatusCode::BAD_REQUEST),
    };
    if wire.trust_device || wire.disable_session || wire.code.len() > 4096 {
        return refused(StatusCode::BAD_REQUEST);
    }
    let input = FactorInput {
        challenge: proof,
        code: Zeroizing::new(wire.code),
    };
    let owner = state.clone();
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        if op == Operation::Totp {
            (auth(&owner).totp)(input)
        } else {
            (auth(&owner).backup)(input)
        }
    })
    .await;
    match result {
        Ok(Ok(VerifyOutcome::Session(cookie))) => {
            let proof = match auth(&state)
                .cookies
                .verify(Some(cookie.request_header()), None)
            {
                Ok(p) => p,
                Err(_) => return uncertain(),
            };
            let cookie = match auth(&state).cookies.issue_browser_session(proof.token()) {
                Ok(c) => c,
                Err(_) => return uncertain(),
            };
            cookie_response(&state, StatusCode::OK, r#"{"status":true}"#, Some(&cookie))
        }
        Ok(Ok(VerifyOutcome::Rejected(Rejection::AccountLocked))) => response(
            StatusCode::TOO_MANY_REQUESTS,
            r#"{"message":"Authentication refused","code":"AUTHENTICATION_LOCKED"}"#,
        ),
        Ok(Ok(VerifyOutcome::Rejected(_))) => {
            if op == Operation::Totp {
                response(
                    StatusCode::UNAUTHORIZED,
                    r#"{"message":"Invalid code","code":"INVALID_CODE"}"#,
                )
            } else {
                response(
                    StatusCode::UNAUTHORIZED,
                    r#"{"message":"Invalid backup code","code":"INVALID_BACKUP_CODE"}"#,
                )
            }
        }
        Ok(Err(e)) => native_failure(e),
        _ => uncertain(),
    }
}
async fn session(State(state): State<Arc<StateData>>, request: Request) -> Response {
    if let Err(s) = gates(&request, &state, Operation::Session) {
        return refused(s);
    }
    let proof = match auth(&state)
        .cookies
        .verify(one(request.headers(), "cookie"), None)
    {
        Ok(p) => p,
        Err(_) => return response(StatusCode::OK, "null"),
    };
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE);
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        (auth(&state).session)(proof)
    })
    .await;
    match result {
        Ok(Ok(view)) => match serde_json::to_string(&view) {
            Ok(body) => {
                let mut r = response(StatusCode::OK, "");
                *r.body_mut() = axum::body::Body::from(body);
                r
            }
            Err(_) => uncertain(),
        },
        Ok(Err(e))
            if e.stage == FailureStage::NotCommitted && e.error.code == "invalid_credentials" =>
        {
            response(StatusCode::OK, "null")
        }
        _ => uncertain(),
    }
}
async fn sign_out(State(state): State<Arc<StateData>>, request: Request) -> Response {
    if let Err(s) = gates(&request, &state, Operation::SignOut) {
        return refused(s);
    }
    let proof = auth(&state)
        .cookies
        .verify(one(request.headers(), "cookie"), None)
        .ok();
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE);
    };
    let bytes = match body_bytes(request).await {
        Ok(v) => v,
        Err(s) => return refused(s),
    };
    let empty: std::collections::BTreeMap<String, serde_json::Value> =
        match serde_json::from_slice(if bytes.is_empty() { b"{}" } else { &bytes }) {
            Ok(w) => w,
            Err(_) => return refused(StatusCode::BAD_REQUEST),
        };
    if !empty.is_empty() {
        return refused(StatusCode::BAD_REQUEST);
    }
    let Some(proof) = proof else {
        return cookie_response(&state, StatusCode::OK, r#"{"success":true}"#, None);
    };
    let owner = state.clone();
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        (auth(&owner).sign_out)(proof)
    })
    .await;
    match result {
        Ok(Ok(_)) => cookie_response(&state, StatusCode::OK, r#"{"success":true}"#, None),
        Ok(Err(e)) => native_failure(e),
        _ => uncertain(),
    }
}
