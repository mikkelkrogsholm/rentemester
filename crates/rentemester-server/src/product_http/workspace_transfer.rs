//! Explicit human handoff management. No request supplies operator keys or paths.
use super::*;
use rentemester_storage::workspace_transfer::{Activate, Cancel, Export, Freeze, Release};
enum Command {
    Status,
    Freeze(Freeze),
    Cancel(Cancel),
    Export(Export),
    Release(Release),
    Activate(Activate),
}
pub(super) async fn handle(State(state): State<Product>, request: Request) -> Response {
    let write = request.method() == axum::http::Method::POST;
    let cookie = match proof(&state, &request, write) {
        Ok(p) => p,
        Err(r) => return *r,
    };
    if request.uri().query().is_some() || request.headers().contains_key("content-encoding") {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    }
    let command = if write {
        if !one(request.headers(), "content-type")
            .is_some_and(|v| v.eq_ignore_ascii_case("application/json"))
        {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        }
        let action = request
            .uri()
            .path()
            .rsplit('/')
            .next()
            .unwrap_or("")
            .to_owned();
        let bound = if matches!(action.as_str(), "release" | "activate") {
            32 * 1024
        } else {
            16 * 1024
        };
        let Ok(bytes) = to_bytes(request.into_body(), bound).await else {
            return refused(StatusCode::PAYLOAD_TOO_LARGE, "invalid_input");
        };
        let parsed = match action.as_str() {
            "freeze" => serde_json::from_slice(&bytes).map(Command::Freeze),
            "cancel" => serde_json::from_slice(&bytes).map(Command::Cancel),
            "export" => serde_json::from_slice(&bytes).map(Command::Export),
            "release" => serde_json::from_slice(&bytes).map(Command::Release),
            "activate" => serde_json::from_slice(&bytes).map(Command::Activate),
            _ => return refused(StatusCode::NOT_FOUND, "not_found"),
        };
        match parsed {
            Ok(v) => v,
            Err(_) => return refused(StatusCode::BAD_REQUEST, "invalid_input"),
        }
    } else {
        match to_bytes(request.into_body(), 1).await {
            Ok(v) if v.is_empty() => {}
            _ => return refused(StatusCode::BAD_REQUEST, "invalid_input"),
        }
        Command::Status
    };
    if matches!(
        command,
        Command::Freeze(_) | Command::Export(_) | Command::Release(_)
    ) && state.native.backup_signer.is_none()
    {
        return refused(StatusCode::UNPROCESSABLE_ENTITY, "transfer_signer_required");
    }
    if matches!(command, Command::Activate(_)) && state.native.transfer_trusted_key.is_none() {
        return refused(
            StatusCode::UNPROCESSABLE_ENTITY,
            "transfer_trusted_key_required",
        );
    }
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let export = matches!(command, Command::Export(_));
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let mut owner = state.native.owner.lock().map_err(|_| ())?;
        let cookies = &state.native.cookies;
        if let Command::Export(input) = command {
            return owner
                .export_workspace_transfer(
                    cookies,
                    &SystemClock,
                    &cookie,
                    &input,
                    state.native.backup_signer.as_ref().ok_or(())?,
                )
                .map(|a| a.outcome)
                .map_err(|_| ());
        }
        let attempt = match &command {
            Command::Status => owner.workspace_transfer_status(cookies, &SystemClock, &cookie),
            Command::Freeze(input) => owner.freeze_workspace(cookies, &SystemClock, &cookie, input),
            Command::Cancel(input) => {
                owner.cancel_workspace_transfer(cookies, &SystemClock, &cookie, input)
            }
            Command::Release(input) => owner.release_workspace_transfer(
                cookies,
                &SystemClock,
                &cookie,
                input,
                state.native.backup_signer.as_ref().ok_or(())?,
            ),
            Command::Activate(input) => owner.activate_workspace_transfer(
                cookies,
                &SystemClock,
                &cookie,
                input,
                state.native.transfer_trusted_key.as_ref().ok_or(())?,
            ),
            Command::Export(_) => unreachable!(),
        }
        .map_err(|_| ())?;
        Ok(attempt
            .outcome
            .map(|value| serde_json::to_vec(&value).expect("JSON Value serializes")))
    })
    .await;
    match result {
        Ok(Ok(Ok(bytes))) if export => {
            let mut response = (StatusCode::OK, bytes).into_response();
            response.headers_mut().insert(
                "content-type",
                axum::http::HeaderValue::from_static(
                    "application/vnd.rentemester.native-backup+json",
                ),
            );
            response.headers_mut().insert(
                "content-disposition",
                axum::http::HeaderValue::from_static(
                    "attachment; filename=workspace.transfer.native-backup.json",
                ),
            );
            response
        }
        Ok(Ok(Ok(bytes))) => match serde_json::from_slice::<serde_json::Value>(&bytes) {
            Ok(value) => response(StatusCode::OK, json!({"ok":true,"transfer":value})),
            Err(_) => refused(StatusCode::SERVICE_UNAVAILABLE, "outcome_unknown"),
        },
        Ok(Ok(Err(failure)))
            if failure.stage
                == rentemester_application::password_login::FailureStage::NotCommitted =>
        {
            response(
                if failure.error.code == "invalid_credentials" {
                    StatusCode::UNAUTHORIZED
                } else if failure.error.code == "access_denied" {
                    StatusCode::FORBIDDEN
                } else {
                    StatusCode::UNPROCESSABLE_ENTITY
                },
                json!({"ok":false,"code":failure.error.code,"errors":[failure.error.message]}),
            )
        }
        _ => refused(StatusCode::SERVICE_UNAVAILABLE, "outcome_unknown"),
    }
}
