//! Source custody routes: no request actor or server-side public export destination.
use super::super::*;
use rentemester_application::original716::custody::{
    ExportPublicKey, Rotate, SignedBackup, Verify,
};
/// POST company original-system-export-public-key / original-system-rotate-backup-keypair,
/// or POST /api/original-system-verify-backup-signature.
pub(crate) async fn handle(State(state): State<Product>, request: Request) -> Response {
    let cookie = match proof(&state, &request, true) {
        Ok(v) => v,
        Err(r) => return *r,
    };
    if request.method() != axum::http::Method::POST
        || request.uri().query().is_some()
        || ["content-encoding", "transfer-encoding", "idempotency-key"]
            .iter()
            .any(|h| request.headers().contains_key(*h))
        || !one(request.headers(), "content-type")
            .is_some_and(|v| v.eq_ignore_ascii_case("application/json"))
    {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    }
    let path = request.uri().path().to_owned();
    let command = path.rsplit('/').next().unwrap_or("").to_owned();
    let local = rentemester_server::verified_local_uid(&request)
        == Some(state.native.backup_destination_uid);
    if !matches!(
        command.as_str(),
        "original-system-export-public-key" | "original-system-backup-signed"
    ) && (!local || state.native.original_restore_operator().is_none())
    {
        return refused(StatusCode::FORBIDDEN, "local_operator_required");
    }
    let slug = path.split('/').nth(3).unwrap_or("").to_owned();
    let Ok(bytes) = to_bytes(request.into_body(), 16384).await else {
        return refused(StatusCode::PAYLOAD_TOO_LARGE, "invalid_input");
    };
    enum Input {
        Export,
        Rotate(Rotate),
        Verify(Verify),
        Signed(SignedBackup),
    }
    let input = match command.as_str() {
        "original-system-backup-signed" => {
            serde_json::from_slice::<SignedBackup>(&bytes).map(Input::Signed)
        }
        "original-system-export-public-key" => {
            serde_json::from_slice::<ExportPublicKey>(&bytes).map(|_| Input::Export)
        }
        "original-system-rotate-backup-keypair" => {
            serde_json::from_slice::<Rotate>(&bytes).map(Input::Rotate)
        }
        "original-system-verify-backup-signature" => {
            serde_json::from_slice::<Verify>(&bytes).map(Input::Verify)
        }
        _ => return refused(StatusCode::NOT_FOUND, "not_found"),
    };
    let Ok(input) = input else {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    };
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        if let Input::Verify(input) = input {
            let operator = state.native.original_restore_operator().ok_or_else(|| {
                Box::new(refused(StatusCode::FORBIDDEN, "local_operator_required"))
            })?;
            let attempt = state
                .native
                .owner
                .lock()
                .map_err(|_| {
                    Box::new(refused(
                        StatusCode::SERVICE_UNAVAILABLE,
                        "owner_unavailable",
                    ))
                })?
                .verify_backup_signature_original716(
                    &state.native.cookies,
                    &SystemClock,
                    &cookie,
                    operator,
                    state.native.backup_destination_uid,
                    input,
                )
                .map_err(|_| {
                    Box::new(refused(
                        StatusCode::SERVICE_UNAVAILABLE,
                        "owner_unavailable",
                    ))
                })?;
            if attempt.retired {
                return Err(Box::new(refused(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "owner_unavailable",
                )));
            }
            return attempt.outcome.map_err(|f| {
                Box::new(refused(
                    if f.error.code == "access_denied" {
                        StatusCode::FORBIDDEN
                    } else {
                        StatusCode::SERVICE_UNAVAILABLE
                    },
                    f.error.code,
                ))
            });
        }
        let mut owner = state.native.lock_company(&slug).map_err(|_| {
            Box::new(refused(
                StatusCode::SERVICE_UNAVAILABLE,
                "owner_unavailable",
            ))
        })?;
        let attempt = match input {
            Input::Signed(input) => owner.signed_backup_original716(
                &cookie,
                &SystemClock,
                input,
                state.native.original_restore_operator(),
            ),
            Input::Export => owner.export_backup_public_key_original716(&cookie, &SystemClock),
            Input::Rotate(input) => owner.rotate_backup_keypair_original716(
                &cookie,
                &SystemClock,
                input,
                state.native.original_restore_operator(),
            ),
            Input::Verify(_) => unreachable!(),
        }
        .map_err(|_| {
            Box::new(refused(
                StatusCode::SERVICE_UNAVAILABLE,
                "owner_unavailable",
            ))
        })?;
        if attempt.retired {
            return Err(Box::new(refused(
                StatusCode::SERVICE_UNAVAILABLE,
                "outcome_unconfirmed",
            )));
        }
        documents::outcome(attempt)
    })
    .await;
    match result {
        Ok(Ok(value)) => response(StatusCode::OK, json!({"ok":true,"result":value})),
        Ok(Err(r)) => *r,
        Err(_) => refused(StatusCode::SERVICE_UNAVAILABLE, "outcome_unconfirmed"),
    }
}
