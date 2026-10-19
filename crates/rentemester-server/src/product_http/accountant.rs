use super::*;
pub(super) async fn export(State(state): State<Product>, request: Request) -> Response {
    let cookie = match proof(&state, &request, true) {
        Ok(c) => c,
        Err(r) => return *r,
    };
    if !selected(&state, &request) {
        return refused(StatusCode::NOT_FOUND, "company_not_found");
    }
    if request.uri().query().is_some()
        || request.headers().contains_key("content-encoding")
        || !one(request.headers(), "content-type")
            .is_some_and(|s| s.eq_ignore_ascii_case("application/json"))
    {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    }
    let Ok(bytes) = to_bytes(request.into_body(), 4096).await else {
        return refused(StatusCode::PAYLOAD_TOO_LARGE, "invalid_input");
    };
    let Ok(input) = serde_json::from_slice::<rentemester_storage::accountant::Input>(&bytes) else {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    };
    let filename = format!(
        "revisor-eksport-{}-{}-{}.tar",
        state.native.catalog.companies[0].slug, input.period_start, input.period_end
    );
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let mut owner = state.native.owner.lock().map_err(|_| ())?;
        owner
            .accountant_export(&cookie, &SystemClock, &input)
            .map_err(|_| ())
    })
    .await;
    let exported = match result {
        Ok(Ok(attempt)) => match super::documents::outcome(attempt) {
            Ok(value) => value,
            Err(r) => return *r,
        },
        _ => return refused(StatusCode::SERVICE_UNAVAILABLE, "outcome_unconfirmed"),
    };
    let mut reply = (StatusCode::OK, exported.bytes).into_response();
    for (name, value) in [
        ("content-type", "application/x-tar".to_owned()),
        (
            "content-disposition",
            format!(
                "attachment; filename*=UTF-8''{}",
                percent_encoding::utf8_percent_encode(
                    &filename,
                    percent_encoding::NON_ALPHANUMERIC
                )
            ),
        ),
        ("x-content-type-options", "nosniff".to_owned()),
        ("cache-control", "private, no-store".to_owned()),
        (
            "x-rentemester-journal-entries",
            exported.journal_entry_count.to_string(),
        ),
        (
            "x-rentemester-documents",
            exported.document_count.to_string(),
        ),
        (
            "x-rentemester-bank-transactions",
            exported.bank_transaction_count.to_string(),
        ),
    ] {
        reply.headers_mut().insert(
            axum::http::HeaderName::from_static(name),
            value
                .parse()
                .expect("constant or encoded accountant export header"),
        );
    }
    reply
}
