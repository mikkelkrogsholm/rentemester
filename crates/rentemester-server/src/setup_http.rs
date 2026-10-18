//! Explicit local-operator installation capability. Never an open signup endpoint.
use crate::backend::Backend;
use axum::{
    Router,
    body::to_bytes,
    extract::{Request, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
};
use rentemester_runtime::{
    auth::OpaqueToken,
    bootstrap::{Operator, ReservationKey},
};
use serde::Deserialize;
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use subtle::ConstantTimeEq;
use tokio::sync::Semaphore;
use zeroize::{Zeroize, Zeroizing};

pub struct Installer {
    pub(crate) operator: Operator,
    pub(crate) key: ReservationKey,
    token: Mutex<Option<OpaqueToken>>,
    expires: Instant,
}
impl Installer {
    pub fn new(operator: Operator, key: ReservationKey, token: OpaqueToken) -> Self {
        Self {
            operator,
            key,
            token: Mutex::new(Some(token)),
            expires: Instant::now() + Duration::from_secs(3600),
        }
    }
}
#[derive(Clone)]
struct Setup {
    native: Arc<Backend>,
    origin: String,
    authority: String,
    workers: Arc<Semaphore>,
}
pub fn router(native: Arc<Backend>, origin: &str) -> Router {
    Router::new()
        .route("/api/setup/first-owner", post(create))
        .with_state(Setup {
            native,
            origin: origin.into(),
            authority: origin.split_once("://").unwrap().1.into(),
            workers: Arc::new(Semaphore::new(2)),
        })
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    name: String,
    email: String,
    password: String,
}
impl Drop for Input {
    fn drop(&mut self) {
        self.password.zeroize();
    }
}
fn response(status: StatusCode, body: &'static str) -> Response {
    (
        status,
        [
            ("content-type", "application/json"),
            ("cache-control", "no-store"),
            ("referrer-policy", "no-referrer"),
        ],
        body,
    )
        .into_response()
}
fn one<'a>(headers: &'a axum::http::HeaderMap, key: &str) -> Option<&'a str> {
    let mut values = headers.get_all(key).iter();
    let value = values.next()?.to_str().ok()?;
    if values.next().is_some() {
        None
    } else {
        Some(value)
    }
}
async fn create(State(state): State<Setup>, request: Request) -> Response {
    let denied = || {
        response(
            StatusCode::FORBIDDEN,
            r#"{"ok":false,"code":"setup_refused","errors":["Åbn et gyldigt installationslink fra serverens terminal."]}"#,
        )
    };
    if one(request.headers(), "host") != Some(state.authority.as_str())
        || one(request.headers(), "origin") != Some(state.origin.as_str())
        || request.headers().contains_key("authorization")
        || request.headers().contains_key("cookie")
        || request.headers().contains_key("x-rentemester-service-key")
        || request.headers().contains_key("content-encoding")
        || request.uri().query().is_some()
        || !one(request.headers(), "content-type")
            .is_some_and(|v| v.eq_ignore_ascii_case("application/json"))
    {
        return denied();
    }
    let Some(token) =
        one(request.headers(), "x-rentemester-install-token").filter(|s| s.len() <= 128)
    else {
        return denied();
    };
    let token = Zeroizing::new(token.to_owned());
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return response(
            StatusCode::SERVICE_UNAVAILABLE,
            r#"{"ok":false,"code":"busy","errors":["Prøv igen om lidt."]}"#,
        );
    };
    let bytes = match to_bytes(request.into_body(), 8192).await {
        Ok(v) => v,
        Err(_) => return denied(),
    };
    let input: Input = match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(_) => return denied(),
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let installer = state.native.setup.as_ref().ok_or(false)?;
        let mut gate = installer.token.lock().map_err(|_| true)?;
        let current = gate.as_ref().ok_or(false)?;
        if Instant::now() >= installer.expires
            || !bool::from(current.as_str().as_bytes().ct_eq(token.as_bytes()))
        {
            return Err(false);
        }
        state
            .native
            .install_owner(
                installer,
                rentemester_application::bootstrap::Input {
                    name: &input.name,
                    email: &input.email,
                    password: &input.password,
                },
            )
            .map_err(|_| true)?;
        *gate = None;
        Ok(())
    })
    .await;
    match result {
        Ok(Err(false)) => denied(),
        Ok(Ok(())) => response(StatusCode::OK, r#"{"ok":true}"#),
        _ => response(
            StatusCode::SERVICE_UNAVAILABLE,
            r#"{"ok":false,"code":"setup_unconfirmed","errors":["Opsætningen kunne ikke bekræftes. Prøv samme e-mail igen, eller kontrollér login."]}"#,
        ),
    }
}
