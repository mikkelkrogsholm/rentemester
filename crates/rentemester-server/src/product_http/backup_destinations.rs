use super::*;
use rentemester_application::backup_destination::{Action, Runtime};
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Input {
    operation_id: String,
    confirm: bool,
}
pub(super) async fn handle(State(state): State<Product>, request: Request) -> Response {
    let mutation = request.method() == axum::http::Method::POST;
    let cookie = match proof(&state, &request, mutation) {
        Ok(v) => v,
        Err(r) => return *r,
    };
    if request.uri().query().is_some() || request.headers().contains_key("content-encoding") {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    }
    let path = request.uri().path().to_owned();
    let parts = path.split('/').collect::<Vec<_>>();
    let destination = parts.get(4).copied().unwrap_or("").to_owned();
    let operation = if mutation {
        if !one(request.headers(), "content-type")
            .is_some_and(|v| v.eq_ignore_ascii_case("application/json"))
        {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        }
        let Ok(bytes) = to_bytes(request.into_body(), 2048).await else {
            return refused(StatusCode::PAYLOAD_TOO_LARGE, "invalid_input");
        };
        let Ok(input) = serde_json::from_slice::<Input>(&bytes) else {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        };
        if !input.confirm {
            return refused(StatusCode::BAD_REQUEST, "confirmation_required");
        }
        input.operation_id
    } else {
        if to_bytes(request.into_body(), 0).await.is_err() {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        }
        parts.get(6).copied().unwrap_or("").to_owned()
    };
    if !destination.is_empty()
        && (!rentemester_runtime::backup_destination::valid_id(&destination)
            || !rentemester_runtime::backup_destination::valid_id(&operation))
    {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    }
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let result = tokio::task::spawn_blocking(
        move || -> Result<Result<Value, rentemester_application::password_login::Failure>, ()> {
            let _permit = permit;
            let key = state.native.backup_signer.as_ref().ok_or(())?;
            let trusted = rentemester_runtime::backup_artifact::TrustedPublicKey::from_bytes(
                &key.public_key_bytes(),
            )
            .map_err(|_| ())?;
            let runtime = Runtime {
                cookies: &state.native.cookies,
                clock: &SystemClock,
                configs: &state.native.backup_destinations,
                uid: state.native.backup_destination_uid,
                trusted: &trusted,
            };
            let mut owner = state.native.owner.lock().map_err(|_| ())?;
            // A backup snapshot never grants later publication authority. The
            // second owner call reauthorizes under a held control transaction.
            let wire = if mutation {
                let attempt = owner
                    .backup_workspace(&state.native.cookies, &SystemClock, &cookie, key)
                    .map_err(|_| ())?;
                match attempt.outcome {
                    Ok(wire) => wire,
                    Err(error) => return Ok(Err(error)),
                }
            } else {
                Vec::new()
            };
            let action = if mutation {
                Action::Place {
                    destination: &destination,
                    operation: &operation,
                    wire: &wire,
                }
            } else if destination.is_empty() {
                Action::List
            } else {
                Action::Verify {
                    destination: &destination,
                    operation: &operation,
                }
            };
            Ok(owner
                .backup_destination(&runtime, &cookie, action)
                .map_err(|_| ())?
                .outcome)
        },
    )
    .await;
    match result {
        Ok(Ok(Ok(value))) => response(StatusCode::OK, value),
        Ok(Ok(Err(error))) => response(
            if error.stage == rentemester_application::password_login::FailureStage::OutcomeUnknown
            {
                StatusCode::SERVICE_UNAVAILABLE
            } else if error.error.code == "access_denied" {
                StatusCode::FORBIDDEN
            } else {
                StatusCode::UNPROCESSABLE_ENTITY
            },
            json!({"ok":false,"code":error.error.code,"errors":[error.error.message],"outcomeUnknown":error.stage==rentemester_application::password_login::FailureStage::OutcomeUnknown}),
        ),
        _ => refused(
            StatusCode::SERVICE_UNAVAILABLE,
            "backup_destination_unavailable",
        ),
    }
}
