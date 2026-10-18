//! Native-only verification endpoints. No implicit opener, legacy proxy or writer grant.
use super::*;
use rentemester_runtime::recovery::VerificationResult;
pub struct VerificationRequest {
    email: Zeroizing<String>,
    callback: Option<Zeroizing<String>>,
    caller: Option<VerifiedCookie>,
}
impl VerificationRequest {
    pub fn email(&self) -> &str {
        &self.email
    }
    pub fn callback(&self) -> Option<&str> {
        self.callback.as_deref().map(String::as_str)
    }
    pub fn caller(&self) -> Option<&VerifiedCookie> {
        self.caller.as_ref()
    }
}
pub struct VerificationCompletion {
    token: Zeroizing<String>,
    callback: Option<Zeroizing<String>>,
}
impl VerificationCompletion {
    pub fn token(&self) -> &str {
        &self.token
    }
    pub fn callback(&self) -> Option<&str> {
        self.callback.as_deref().map(String::as_str)
    }
}
/// Inject only native persisted request/completion operations on admitted handles. The
/// serving owner must route these paths exclusively here and drain any legacy writer first.
pub struct VerificationHandlers {
    pub request: Box<
        dyn Fn(VerificationRequest) -> Result<(), rentemester_application::reset_request::Failure>
            + Send
            + Sync,
    >,
    pub complete:
        Box<dyn Fn(VerificationCompletion) -> Result<(), ContinuationFailure> + Send + Sync>,
}
pub(super) fn routes(router: Router<Arc<StateData>>) -> Router<Arc<StateData>> {
    // Handlers may be absent: requests then fail closed without dispatching legacy code.
    router
        .route("/api/auth/send-verification-email", post(request))
        .route(
            "/api/auth/verify-email",
            get(complete).head(|| async {
                let mut r = refused(StatusCode::METHOD_NOT_ALLOWED);
                r.headers_mut()
                    .insert("allow", HeaderValue::from_static("GET"));
                r
            }),
        )
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Wire {
    email: String,
    #[serde(
        default,
        rename = "callbackURL",
        deserialize_with = "optional_callback"
    )]
    callback: Option<String>,
}
fn optional_callback<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    String::deserialize(d).map(Some)
}
async fn request(State(state): State<Arc<StateData>>, request: Request) -> Response {
    if auth(&state).verification.is_none() {
        return refused(StatusCode::NOT_FOUND);
    }
    if let Err(status) = gates(&request, &state, Operation::VerificationRequest) {
        return refused(status);
    }
    let caller = match auth(&state)
        .cookies
        .verify_optional(one(request.headers(), "cookie"), None)
    {
        Ok(p) => p,
        Err(_) => return denied(),
    };
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE);
    };
    let bytes = match body_bytes(request).await {
        Ok(b) => b,
        Err(status) => return refused(status),
    };
    let wire: Wire = match serde_json::from_slice(&bytes) {
        Ok(w) => w,
        Err(_) => return refused(StatusCode::BAD_REQUEST),
    };
    let email = match rentemester_storage::password_credential::lookup_email(&wire.email) {
        Ok(e) => e,
        Err(_) => return refused(StatusCode::BAD_REQUEST),
    };
    if state
        .links
        .validate_redirect(wire.callback.as_deref())
        .is_err()
    {
        return refused(StatusCode::BAD_REQUEST);
    }
    let input = VerificationRequest {
        email: Zeroizing::new(email),
        callback: wire.callback.map(Zeroizing::new),
        caller,
    };
    let started = Instant::now();
    // The permit moves into the work: cancellation cannot admit more still-running writers.
    // Error/unknown commit/delivery/panic all receive the same admitted acknowledgement.
    let _ = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        (auth(&state)
            .verification
            .as_ref()
            .expect("registered verification")
            .request)(input)
    })
    .await;
    if let Some(remaining) = Duration::from_millis(500).checked_sub(started.elapsed()) {
        tokio::time::sleep(remaining).await;
    }
    // A floor plus identical body does not prove constant-time privacy for slow delivery.
    response(StatusCode::OK, r#"{"status":true}"#)
}
fn query(raw: Option<&str>) -> Result<(Zeroizing<String>, Option<Zeroizing<String>>), ()> {
    let raw = raw.filter(|s| s.len() <= 16384).ok_or(())?;
    let mut token = None;
    let mut callback = None;
    for part in raw.split('&') {
        let (name, value) = part.split_once('=').ok_or(())?;
        for (i, b) in value.as_bytes().iter().enumerate() {
            if *b == b'%'
                && !(value
                    .as_bytes()
                    .get(i + 1)
                    .is_some_and(u8::is_ascii_hexdigit)
                    && value
                        .as_bytes()
                        .get(i + 2)
                        .is_some_and(u8::is_ascii_hexdigit))
            {
                return Err(());
            }
        }
        match name {
            "token" if token.is_none() => {
                if value.is_empty()
                    || value.len() > 8192
                    || value.split('.').count() != 3
                    || value.split('.').any(str::is_empty)
                    || !value
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
                {
                    return Err(());
                }
                token = Some(Zeroizing::new(value.to_owned()));
            }
            "callbackURL" if callback.is_none() => {
                let value = value.replace('+', " ");
                let decoded = percent_encoding::percent_decode_str(&value)
                    .decode_utf8()
                    .map_err(|_| ())?;
                callback = Some(Zeroizing::new(decoded.into_owned()));
            }
            _ => return Err(()),
        }
    }
    Ok((token.ok_or(())?, callback))
}
fn result(state: &StateData, callback: Option<&str>, outcome: VerificationResult) -> Response {
    match state.links.verification_callback(callback, outcome) {
        Ok(Some(url)) => {
            let Ok(mut location) = HeaderValue::from_str(url.as_str()) else {
                return uncertain();
            };
            location.set_sensitive(true);
            let mut r = response(StatusCode::FOUND, "");
            r.headers_mut().insert("location", location);
            r
        }
        Ok(None) => match outcome {
            VerificationResult::Accepted => {
                response(StatusCode::OK, r#"{"status":true,"user":null}"#)
            }
            VerificationResult::InvalidToken => {
                response(StatusCode::UNAUTHORIZED, r#"{"code":"INVALID_TOKEN"}"#)
            }
            VerificationResult::Unconfirmed => response(
                StatusCode::SERVICE_UNAVAILABLE,
                r#"{"code":"VERIFICATION_UNCONFIRMED"}"#,
            ),
        },
        Err(_) => refused(StatusCode::BAD_REQUEST),
    }
}
async fn complete(State(state): State<Arc<StateData>>, request: Request) -> Response {
    if auth(&state).verification.is_none() {
        return refused(StatusCode::NOT_FOUND);
    }
    if let Err(status) = admission_for(&request, &state, Operation::VerifyEmail) {
        return refused(status);
    }
    if request.headers().contains_key("authorization")
        || request.headers().contains_key("x-rentemester-service-key")
    {
        return denied();
    }
    let (token, callback) = match query(request.uri().query()) {
        Ok(q) => q,
        Err(_) => return refused(StatusCode::BAD_REQUEST),
    };
    // All callback processing that could refuse must run before any email state mutation.
    if state
        .links
        .verification_callback(
            callback.as_deref().map(String::as_str),
            VerificationResult::Accepted,
        )
        .is_err()
    {
        return refused(StatusCode::BAD_REQUEST);
    }
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE);
    };
    let input = VerificationCompletion {
        token,
        callback: callback.as_ref().map(|v| Zeroizing::new(v.to_string())),
    };
    let work = state.clone();
    let completion = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        (auth(&work)
            .verification
            .as_ref()
            .expect("registered verification")
            .complete)(input)
    })
    .await;
    let outcome = match completion {
        Ok(Ok(())) => VerificationResult::Accepted,
        Ok(Err(e))
            if e.stage == FailureStage::NotCommitted && e.error.code == "invalid_credentials" =>
        {
            VerificationResult::InvalidToken
        }
        _ => VerificationResult::Unconfirmed,
    };
    result(&state, callback.as_deref().map(String::as_str), outcome)
}
