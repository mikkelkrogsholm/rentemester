use super::*;
use rentemester_application::backup_original122::original717::{Archive, Create, Operation};
pub(super) async fn handle(State(state): State<Product>, request: Request) -> Response {
    if request.uri().path() == "/api/original-system-restore-backup" {
        return restore(state, request).await;
    }
    if request
        .uri()
        .path()
        .ends_with("/original-system-backup-place")
    {
        return place(state, request).await;
    }
    if request
        .uri()
        .path()
        .ends_with("/original-system-backup-verify-remote-placement")
    {
        return remote(state, request).await;
    }
    let credential = match documents::credential(&state, &request, true) {
        Ok(v) => v,
        Err(r) => return *r,
    };
    if request.uri().query().is_some()
        || ["content-encoding", "transfer-encoding", "idempotency-key"]
            .iter()
            .any(|h| request.headers().contains_key(*h))
        || !one(request.headers(), "content-type")
            .is_some_and(|v| v.eq_ignore_ascii_case("application/json"))
    {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    }
    let local = rentemester_server::verified_local_uid(&request) == Some(state.native.backup_destination_uid);
    let path = request.uri().path().to_owned();
    let slug = path.split('/').nth(3).unwrap_or("").to_owned();
    let Ok(bytes) = to_bytes(request.into_body(), 16384).await else {
        return refused(StatusCode::PAYLOAD_TOO_LARGE, "invalid_input");
    };
    let operation = match path.rsplit('/').next().unwrap_or("") {
        "original-system-backup" => serde_json::from_slice::<Create>(&bytes).map(Operation::Create),
        "original-system-backup-archive" => {
            serde_json::from_slice::<Archive>(&bytes).map(Operation::Archive)
        }
        _ => return refused(StatusCode::NOT_FOUND, "not_found"),
    };
    let Ok(operation) = operation else {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    };
    if !operation.confirmed() {
        return refused(StatusCode::BAD_REQUEST, "confirmation_required");
    }
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let mut owner = state.native.lock_company(&slug).map_err(|_| {
            Box::new(refused(
                StatusCode::SERVICE_UNAVAILABLE,
                "owner_unavailable",
            ))
        })?;
        match credential {
            documents::Credential::Human(cookie) => {
                let operator = if local {
                    state.native.original_restore_operator()
                } else {
                    None
                };
                let attempt = owner
                    .backup_original717(&cookie, &SystemClock, operation, operator)
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
            }
            documents::Credential::Service(key) => {
                let attempt = owner
                    .backup_original717_service(&key, &SystemClock, operation)
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
                documents::service_outcome(attempt)
            }
        }
    })
    .await;
    match result {
        Ok(Ok(core)) => response(StatusCode::OK, json!({"ok":true,"result":core})),
        Ok(Err(r)) => *r,
        Err(_) => refused(StatusCode::SERVICE_UNAVAILABLE, "outcome_unconfirmed"),
    }
}

async fn restore(state: Product, request: Request) -> Response {
    if rentemester_server::verified_local_uid(&request) != Some(state.native.backup_destination_uid)
        || state.native.original_restore_operator().is_none()
    {
        return refused(StatusCode::FORBIDDEN, "local_operator_required");
    }
    let cookie = match proof(&state, &request, true) {
        Ok(v) => v,
        Err(r) => return *r,
    };
    if request.uri().query().is_some()
        || ["content-encoding", "transfer-encoding", "idempotency-key"]
            .iter()
            .any(|h| request.headers().contains_key(*h))
        || !one(request.headers(), "content-type")
            .is_some_and(|v| v.eq_ignore_ascii_case("application/json"))
    {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    }
    let Ok(bytes) = to_bytes(request.into_body(), 16384).await else {
        return refused(StatusCode::PAYLOAD_TOO_LARGE, "invalid_input");
    };
    let Ok(input) = serde_json::from_slice::<
        rentemester_application::backup_original122::original717::Restore,
    >(&bytes) else {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    };
    if !input.confirm {
        return refused(StatusCode::BAD_REQUEST, "confirmation_required");
    }
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let operator = state
            .native
            .original_restore_operator()
            .ok_or_else(|| Box::new(refused(StatusCode::FORBIDDEN, "local_operator_required")))?;
        let mut owner = state.native.owner.lock().map_err(|_| {
            Box::new(refused(
                StatusCode::SERVICE_UNAVAILABLE,
                "owner_unavailable",
            ))
        })?;
        let attempt = owner
            .restore_original717(
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
                "outcome_unconfirmed",
            )));
        }
        match attempt.outcome {
            Ok(core) => Ok(core),
            Err(f) if f.error.code == "access_denied" => {
                Err(Box::new(refused(StatusCode::FORBIDDEN, "access_denied")))
            }
            Err(_) => Err(Box::new(refused(
                StatusCode::SERVICE_UNAVAILABLE,
                "outcome_unconfirmed",
            ))),
        }
    })
    .await;
    match result {
        Ok(Ok(core)) => response(StatusCode::OK, json!({"ok":true,"result":core})),
        Ok(Err(r)) => *r,
        Err(_) => refused(StatusCode::SERVICE_UNAVAILABLE, "outcome_unconfirmed"),
    }
}

