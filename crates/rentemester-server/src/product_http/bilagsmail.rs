//! Human mailbox configuration and one-pass admitted receive. GET never performs provider I/O.
use super::*;
use rentemester_application::mail_intake::Action;
use rentemester_runtime::imap::{Client, NetworkClient};
enum Command {
    Status,
    Configure(Value),
    Delete,
    Alias(Option<String>),
    Poll,
}
fn failed(f: rentemester_application::control_fence::Failure) -> Response {
    let unknown = f.stage == FailureStage::OutcomeUnknown;
    let status = if unknown {
        StatusCode::SERVICE_UNAVAILABLE
    } else if matches!(f.error.code, "access_denied" | "invalid_credentials") {
        StatusCode::FORBIDDEN
    } else {
        StatusCode::UNPROCESSABLE_ENTITY
    };
    response(
        status,
        json!({"ok":false,"code":if unknown{"internal"}else{f.error.code},"errors":[f.error.message]}),
    )
}
pub(super) async fn handle(
    State(state): State<Product>,
    axum::extract::Path(parameters): axum::extract::Path<std::collections::HashMap<String, String>>,
    request: Request,
) -> Response {
    let path = request.uri().path().to_owned();
    let method = request.method().clone();
    let mutation = method != axum::http::Method::GET;
    let cookie = match proof(&state, &request, mutation) {
        Ok(c) => c,
        Err(r) => return *r,
    };
    if !selected(&state, &request)
        || request.uri().query().is_some()
        || request.headers().contains_key("content-encoding")
    {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    }
    let slug = parameters.get("slug").cloned().unwrap_or_default();
    let command = match (method, path.rsplit('/').next()) {
        (axum::http::Method::GET, Some("bilagsmail")) => Command::Status,
        (axum::http::Method::DELETE, Some("imap-config")) => {
            let Ok(bytes) = to_bytes(request.into_body(), 1).await else {
                return refused(StatusCode::BAD_REQUEST, "invalid_input");
            };
            if !bytes.is_empty() {
                return refused(StatusCode::BAD_REQUEST, "invalid_input");
            }
            Command::Delete
        }
        (method, Some(part))
            if (method == axum::http::Method::POST && matches!(part, "imap-config" | "poll"))
                || (method == axum::http::Method::PATCH && part == "alias") =>
        {
            if !one(request.headers(), "content-type")
                .is_some_and(|v| v.eq_ignore_ascii_case("application/json"))
            {
                return refused(StatusCode::BAD_REQUEST, "invalid_input");
            }
            let Ok(bytes) = to_bytes(request.into_body(), 16384).await else {
                return refused(StatusCode::PAYLOAD_TOO_LARGE, "upload_too_large");
            };
            let Ok(body) = serde_json::from_slice::<Value>(&bytes) else {
                return refused(StatusCode::BAD_REQUEST, "invalid_input");
            };
            match part {
                "imap-config" => Command::Configure(body),
                "poll" => {
                    if body != json!({"confirm":true}) {
                        return refused(StatusCode::BAD_REQUEST, "confirmation_required");
                    }
                    Command::Poll
                }
                _ => {
                    #[derive(serde::Deserialize)]
                    #[serde(deny_unknown_fields)]
                    struct Alias {
                        alias: Option<String>,
                    }
                    // Require the explicit nullable source field, never treat an omitted value as deletion.
                    if body.get("alias").is_none() {
                        return refused(StatusCode::BAD_REQUEST, "invalid_input");
                    }
                    let Ok(alias) = serde_json::from_value::<Alias>(body) else {
                        return refused(StatusCode::BAD_REQUEST, "invalid_input");
                    };
                    Command::Alias(alias.alias)
                }
            }
        }
        _ => return refused(StatusCode::NOT_FOUND, "not_found"),
    };
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        if matches!(command, Command::Poll) {
            let admission = {
                let Ok(mut owner) = state.native.lock_company(&slug) else {
                    return refused(StatusCode::SERVICE_UNAVAILABLE, "owner_unavailable");
                };
                let Ok(attempt) =
                    owner.prepare_mail_poll(&state.native.cookies, &cookie, &SystemClock)
                else {
                    return refused(StatusCode::SERVICE_UNAVAILABLE, "internal");
                };
                if attempt.retired {
                    return refused(StatusCode::SERVICE_UNAVAILABLE, "internal");
                }
                match attempt.outcome {
                    Ok(value) => value.value,
                    Err(f) => return failed(f),
                }
            };
            // No Owner guard or SQL transaction survives into this network call.
            let outcome = match NetworkClient::new(state.native.imap_transport.clone()) {
                Ok(mut client) => client.fetch(admission.config(), admission.cursor()),
                Err(e) => Err(e),
            };
            let Ok(mut owner) = state.native.lock_company(&slug) else {
                return refused(StatusCode::SERVICE_UNAVAILABLE, "owner_unavailable");
            };
            let Ok(attempt) = owner.finish_mail_poll(
                &state.native.cookies,
                &cookie,
                &SystemClock,
                admission,
                outcome,
            ) else {
                return refused(StatusCode::SERVICE_UNAVAILABLE, "internal");
            };
            return match attempt.outcome {
                Ok(value) => response(
                    if value.value["ok"] == false {
                        StatusCode::UNPROCESSABLE_ENTITY
                    } else {
                        StatusCode::OK
                    },
                    value.value,
                ),
                Err(f) => failed(f),
            };
        }
        let Ok(mut owner) = state.native.lock_company(&slug) else {
            return refused(StatusCode::SERVICE_UNAVAILABLE, "owner_unavailable");
        };
        let action = match &command {
            Command::Status => Action::Status,
            Command::Configure(v) => Action::Configure(v),
            Command::Delete => Action::DeleteConfig,
            Command::Alias(a) => Action::Alias(a.as_deref()),
            Command::Poll => unreachable!(),
        };
        let Ok(attempt) = owner.mail_intake(
            &state.native.cookies,
            &cookie,
            &SystemClock,
            state.native.mail_executor.as_deref(),
            action,
        ) else {
            return refused(StatusCode::SERVICE_UNAVAILABLE, "internal");
        };
        match attempt.outcome {
            Ok(mut value) => {
                value.value["ok"] = json!(true);
                response(StatusCode::OK, value.value)
            }
            Err(f) => failed(f),
        }
    })
    .await;
    match result {
        Ok(r) => r,
        Err(_) => refused(StatusCode::SERVICE_UNAVAILABLE, "internal"),
    }
}
