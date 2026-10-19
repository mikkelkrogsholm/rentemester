//! HTTP boundary. The owner supplies an admitted executor; no workspace paths or writer grants.
pub mod auth_http;
use axum::{
    Router,
    body::to_bytes,
    extract::{ConnectInfo, Request, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use rentemester_application::reset_request::Failure;
use rentemester_runtime::{
    auth::CookieProfile,
    recovery::{RecoveryLinks, SensitiveUrl},
};
use serde::Deserialize;
use std::{
    collections::HashMap,
    net::{IpAddr, SocketAddr},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::sync::Semaphore;
use url::Url;
use zeroize::Zeroizing;

const PATH: &str = "/api/auth/request-password-reset";
const COMPLETE: &str = "/api/auth/reset-password";
const ACK: &str = r#"{"status":true,"message":"If this email exists in our system, check your email for the reset link"}"#;
const WINDOW: Duration = Duration::from_secs(60);
const MAX_KEYS: usize = 4096;
const BODY_LIMIT: usize = 8192;

/// Transient sensitive input. This is not an authenticated principal or a writer grant.
pub struct ResetRequest {
    email: Zeroizing<String>,
    redirect: Option<Zeroizing<String>>,
}
impl ResetRequest {
    pub fn email(&self) -> &str {
        &self.email
    }
    pub fn redirect(&self) -> Option<&str> {
        self.redirect.as_deref().map(String::as_str)
    }
}

/// Sensitive bearer and callback destination; neither constitutes a reusable admission grant.
pub struct ResetCallback {
    token: Zeroizing<String>,
    redirect: Zeroizing<String>,
}
impl ResetCallback {
    pub fn token(&self) -> &str {
        &self.token
    }
    pub fn redirect(&self) -> &str {
        &self.redirect
    }
}
pub struct ResetCompletion {
    token: Zeroizing<String>,
    password: Zeroizing<String>,
}
impl ResetCompletion {
    pub fn token(&self) -> &str {
        &self.token
    }
    pub fn password(&self) -> &str {
        &self.password
    }
}
type ContinuationFailure = rentemester_application::password_reset::Failure;
struct RecoveryHandlers {
    callback: Box<dyn Fn(ResetCallback) -> Result<SensitiveUrl, ContinuationFailure> + Send + Sync>,
    complete: Box<dyn Fn(ResetCompletion) -> Result<(), ContinuationFailure> + Send + Sync>,
}
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Operation {
    Request,
    Callback,
    Complete,
    SignIn,
    Totp,
    Backup,
    Session,
    SignOut,
    SessionList,
    SessionRevoke,
    BackupManagement,
    PasswordChange,
    WorkspaceContext,
    ServiceLifecycle,
    ServiceLifecycleList,
    EnrollmentBegin,
    EnrollmentConfirm,
    VerificationRequest,
    VerifyEmail,
}
impl Operation {
    fn rate(self) -> (Duration, u8) {
        match self {
            Self::SignIn
            | Self::Totp
            | Self::Backup
            | Self::EnrollmentBegin
            | Self::EnrollmentConfirm
            | Self::BackupManagement
            | Self::PasswordChange => (Duration::from_secs(10), 3),
            Self::Request | Self::VerificationRequest => (WINDOW, 3),
            // A browser refresh reads both session and workspace context. Keep
            // ordinary navigation usable without changing credential budgets.
            Self::Session | Self::WorkspaceContext => (WINDOW, 120),
            _ => (WINDOW, 10),
        }
    }
}

type RequestHandler = Box<dyn Fn(ResetRequest) -> Result<(), Failure> + Send + Sync>;
struct StateData {
    origin: String,
    authority: String,
    links: RecoveryLinks,
    rate: Mutex<HashMap<(IpAddr, Operation), (Instant, u8)>>,
    recovery: Option<RecoveryHandlers>,
    auth: Option<auth_http::AuthHandlers>,
    workers: Arc<Semaphore>,
    execute: Option<RequestHandler>,
}

#[derive(Debug)]
pub struct InvalidConfig;

/// Same-origin JSON browser/native boundary. The trusted owner must retain physical/workspace
/// lifetime admission, retire unusable connections, and call reset_request::execute. No default
/// executor opens data or grants mutation authority. Construction happens outside async work.
/// All executor outcomes (including unknown commit/delivery and panics) receive the same ACK.
pub fn reset_router(
    origin: &str,
    profile: CookieProfile,
    execute: impl Fn(ResetRequest) -> Result<(), Failure> + Send + Sync + 'static,
) -> Result<Router, InvalidConfig> {
    build_router(origin, profile, Some(Box::new(execute)), None, None)
}
/// Complete recovery HTTP surface. All three trusted handlers share rate-map/capacity admission.
/// Native callback must be read-only; native completion must retain its known/unknown commit result.
pub fn recovery_router(
    origin: &str,
    profile: CookieProfile,
    request: impl Fn(ResetRequest) -> Result<(), Failure> + Send + Sync + 'static,
    callback: impl Fn(ResetCallback) -> Result<SensitiveUrl, ContinuationFailure>
    + Send
    + Sync
    + 'static,
    complete: impl Fn(ResetCompletion) -> Result<(), ContinuationFailure> + Send + Sync + 'static,
) -> Result<Router, InvalidConfig> {
    build_router(
        origin,
        profile,
        Some(Box::new(request)),
        Some(RecoveryHandlers {
            callback: Box::new(callback),
            complete: Box::new(complete),
        }),
        None,
    )
}
/// Enrolled human login/session routes only; no implicit recovery or product data opener.
pub fn identity_router(
    origin: &str,
    profile: CookieProfile,
    auth: auth_http::AuthHandlers,
) -> Result<Router, InvalidConfig> {
    build_router(origin, profile, None, None, Some(auth))
}
/// Current identity and recovery share one capacity/rate state in a combined runtime.
pub fn recovery_identity_router(
    origin: &str,
    profile: CookieProfile,
    request: impl Fn(ResetRequest) -> Result<(), Failure> + Send + Sync + 'static,
    callback: impl Fn(ResetCallback) -> Result<SensitiveUrl, ContinuationFailure>
    + Send
    + Sync
    + 'static,
    complete: impl Fn(ResetCompletion) -> Result<(), ContinuationFailure> + Send + Sync + 'static,
    auth: auth_http::AuthHandlers,
) -> Result<Router, InvalidConfig> {
    build_router(
        origin,
        profile,
        Some(Box::new(request)),
        Some(RecoveryHandlers {
            callback: Box::new(callback),
            complete: Box::new(complete),
        }),
        Some(auth),
    )
}
fn build_router(
    origin: &str,
    profile: CookieProfile,
    execute: Option<RequestHandler>,
    recovery: Option<RecoveryHandlers>,
    auth: Option<auth_http::AuthHandlers>,
) -> Result<Router, InvalidConfig> {
    if auth
        .as_ref()
        .is_some_and(|a| a.cookies.profile() != profile)
    {
        return Err(InvalidConfig);
    }
    let links = RecoveryLinks::new(origin, &[origin], profile).map_err(|_| InvalidConfig)?;
    let parsed = Url::parse(origin).map_err(|_| InvalidConfig)?;
    let origin = parsed.origin().ascii_serialization();
    let authority = origin.split_once("://").ok_or(InvalidConfig)?.1.to_owned();
    let state = Arc::new(StateData {
        origin,
        authority,
        links,
        rate: Mutex::new(HashMap::new()),
        recovery,
        auth,
        workers: Arc::new(Semaphore::new(4)),
        execute,
    });
    let mut router = Router::new();
    if state.execute.is_some() {
        router = router.route(PATH, post(request_reset));
    }
    if state.recovery.is_some() {
        router = router.route(COMPLETE, post(complete_reset)).route(
            "/api/auth/reset-password/{token}",
            get(callback_reset).head(|| async {
                let mut r = refused(StatusCode::METHOD_NOT_ALLOWED);
                r.headers_mut()
                    .insert("allow", HeaderValue::from_static("GET"));
                r
            }),
        );
    }
    if state.auth.is_some() {
        let enrollment = state.auth.as_ref().is_some_and(|a| a.enrollment.is_some());
        router = auth_http::routes(router, enrollment);
    }
    Ok(router.with_state(state))
}

fn response(status: StatusCode, body: &'static str) -> Response {
    let mut r = (status, body).into_response();
    let h = r.headers_mut();
    h.insert("content-type", HeaderValue::from_static("application/json"));
    h.insert("cache-control", HeaderValue::from_static("no-store"));
    h.insert("referrer-policy", HeaderValue::from_static("no-referrer"));
    h.insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    r
}
fn refused(status: StatusCode) -> Response {
    let mut r = response(status, r#"{"code":"request_refused"}"#);
    if status == StatusCode::TOO_MANY_REQUESTS {
        r.headers_mut()
            .insert("retry-after", HeaderValue::from_static("60"));
    }
    r
}
fn one<'a>(headers: &'a HeaderMap, key: &str) -> Option<&'a str> {
    let mut values = headers.get_all(key).iter();
    let value = values.next()?.to_str().ok()?;
    if values.next().is_some() {
        return None;
    }
    Some(value)
}
fn admission(headers: &HeaderMap, state: &StateData) -> bool {
    one(headers, "origin") == Some(state.origin.as_str())
        && one(headers, "host") == Some(state.authority.as_str())
        && (!headers.contains_key("sec-fetch-site")
            || matches!(
                one(headers, "sec-fetch-site"),
                Some("same-origin" | "same-site" | "none")
            ))
}
fn json_content(headers: &HeaderMap) -> bool {
    !headers.contains_key("content-encoding")
        && one(headers, "content-type").is_some_and(|s| {
            let mut p = s.split(';').map(str::trim);
            p.next()
                .is_some_and(|s| s.eq_ignore_ascii_case("application/json"))
                && p.all(|s| s.eq_ignore_ascii_case("charset=utf-8"))
        })
}
fn canonical_ip(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V6(v) => v.to_ipv4_mapped().map_or_else(
            || IpAddr::V6(std::net::Ipv6Addr::from(u128::from(v) & (u128::MAX << 64))),
            IpAddr::V4,
        ),
        _ => ip,
    }
}
impl StateData {
    #[cfg(test)]
    fn allow(&self, peer: IpAddr) -> bool {
        self.allow_route_at(peer, Operation::Request, Instant::now())
    }
    #[cfg(test)]
    fn allow_at(&self, peer: IpAddr, now: Instant) -> bool {
        self.allow_route_at(peer, Operation::Request, now)
    }
    fn allow_route_at(&self, peer: IpAddr, operation: Operation, now: Instant) -> bool {
        let operation = if operation == Operation::ServiceLifecycleList {
            Operation::ServiceLifecycle
        } else {
            operation
        };
        let Ok(mut keys) = self.rate.lock() else {
            return false;
        };
        keys.retain(|(_, op), (start, _)| now.duration_since(*start) < op.rate().0);
        let peer = (canonical_ip(peer), operation);
        if !keys.contains_key(&peer) && keys.len() >= MAX_KEYS {
            return false;
        }
        let (last, count) = keys.entry(peer).or_insert((now, 0));
        if *count >= operation.rate().1 {
            return false;
        }
        *count += 1;
        // Read-only browser navigation has a bounded per-minute window, not
        // a lifetime allowance that requires a full minute idle to recover.
        // Credential and sensitive-operation cooldowns keep their existing semantics.
        if !matches!(operation, Operation::Session | Operation::WorkspaceContext) {
            *last = (*last).max(now);
        }
        true
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Wire {
    email: String,
    #[serde(default, deserialize_with = "present_redirect")]
    redirect_to: Option<String>,
}
fn present_redirect<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    String::deserialize(d).map(Some)
}
async fn request_reset(State(state): State<Arc<StateData>>, request: Request) -> Response {
    if request.uri().query().is_some() || !admission(request.headers(), &state) {
        return refused(StatusCode::FORBIDDEN);
    }
    if let Err(r) = admission_for(&request, &state, Operation::Request) {
        return refused(r);
    }
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE);
    };
    let bytes = match body_bytes(request).await {
        Ok(b) => b,
        Err(status) => return refused(status),
    };
    let wire: Wire = match serde_json::from_slice(&bytes) {
        Ok(wire) => wire,
        Err(_) => return refused(StatusCode::BAD_REQUEST),
    };
    let email = match rentemester_storage::password_credential::lookup_email(&wire.email) {
        Ok(email) => Zeroizing::new(email),
        Err(_) => return refused(StatusCode::BAD_REQUEST),
    };
    if state
        .links
        .validate_redirect(wire.redirect_to.as_deref())
        .is_err()
    {
        return refused(StatusCode::BAD_REQUEST);
    }
    let input = ResetRequest {
        email,
        redirect: wire.redirect_to.map(Zeroizing::new),
    };
    // The permit lives in the blocking work, even if HTTP cancellation drops its JoinHandle.
    // No request timeout may claim that admitted side effects were canceled or rolled back.
    let _ = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        (state.execute.as_ref().expect("registered request"))(input)
    })
    .await;
    response(StatusCode::OK, ACK)
}