async fn place(state: Product, request: Request) -> Response {
    let credential = match documents::credential(&state, &request, true) {
        Ok(v) => v,
        Err(r) => return *r,
    };
    if !valid_body(&request) {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    }
    let slug = request
        .uri()
        .path()
        .split('/')
        .nth(3)
        .unwrap_or("")
        .to_owned();
    let Ok(bytes) = to_bytes(request.into_body(), 16384).await else {
        return refused(StatusCode::PAYLOAD_TOO_LARGE, "invalid_input");
    };
    let Ok(input) = serde_json::from_slice::<
        rentemester_application::backup_original122::original717::Place,
    >(&bytes) else {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    };
    if !input.confirm {
        return refused(StatusCode::BAD_REQUEST, "confirmation_required");
    }
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let mut owner = state.native.lock_company(&slug).map_err(|_| {
            Box::new(refused(
                StatusCode::SERVICE_UNAVAILABLE,
                "owner_unavailable",
            ))
        })?;
        match credential {
            documents::Credential::Human(cookie) => {
                let attempt = owner
                    .place_original717(
                        &cookie,
                        &SystemClock,
                        input,
                        &state.native.backup_destinations,
                        state.native.backup_destination_uid,
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
                        "outcome_unconfirmed",
                    )));
                }
                documents::outcome(attempt)
            }
            documents::Credential::Service(key) => {
                let attempt = owner
                    .place_original717_service(
                        &key,
                        &SystemClock,
                        input,
                        &state.native.backup_destinations,
                        state.native.backup_destination_uid,
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
                        "outcome_unconfirmed",
                    )));
                }
                documents::service_outcome(attempt)
            }
        }
    })
    .await;
    match result {
        Ok(Ok(core)) => response(StatusCode::OK, json!({"ok":true,"result":core})),
        Ok(Err(r)) => *r,
        Err(_) => refused(StatusCode::SERVICE_UNAVAILABLE, "outcome_unconfirmed"),
    }
}
fn valid_body(request: &Request) -> bool {
    request.uri().query().is_none()
        && !(["content-encoding", "transfer-encoding", "idempotency-key"]
            .iter()
            .any(|h| request.headers().contains_key(*h)))
        && one(request.headers(), "content-type")
            .is_some_and(|v| v.eq_ignore_ascii_case("application/json"))
}
async fn remote(state: Product, request: Request) -> Response {
    use rentemester_application::backup_original122::original717::{Remote, RemotePreparation};
    use rentemester_runtime::backup_remote::Verifier;
    let credential = match documents::credential(&state, &request, true) {
        Ok(v) => v,
        Err(r) => return *r,
    };
    if !valid_body(&request) {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    }
    let slug = request
        .uri()
        .path()
        .split('/')
        .nth(3)
        .unwrap_or("")
        .to_owned();
    let Ok(bytes) = to_bytes(request.into_body(), 16384).await else {
        return refused(StatusCode::PAYLOAD_TOO_LARGE, "invalid_input");
    };
    let Ok(input) = serde_json::from_slice::<Remote>(&bytes) else {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    };
    if !input.confirm {
        return refused(StatusCode::BAD_REQUEST, "confirmation_required");
    }
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let bindings = state
            .native
            .backup_remote
            .as_ref()
            .map(|p| p.bindings.as_slice())
            .unwrap_or(&[]);
        let prepared = {
            let mut owner = state.native.lock_company(&slug).map_err(|_| {
                Box::new(refused(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "owner_unavailable",
                ))
            })?;
            match &credential {
                documents::Credential::Human(cookie) => {
                    let attempt = owner
                        .prepare_remote_original717(
                            cookie,
                            &SystemClock,
                            input,
                            bindings,
                            state.native.backup_destination_uid,
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
                            "outcome_unconfirmed",
                        )));
                    }
                    documents::outcome(attempt)?
                }
                documents::Credential::Service(key) => {
                    let attempt = owner
                        .prepare_remote_original717_service(
                            key,
                            &SystemClock,
                            input,
                            bindings,
                            state.native.backup_destination_uid,
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
                            "outcome_unconfirmed",
                        )));
                    }
                    documents::service_outcome(attempt)?
                }
            }
        };
        let prepared = match prepared {
            RemotePreparation::Complete(core) => return Ok(core),
            RemotePreparation::Ready(p) => p,
        };
        // Provider I/O is outside the Owner lock and both SQL transactions.
        let observation = state.native.backup_remote.as_ref().ok_or(()).and_then(|p| {
            p.verifier
                .verify(&prepared.expected, &SystemClock)
                .map_err(|_| ())
        });
        let mut owner = state.native.lock_company(&slug).map_err(|_| {
            Box::new(refused(
                StatusCode::SERVICE_UNAVAILABLE,
                "owner_unavailable",
            ))
        })?;
        match credential {
            documents::Credential::Human(cookie) => {
                let attempt = owner
                    .finish_remote_original717(
                        &cookie,
                        &SystemClock,
                        &prepared,
                        observation.as_ref().map_err(|_| ()),
                        bindings,
                        state.native.backup_destination_uid,
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
                        "outcome_unconfirmed",
                    )));
                }
                documents::outcome(attempt)
            }
            documents::Credential::Service(key) => {
                let attempt = owner
                    .finish_remote_original717_service(
                        &key,
                        &SystemClock,
                        &prepared,
                        observation.as_ref().map_err(|_| ()),
                        bindings,
                        state.native.backup_destination_uid,
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
                        "outcome_unconfirmed",
                    )));
                }
                documents::service_outcome(attempt)
            }
        }
    })
    .await;
    match result {
        Ok(Ok(core)) => response(StatusCode::OK, json!({"ok":true,"result":core})),
        Ok(Err(r)) => *r,
        Err(_) => refused(StatusCode::SERVICE_UNAVAILABLE, "outcome_unconfirmed"),
    }
}
