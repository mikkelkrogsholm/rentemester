//! Read-only archive preparation with explicit confirmation; no output/restore paths from HTTP.
use super::*;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    confirm: bool,
}
pub(super) async fn handle(State(state): State<Product>, request: Request) -> Response {
    let cookie = match proof(&state, &request, true) {
        Ok(p) => p,
        Err(r) => return *r,
    };
    if request.uri().query().is_some()
        || request.headers().contains_key("content-encoding")
        || !one(request.headers(), "content-type")
            .is_some_and(|s| s.eq_ignore_ascii_case("application/json"))
    {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    }
    let Ok(bytes) = to_bytes(request.into_body(), 1024).await else {
        return refused(StatusCode::PAYLOAD_TOO_LARGE, "invalid_input");
    };
    if !serde_json::from_slice::<Input>(&bytes).is_ok_and(|v| v.confirm) {
        return refused(StatusCode::BAD_REQUEST, "confirmation_required");
    }
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let Some(key) = state.native.backup_signer.as_ref() else {
            return Err(());
        };
        state
            .native
            .owner
            .lock()
            .map_err(|_| ())?
            .backup_workspace(&state.native.cookies, &SystemClock, &cookie, key)
            .map_err(|_| ())
    })
    .await;
    match result {
        Ok(Ok(attempt)) => match attempt.outcome {
            Ok(bytes) => {
                let mut r = (StatusCode::OK, bytes).into_response();
                r.headers_mut().insert(
                    "content-type",
                    axum::http::HeaderValue::from_static(
                        "application/vnd.rentemester.native-backup+json",
                    ),
                );
                r.headers_mut().insert(
                    "content-disposition",
                    axum::http::HeaderValue::from_static(
                        "attachment; filename=workspace.native-backup.json",
                    ),
                );
                r
            }
            Err(e) => response(
                if e.error.code == "access_denied" {
                    StatusCode::FORBIDDEN
                } else {
                    StatusCode::UNPROCESSABLE_ENTITY
                },
                json!({"ok":false,"code":e.error.code,"errors":[e.error.message]}),
            ),
        },
        _ => refused(StatusCode::SERVICE_UNAVAILABLE, "backup_unavailable"),
    }
}
