//! Workspace-control HTTP adapter. Public claim grants no session or MFA assertion.
use super::*;
use rentemester_application::workspace_access as app;
use zeroize::{Zeroize, Zeroizing};
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Change {
    user_id: String,
    action: String,
    workspace_role: Option<String>,
    company_slug: Option<String>,
    role: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Cancel {
    invitation_id: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Claim {
    token: String,
    name: String,
    password: String,
}
impl Drop for Claim {
    fn drop(&mut self) {
        self.token.zeroize();
        self.password.zeroize();
    }
}
enum Command {
    Members,
    Invitations,
    Issue(app::Issue),
    Cancel(Cancel),
    Access(Change),
    Company(Change),
    Claim(Claim),
}
pub(super) async fn handle(State(state): State<Product>, request: Request) -> Response {
    let path = request.uri().path().to_owned();
    let public = path == "/api/invitations/claim";
    let mutation = request.method() == axum::http::Method::POST;
    if request.uri().query().is_some() {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    }
    let cookie = if public {
        let h = request.headers();
        if one(h, "host") != Some(&state.authority)
            || one(h, "origin") != Some(&state.origin)
            || h.contains_key("authorization")
            || h.contains_key("x-rentemester-service-key")
            || h.contains_key("sec-fetch-site")
                && !matches!(
                    one(h, "sec-fetch-site"),
                    Some("same-origin" | "same-site" | "none")
                )
        {
            return refused(StatusCode::FORBIDDEN, "origin_refused");
        }
        None
    } else {
        match proof(&state, &request, mutation) {
            Ok(p) => Some(p),
            Err(r) => return *r,
        }
    };
    let command = if mutation {
        if request.headers().contains_key("content-encoding")
            || !one(request.headers(), "content-type")
                .is_some_and(|s| s.eq_ignore_ascii_case("application/json"))
        {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        }
        let Ok(body) = to_bytes(request.into_body(), 32768).await else {
            return refused(StatusCode::PAYLOAD_TOO_LARGE, "invalid_input");
        };
        let body = Zeroizing::new(body.to_vec());
        let parsed = match path.as_str() {
            "/api/workspace/invitations" => serde_json::from_slice(&body).map(Command::Issue),
            "/api/workspace/invitations/cancel" => {
                serde_json::from_slice(&body).map(Command::Cancel)
            }
            "/api/workspace/members/access" => serde_json::from_slice(&body).map(Command::Access),
            "/api/workspace/members/company" => serde_json::from_slice(&body).map(Command::Company),
            "/api/invitations/claim" => serde_json::from_slice(&body).map(Command::Claim),
            _ => return refused(StatusCode::NOT_FOUND, "not_found"),
        };
        match parsed {
            Ok(c) => c,
            Err(_) => return refused(StatusCode::BAD_REQUEST, "invalid_input"),
        }
    } else if path == "/api/workspace/invitations" {
        Command::Invitations
    } else {
        Command::Members
    };
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let native = state.native;
        let mut owner = native.owner.lock().map_err(|_| ())?;
        let catalog=owner.catalog().map_err(|_|())?;
        let runtime = app::Runtime {
            cookies: &native.cookies,
            clock: &SystemClock,
            key: &native.invitation_key,
            catalog: &catalog,
        };
        let attempt = if let Command::Claim(i) = &command {
            let verification = rentemester_application::email_verification::Runtime {
                signer: &native.signer,
                clock: &SystemClock,
                audit_key_version: 1,
            };
            let vr = rentemester_application::verification_request::Runtime {
                verification: &verification,
                cookies: &native.cookies,
                links: &native.links,
            };
            owner.claim_invitation(
                &runtime,
                &vr,
                &mut *native.delivery.lock().map_err(|_| ())?,
                app::Claim {
                    token: &i.token,
                    name: &i.name,
                    password: &i.password,
                },
            )
        } else {
            let Some(cookie) = cookie else { return Err(()) };
            let action = match &command {
                Command::Members => app::Action::Members,
                Command::Invitations => app::Action::Invitations,
                Command::Issue(i) => app::Action::Issue(i),
                Command::Cancel(i) => app::Action::Cancel(&i.invitation_id),
                Command::Access(i) => match i.action.as_str() {
                    "set-role"
                        if i.workspace_role.is_some()
                            && i.company_slug.is_none()
                            && i.role.is_none() =>
                    {
                        app::Action::Access {
                            user: &i.user_id,
                            role: i.workspace_role.as_deref(),
                        }
                    }
                    "disable"
                        if i.workspace_role.is_none()
                            && i.company_slug.is_none()
                            && i.role.is_none() =>
                    {
                        app::Action::Access {
                            user: &i.user_id,
                            role: None,
                        }
                    }
                    _ => return Ok(Err((StatusCode::BAD_REQUEST, "invalid_input"))),
                },
                Command::Company(i) => match (i.action.as_str(), i.company_slug.as_deref()) {
                    ("grant", Some(slug)) if i.role.is_some() && i.workspace_role.is_none() => {
                        app::Action::Company {
                            user: &i.user_id,
                            slug,
                            role: i.role.as_deref(),
                        }
                    }
                    ("revoke", Some(slug)) if i.role.is_none() && i.workspace_role.is_none() => {
                        app::Action::Company {
                            user: &i.user_id,
                            slug,
                            role: None,
                        }
                    }
                    _ => return Ok(Err((StatusCode::BAD_REQUEST, "invalid_input"))),
                },
                Command::Claim(_) => unreachable!(),
            };
            owner.workspace_access(&runtime, &cookie, action, &mut |recipient, token| {
                native.delivery.lock().map_err(|_|())?.invitation(&state.origin, recipient, token)
            })
        }
        .map_err(|_| ())?;
        Ok(match attempt.outcome {
            Ok(mut v) => {
                v["ok"] = json!(true);
                Ok(v)
            }
            Err(f) => Err(
                if f.stage == rentemester_application::password_login::FailureStage::OutcomeUnknown
                {
                    (StatusCode::SERVICE_UNAVAILABLE, "internal")
                } else if ["invalid_credentials", "access_denied"].contains(&f.error.code) {
                    (StatusCode::FORBIDDEN, "access_denied")
                } else if f.error.code == "auth_delivery_unconfirmed" {
                    (StatusCode::SERVICE_UNAVAILABLE, "auth_delivery_unconfirmed")
                } else {
                    (
                        StatusCode::BAD_REQUEST,
                        if public {
                            "invitation_refused"
                        } else {
                            f.error.code
                        },
                    )
                },
            ),
        })
    })
    .await;
    let mut response = match result {
        Ok(Ok(Ok(v))) => response(StatusCode::OK, v),
        Ok(Ok(Err((s, c)))) => refused(s, c),
        _ => refused(StatusCode::SERVICE_UNAVAILABLE, "internal"),
    };
    response.headers_mut().insert(
        "referrer-policy",
        axum::http::HeaderValue::from_static("no-referrer"),
    );
    response
}
