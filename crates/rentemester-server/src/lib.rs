//! HTTP boundary. The owner supplies an admitted executor; no workspace paths or writer grants.
use axum::{
    Router,
    body::to_bytes,
    extract::{ConnectInfo, Request, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
};
use rentemester_application::reset_request::Failure;
use rentemester_runtime::{auth::CookieProfile, recovery::RecoveryLinks};
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

struct StateData {
    origin: String,
    authority: String,
    links: RecoveryLinks,
    rate: Mutex<HashMap<IpAddr, (Instant, u8)>>,
    workers: Arc<Semaphore>,
    execute: Box<dyn Fn(ResetRequest) -> Result<(), Failure> + Send + Sync>,
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
    let links = RecoveryLinks::new(origin, &[origin], profile).map_err(|_| InvalidConfig)?;
    let parsed = Url::parse(origin).map_err(|_| InvalidConfig)?;
    let origin = parsed.origin().ascii_serialization();
    let authority = origin.split_once("://").ok_or(InvalidConfig)?.1.to_owned();
    let state = Arc::new(StateData {
        origin,
        authority,
        links,
        rate: Mutex::new(HashMap::new()),
        workers: Arc::new(Semaphore::new(4)),
        execute: Box::new(execute),
    });
    Ok(Router::new()
        .route(PATH, post(request_reset))
        .with_state(state))
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
    response(status, r#"{"code":"request_refused"}"#)
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
    fn allow(&self, peer: IpAddr) -> bool {
        self.allow_at(peer, Instant::now())
    }
    fn allow_at(&self, peer: IpAddr, now: Instant) -> bool {
        let Ok(mut keys) = self.rate.lock() else {
            return false;
        };
        keys.retain(|_, (start, _)| now.duration_since(*start) < WINDOW);
        let peer = canonical_ip(peer);
        if !keys.contains_key(&peer) && keys.len() >= MAX_KEYS {
            return false;
        }
        let (last, count) = keys.entry(peer).or_insert((now, 0));
        if *count >= 3 {
            return false;
        }
        *count += 1;
        *last = (*last).max(now);
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
    if !json_content(request.headers()) {
        return refused(StatusCode::UNSUPPORTED_MEDIA_TYPE);
    }
    let Some(peer) = request.extensions().get::<ConnectInfo<SocketAddr>>() else {
        return refused(StatusCode::FORBIDDEN);
    };
    // Never trust X-Forwarded-For or other caller-selected rate identities.
    if !state.allow(peer.0.ip()) {
        let mut r = refused(StatusCode::TOO_MANY_REQUESTS);
        r.headers_mut()
            .insert("retry-after", HeaderValue::from_static("60"));
        return r;
    }
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE);
    };
    let bytes = match tokio::time::timeout(
        Duration::from_secs(2),
        to_bytes(request.into_body(), BODY_LIMIT),
    )
    .await
    {
        Ok(Ok(bytes)) => Zeroizing::new(bytes.to_vec()),
        Ok(Err(_)) => return refused(StatusCode::PAYLOAD_TOO_LARGE),
        Err(_) => return refused(StatusCode::REQUEST_TIMEOUT),
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
        (state.execute)(input)
    })
    .await;
    response(StatusCode::OK, ACK)
}

#[cfg(test)]
mod tests;