#[cfg(test)]
mod tests;

fn admission_for(
    request: &Request,
    state: &StateData,
    operation: Operation,
) -> Result<(), StatusCode> {
    let headers = request.headers();
    let allowed = if matches!(
        operation,
        Operation::Callback
            | Operation::VerifyEmail
            | Operation::Session
            | Operation::SessionList
            | Operation::WorkspaceContext
            | Operation::ServiceLifecycleList
    ) {
        one(headers, "host") == Some(state.authority.as_str())
            && (!headers.contains_key("origin")
                || one(headers, "origin") == Some(state.origin.as_str()))
            && (matches!(operation, Operation::Callback | Operation::VerifyEmail)
                || !headers.contains_key("sec-fetch-site")
                || matches!(
                    one(headers, "sec-fetch-site"),
                    Some("same-origin" | "same-site" | "none")
                ))
    } else {
        admission(headers, state)
    };
    if !allowed {
        return Err(StatusCode::FORBIDDEN);
    }
    if operation == Operation::SignOut && !headers.contains_key("content-type") {
        if headers.contains_key("content-encoding") {
            return Err(StatusCode::UNSUPPORTED_MEDIA_TYPE);
        }
    } else if !matches!(
        operation,
        Operation::Callback
            | Operation::VerifyEmail
            | Operation::Session
            | Operation::SessionList
            | Operation::WorkspaceContext
            | Operation::ServiceLifecycleList
    ) && !json_content(headers)
    {
        return Err(StatusCode::UNSUPPORTED_MEDIA_TYPE);
    }
    let Some(peer) = request.extensions().get::<ConnectInfo<SocketAddr>>() else {
        return Err(StatusCode::FORBIDDEN);
    };
    if !state.allow_route_at(peer.0.ip(), operation, Instant::now()) {
        return Err(StatusCode::TOO_MANY_REQUESTS);
    }
    Ok(())
}
fn invalid_token() -> Response {
    response(
        StatusCode::BAD_REQUEST,
        r#"{"message":"Invalid token","code":"INVALID_TOKEN"}"#,
    )
}
fn unconfirmed() -> Response {
    response(
        StatusCode::SERVICE_UNAVAILABLE,
        r#"{"message":"Reset outcome could not be confirmed","code":"RESET_UNCONFIRMED"}"#,
    )
}
async fn callback_reset(State(state): State<Arc<StateData>>, request: Request) -> Response {
    if let Err(r) = admission_for(&request, &state, Operation::Callback) {
        return refused(r);
    }
    let Some(token) = request
        .uri()
        .path()
        .strip_prefix("/api/auth/reset-password/")
    else {
        return invalid_token();
    };
    // Only the canonical generated bearer path is accepted: no percent-decoded aliases.
    if rentemester_storage::password_reset::validate_token(token).is_err() {
        return invalid_token();
    }
    let Some(raw) = request
        .uri()
        .query()
        .filter(|s| s.len() <= 8192)
        .and_then(|s| s.strip_prefix("callbackURL="))
    else {
        return refused(StatusCode::BAD_REQUEST);
    };
    if raw.contains('&') {
        return refused(StatusCode::BAD_REQUEST);
    }
    let bytes = raw.as_bytes();
    for (i, b) in bytes.iter().enumerate() {
        if *b == b'%'
            && !(bytes.get(i + 1).is_some_and(u8::is_ascii_hexdigit)
                && bytes.get(i + 2).is_some_and(u8::is_ascii_hexdigit))
        {
            return refused(StatusCode::BAD_REQUEST);
        }
    }
    let raw = raw.replace('+', " ");
    let Ok(redirect) = percent_encoding::percent_decode_str(&raw).decode_utf8() else {
        return refused(StatusCode::BAD_REQUEST);
    };
    if state.links.validate_redirect(Some(&redirect)).is_err() {
        return refused(StatusCode::BAD_REQUEST);
    }
    let input = ResetCallback {
        token: Zeroizing::new(token.to_owned()),
        redirect: Zeroizing::new(redirect.into_owned()),
    };
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE);
    };
    let work = state.clone();
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        (work
            .recovery
            .as_ref()
            .expect("registered callback")
            .callback)(input)
    })
    .await;
    match result {
        Ok(Ok(url)) if state.links.validate_redirect(Some(url.as_str())).is_ok() => {
            let Ok(mut location) = HeaderValue::from_str(url.as_str()) else {
                return unconfirmed();
            };
            location.set_sensitive(true);
            let mut r = response(StatusCode::FOUND, "");
            r.headers_mut().insert("location", location);
            r
        }
        _ => unconfirmed(),
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct CompleteWire {
    token: String,
    new_password: String,
}
async fn complete_reset(State(state): State<Arc<StateData>>, request: Request) -> Response {
    if request.uri().query().is_some() {
        return refused(StatusCode::BAD_REQUEST);
    }
    if let Err(r) = admission_for(&request, &state, Operation::Complete) {
        return refused(r);
    }
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE);
    };
    let bytes = match body_bytes(request).await {
        Ok(b) => b,
        Err(status) => return refused(status),
    };
    let wire: CompleteWire = match serde_json::from_slice(&bytes) {
        Ok(w) => w,
        Err(_) => return refused(StatusCode::BAD_REQUEST),
    };
    if rentemester_storage::password_reset::validate_token(&wire.token).is_err() {
        return invalid_token();
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
    let input = ResetCompletion {
        token: Zeroizing::new(wire.token),
        password: Zeroizing::new(wire.new_password),
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        (state
            .recovery
            .as_ref()
            .expect("registered completion")
            .complete)(input)
    })
    .await;
    match result {
        Ok(Ok(())) => response(StatusCode::OK, r#"{"status":true}"#),
        Ok(Err(e))
            if e.stage == rentemester_application::password_reset::FailureStage::NotCommitted
                && e.error.code == "invalid_credentials" =>
        {
            invalid_token()
        }
        _ => unconfirmed(),
    }
}

#[cfg(test)]
mod recovery_tests;

async fn body_bytes(request: Request) -> Result<Zeroizing<Vec<u8>>, StatusCode> {
    match tokio::time::timeout(
        Duration::from_secs(2),
        to_bytes(request.into_body(), BODY_LIMIT),
    )
    .await
    {
        Ok(Ok(b)) => Ok(Zeroizing::new(b.to_vec())),
        Ok(Err(_)) => Err(StatusCode::PAYLOAD_TOO_LARGE),
        Err(_) => Err(StatusCode::REQUEST_TIMEOUT),
    }
}
