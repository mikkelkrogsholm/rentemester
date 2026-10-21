//! Exact remote reads run outside the Owner mutex; final receipt has fresh authority.
use super::*;
use rentemester_application::{
    backup_remote as app,
    password_login::{Failure, FailureStage},
};
use rentemester_runtime::backup_remote::Verifier;
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Input {
    operation_id: String,
    placement_operation_id: String,
    remote_object_id: String,
    confirm: bool,
}
fn unavailable(code: &'static str) -> Failure {
    Failure {
        stage: FailureStage::NotCommitted,
        error: rentemester_engine::Error::new(
            code,
            "Remote verification unavailable; read the exact verification ID before another deliberate action",
        ),
        connection_reusable: true,
    }
}
pub(super) async fn handle(State(state): State<Product>, request: Request) -> Response {
    let mutation = request.method() == axum::http::Method::POST;
    let cookie = match proof(&state, &request, mutation) {
        Ok(proof) => proof,
        Err(r) => return *r,
    };
    if request.uri().query().is_some() || request.headers().contains_key("content-encoding") {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    }
    let path = request.uri().path().to_owned();
    let parts = path.split('/').collect::<Vec<_>>();
    let destination = parts.get(4).copied().unwrap_or("").to_owned();
    if !rentemester_runtime::backup_destination::valid_id(&destination) {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    }
    let input = if mutation {
        if !one(request.headers(), "content-type")
            .is_some_and(|v| v.eq_ignore_ascii_case("application/json"))
        {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        }
        let Ok(bytes) = to_bytes(request.into_body(), 4096).await else {
            return refused(StatusCode::PAYLOAD_TOO_LARGE, "invalid_input");
        };
        let Ok(input) = serde_json::from_slice::<Input>(&bytes) else {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        };
        if !input.confirm {
            return refused(StatusCode::BAD_REQUEST, "confirmation_required");
        }
        if !rentemester_runtime::backup_destination::valid_id(&input.operation_id)
            || !rentemester_runtime::backup_destination::valid_id(&input.placement_operation_id)
            || !rentemester_runtime::backup_remote::valid_object_id(&input.remote_object_id)
        {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        }
        Some(input)
    } else {
        if to_bytes(request.into_body(), 0).await.is_err() {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        }
        None
    };
    let operation = input
        .as_ref()
        .map(|i| i.operation_id.clone())
        .unwrap_or_else(|| parts.get(6).copied().unwrap_or("").to_owned());
    if !rentemester_runtime::backup_destination::valid_id(&operation) {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    }
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let result = tokio::task::spawn_blocking(move || -> Result<Result<Value, Failure>, ()> {
        let _permit = permit;
        if !mutation {
            let mut owner = state.native.owner.lock().map_err(|_| ())?;
            return Ok(owner
                .backup_remote_verification(
                    &state.native.cookies,
                    &SystemClock,
                    &cookie,
                    &destination,
                    &operation,
                )
                .map_err(|_| ())?
                .outcome);
        }
        let provider = match state.native.backup_remote.as_ref() {
            Some(provider) => provider,
            None => return Ok(Err(unavailable("backup_remote_not_configured"))),
        };
        let key = state.native.backup_signer.as_ref().ok_or(())?;
        let trusted = rentemester_runtime::backup_artifact::TrustedPublicKey::from_bytes(
            &key.public_key_bytes(),
        )
        .map_err(|_| ())?;
        let local = rentemester_application::backup_destination::Runtime {
            cookies: &state.native.cookies,
            clock: &SystemClock,
            configs: &state.native.backup_destinations,
            uid: state.native.backup_destination_uid,
            trusted: &trusted,
        };
        let runtime = app::Runtime {
            local: &local,
            bindings: &provider.bindings,
        };
        let input = input.ok_or(())?;
        let request = app::Request {
            destination_id: destination,
            operation_id: operation,
            placement_operation_id: input.placement_operation_id,
            remote_object_id: input.remote_object_id,
        };
        let prepared = {
            let mut owner = state.native.owner.lock().map_err(|_| ())?;
            match owner
                .prepare_backup_remote(&runtime, &cookie, &request)
                .map_err(|_| ())?
                .outcome
            {
                Ok(prepared) => prepared,
                Err(failure) => return Ok(Err(failure)),
            }
        };
        let prepared = match prepared {
            app::Preparation::Ready(prepared) => prepared,
            app::Preparation::Complete(value) => return Ok(Ok(value)),
        };
        // Only immutable prepared identity crosses this boundary; no SQL/Owner lock.
        let observation = match provider.verifier.verify(&prepared.expected, &SystemClock) {
            Ok(value) => value,
            Err(error) => {
                return Ok(Err(Failure {
                    stage: FailureStage::NotCommitted,
                    error,
                    connection_reusable: true,
                }));
            }
        };
        let mut owner = state.native.owner.lock().map_err(|_| ())?;
        Ok(owner
            .finish_backup_remote(&runtime, &cookie, &prepared, &observation)
            .map_err(|_| ())?
            .outcome)
    })
    .await;
    match result {
        Ok(Ok(Ok(value))) => response(StatusCode::OK, value),
        Ok(Ok(Err(failure))) => {
            let unknown = failure.stage == FailureStage::OutcomeUnknown;
            let status = if unknown
                || matches!(
                    failure.error.code,
                    "backup_remote_unavailable" | "backup_remote_not_configured"
                ) {
                StatusCode::SERVICE_UNAVAILABLE
            } else if matches!(failure.error.code, "access_denied" | "invalid_credentials") {
                StatusCode::FORBIDDEN
            } else {
                StatusCode::UNPROCESSABLE_ENTITY
            };
            response(
                status,
                json!({"ok":false,"code":failure.error.code,"errors":[failure.error.message],"outcomeUnknown":unknown}),
            )
        }
        _ => refused(StatusCode::SERVICE_UNAVAILABLE, "backup_remote_unavailable"),
    }
}
