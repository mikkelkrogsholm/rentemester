//! Normal workspace creation; same origin, current workspace authority, no retry.
use super::*;
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
    let Ok(bytes) = to_bytes(request.into_body(), 16 * 1024).await else {
        return refused(StatusCode::PAYLOAD_TOO_LARGE, "invalid_input");
    };
    let Ok(input) =
        serde_json::from_slice::<rentemester_application::company_creation::Input>(&bytes)
    else {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    };
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        state
            .native
            .owner
            .lock()
            .map_err(|_| ())?
            .create_company(&state.native.cookies, &SystemClock, &cookie, &input)
            .map_err(|_| ())
    })
    .await;
    match result {
        Ok(Ok(a)) => match a.outcome {
            Ok(company) => response(StatusCode::CREATED, json!({"ok":true,"company":company})),
            Err(f)
                if f.stage
                    == rentemester_application::password_login::FailureStage::OutcomeUnknown =>
            {
                refused(StatusCode::SERVICE_UNAVAILABLE, "outcome_unconfirmed")
            }
            Err(f) => refused(
                match f.error.code {
                    "company_exists" => StatusCode::CONFLICT,
                    "access_denied" => StatusCode::FORBIDDEN,
                    "invalid_credentials" => StatusCode::UNAUTHORIZED,
                    _ => StatusCode::UNPROCESSABLE_ENTITY,
                },
                f.error.code,
            ),
        },
        _ => refused(StatusCode::SERVICE_UNAVAILABLE, "outcome_unconfirmed"),
    }
}
