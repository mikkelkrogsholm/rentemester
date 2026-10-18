//! Explicit password-bound setup; never adopts a first-factor session or enrolled login challenge.
use super::*;
use rentemester_application::mfa_enrollment::{ConfirmOutcome, Setup};
pub struct EnrollmentHandlers {
    pub begin: Box<dyn Fn(LoginInput) -> Result<Setup, ContinuationFailure> + Send + Sync>,
    pub confirm:
        Box<dyn Fn(FactorInput) -> Result<ConfirmOutcome, ContinuationFailure> + Send + Sync>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BeginWire {
    email: String,
    password: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfirmWire {
    code: String,
}
pub(super) fn routes(router: Router<Arc<StateData>>) -> Router<Arc<StateData>> {
    router
        .route("/api/auth/mfa-enrollment/begin", post(begin))
        .route("/api/auth/mfa-enrollment/confirm", post(confirm))
}
fn handlers(state: &StateData) -> &EnrollmentHandlers {
    auth(state)
        .enrollment
        .as_ref()
        .expect("registered enrollment")
}
async fn begin(State(state): State<Arc<StateData>>, request: Request) -> Response {
    if let Err(s) = gates(&request, &state, Operation::EnrollmentBegin) {
        return refused(s);
    }
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE);
    };
    let bytes = match body_bytes(request).await {
        Ok(b) => b,
        Err(s) => return refused(s),
    };
    let wire: BeginWire = match serde_json::from_slice(&bytes) {
        Ok(w) => w,
        Err(_) => return refused(StatusCode::BAD_REQUEST),
    };
    if wire.password.len() > 4096 {
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
        (handlers(&owner).begin)(input)
    })
    .await;
    match result {
        Ok(Ok(setup)) => {
            if auth(&state)
                .cookies
                .verify_challenge(Some(setup.cookie.request_header()), None)
                .is_err()
            {
                return uncertain();
            }
            // URI is disclosed only after the native setup transaction is known committed.
            let body = Zeroizing::new(
                serde_json::json!({"method":"totp","totpURI":setup.uri.as_str()}).to_string(),
            );
            cookie_response(&state, StatusCode::OK, &body, Some(&setup.cookie))
        }
        Ok(Err(e)) => native_failure(e),
        _ => uncertain(),
    }
}
async fn confirm(State(state): State<Arc<StateData>>, request: Request) -> Response {
    if let Err(s) = gates(&request, &state, Operation::EnrollmentConfirm) {
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
        Ok(b) => b,
        Err(s) => return refused(s),
    };
    let wire: ConfirmWire = match serde_json::from_slice(&bytes) {
        Ok(w) => w,
        Err(_) => return refused(StatusCode::BAD_REQUEST),
    };
    if wire.code.len() > 4096 {
        return refused(StatusCode::BAD_REQUEST);
    }
    let input = FactorInput {
        challenge: proof,
        code: Zeroizing::new(wire.code),
    };
    let owner = state.clone();
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        (handlers(&owner).confirm)(input)
    })
    .await;
    match result {
        Ok(Ok(ConfirmOutcome::Completed { cookie, codes })) => {
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
            if codes.len() != 10 {
                return uncertain();
            }
            let codes: Vec<&str> = codes.iter().collect();
            let body =
                Zeroizing::new(serde_json::json!({"status":true,"backupCodes":codes}).to_string());
            cookie_response(&state, StatusCode::OK, &body, Some(&cookie))
        }
        Ok(Ok(ConfirmOutcome::Rejected(Rejection::AccountLocked))) => response(
            StatusCode::TOO_MANY_REQUESTS,
            r#"{"message":"Authentication refused","code":"AUTHENTICATION_LOCKED"}"#,
        ),
        Ok(Ok(ConfirmOutcome::Rejected(_))) => response(
            StatusCode::UNAUTHORIZED,
            r#"{"message":"Invalid code","code":"INVALID_CODE"}"#,
        ),
        Ok(Err(e)) => native_failure(e),
        _ => uncertain(),
    }
}
