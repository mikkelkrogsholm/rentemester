use super::super::*;
use rentemester_application::backup_registration::original717::{Add, Confirm, Operation, Remove};
pub(super) async fn handle(State(state): State<Product>, request: Request) -> Response {
    let mutation = request.method() == axum::http::Method::POST;
    let credential = match documents::credential(&state, &request, mutation) {
        Ok(v) => v,
        Err(r) => return *r,
    };
    if ["content-encoding", "transfer-encoding", "idempotency-key"]
        .iter()
        .any(|h| request.headers().contains_key(*h))
    {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    }
    let path = request.uri().path().to_owned();
    let slug = path.split('/').nth(3).unwrap_or("").to_owned();
    let mut date = None;
    for (k, v) in url::form_urlencoded::parse(request.uri().query().unwrap_or("").as_bytes()) {
        if mutation
            || !path.ends_with("original-system-backup-governance")
            || k != "asOf"
            || date.is_some()
        {
            return refused(StatusCode::BAD_REQUEST, "invalid_query");
        }
        date = Some(v.into_owned());
    }
    let op = if mutation {
        if !one(request.headers(), "content-type")
            .is_some_and(|v| v.eq_ignore_ascii_case("application/json"))
        {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        }
        let Ok(bytes) = to_bytes(request.into_body(), 16384).await else {
            return refused(StatusCode::PAYLOAD_TOO_LARGE, "invalid_input");
        };
        let parsed = match path.rsplit('/').next().unwrap_or("") {
            "original-system-backup-destination-add" => {
                serde_json::from_slice::<Add>(&bytes).map(Operation::Add)
            }
            "original-system-backup-destination-remove" => {
                serde_json::from_slice::<Remove>(&bytes).map(Operation::Remove)
            }
            "original-system-backup-confirm-placement" => {
                serde_json::from_slice::<Confirm>(&bytes).map(Operation::Confirm)
            }
            _ => return refused(StatusCode::NOT_FOUND, "not_found"),
        };
        match parsed {
            Ok(op) => op,
            Err(_) => return refused(StatusCode::BAD_REQUEST, "invalid_input"),
        }
    } else {
        if !to_bytes(request.into_body(), 1)
            .await
            .is_ok_and(|b| b.is_empty())
        {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        }
        if path.ends_with("original-system-backup-governance") {
            Operation::Governance(date)
        } else if path.ends_with("original-system-backup-destination-list") {
            Operation::List
        } else {
            return refused(StatusCode::NOT_FOUND, "not_found");
        }
    };
    if !op.confirmed() {
        return response(
            StatusCode::OK,
            json!({"ok":true,"result":{"ok":false,"code":"CONFIRM_REQUIRED","errors":["confirm: true required for write tool"],"appliedRules":["DK-BOOKKEEPING-BACKUP-001"]}}),
        );
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
                    .backup_views_original717(
                        &cookie,
                        &SystemClock,
                        &state.native.backup_destinations,
                        op,
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
                    .backup_views_original717_service(
                        &key,
                        &SystemClock,
                        &state.native.backup_destinations,
                        op,
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
