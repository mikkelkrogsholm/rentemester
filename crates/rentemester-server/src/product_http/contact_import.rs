//! Existing Contacts import, one confirmed bounded request; operator-only lazy CVR adapter.
use super::*;
use rentemester_engine::contact_import::Input;
use rentemester_runtime::contact_import::OperatorLookup;
fn parse(bytes: &[u8]) -> Result<Input, ()> {
    if bytes.len() > 12 * 1024 * 1024 {
        return Err(());
    }
    let input: Input = serde_json::from_slice(bytes).map_err(|_| ())?;
    if !input.confirm {
        return Err(());
    }
    Ok(input)
}
pub(super) async fn handle(State(state): State<Product>, request: Request) -> Response {
    let cookie = match proof(&state, &request, true) {
        Ok(p) => p,
        Err(r) => return *r,
    };
    if !selected(&state, &request) {
        return refused(StatusCode::NOT_FOUND, "company_not_found");
    }
    if request.uri().query().is_some()
        || request.headers().contains_key("content-encoding")
        || request.headers().contains_key("idempotency-key")
        || !one(request.headers(), "content-type")
            .is_some_and(|s| s.eq_ignore_ascii_case("application/json"))
    {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    }
    let slug = request
        .uri()
        .path()
        .split('/')
        .nth(3)
        .unwrap_or("")
        .to_owned();
    let bytes = match to_bytes(request.into_body(), 12 * 1024 * 1024).await {
        Ok(b) => b,
        Err(_) => return refused(StatusCode::PAYLOAD_TOO_LARGE, "invalid_input"),
    };
    let input = match parse(&bytes) {
        Ok(i) => i,
        Err(()) => return refused(StatusCode::BAD_REQUEST, "invalid_input"),
    };
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let mut owner = state.native.lock_company(&slug).map_err(|_| ())?;
        let mut lookup = OperatorLookup::default();
        owner
            .import_contacts(&cookie, &SystemClock, &input, &mut lookup)
            .map_err(|_| ())
    })
    .await;
    match result {
        Ok(Ok(attempt)) => {
            if let Err(f) = &attempt.outcome
                && f.stage == FailureStage::NotCommitted
            {
                if f.error.code == "access_denied" {
                    return refused(StatusCode::FORBIDDEN, "access_denied");
                }
                if f.error.code == "contact_import_invalid" {
                    return response(
                        StatusCode::BAD_REQUEST,
                        json!({"ok":false,"code":f.error.code,"errors":[f.error.message]}),
                    );
                }
                if matches!(
                    f.error.code,
                    "contact_import_storage_unavailable" | "contact_storage_unavailable"
                ) {
                    return response(
                        StatusCode::SERVICE_UNAVAILABLE,
                        json!({"ok":false,"code":f.error.code,"errors":[f.error.message],"outcome":"not_committed"}),
                    );
                }
            }
            match super::documents::outcome(attempt) {
                Ok(v) => response(StatusCode::OK, v),
                Err(r) => *r,
            }
        }
        _ => refused(StatusCode::SERVICE_UNAVAILABLE, "internal"),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn strict_bounded_body_cannot_supply_provider_or_actor() {
        assert!(parse(br#"{"fileName":"synthetic.csv","content":"Kontaktnavn;CVR-nummer;Kontakttype\nSynthetic;;Vendor","confirm":true}"#).is_ok());
        for body in [
            r#"{"fileName":"x","content":"x","confirm":false}"#,
            r#"{"fileName":"x","content":"x","confirm":true,"endpoint":"https://untrusted.invalid"}"#,
            r#"{"fileName":"x","content":"x","confirm":true,"actor":"user:guessed"}"#,
            r#"{"fileName":"x","content":"x","confirm":true,"enrichCvr":true,"enrichCvr":false}"#,
        ] {
            assert!(parse(body.as_bytes()).is_err());
        }
        assert!(parse(&vec![b' '; 12 * 1024 * 1024 + 1]).is_err());
    }
}
