use super::*;
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SourceInput {
    confirm: Option<bool>,
    digisense_company_key: Option<String>,
    limit: Option<u32>,
    max_timestamp: Option<String>,
    metadata: Option<Value>,
    force: Option<bool>,
}
pub(super) async fn handle(State(state): State<Product>, request: Request) -> Response {
    if request.method() != axum::http::Method::POST {
        return refused(StatusCode::METHOD_NOT_ALLOWED, "method_not_allowed");
    }
    if service_company::requested(&request) {
        return refused(StatusCode::UNAUTHORIZED, "human_cookie_required");
    }
    let cookie = match proof(&state, &request, true) {
        Ok(c) => c,
        Err(r) => return *r,
    };
    let workspace = request.uri().path() == "/api/workspace/original-efaktura-modtag";
    let slug = request
        .uri()
        .path()
        .split('/')
        .nth(3)
        .unwrap_or("")
        .to_owned();
    if !workspace && !selected(&state, &request) {
        return refused(StatusCode::NOT_FOUND, "company_not_found");
    }
    if request.uri().query().is_some()
        || request.headers().contains_key("content-encoding")
        || !one(request.headers(), "content-type")
            .is_some_and(|v| v.eq_ignore_ascii_case("application/json"))
    {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    }
    let Ok(bytes) = to_bytes(request.into_body(), 16384).await else {
        return refused(StatusCode::PAYLOAD_TOO_LARGE, "invalid_input");
    };
    let Ok(source) = serde_json::from_slice::<SourceInput>(&bytes) else {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    };
    if workspace
        && serde_json::from_slice::<Value>(&bytes)
            .ok()
            .and_then(|v| v.as_object().map(|o| o.keys().any(|k| k != "confirm")))
            .unwrap_or(true)
    {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    }
    if source.confirm != Some(true) {
        return response(
            StatusCode::OK,
            json!({"ok":true,"result":{"ok":false,"errors":["confirm: true is required"],"code":"CONFIRM_REQUIRED"}}),
        );
    }
    let input = Input {
        confirm: true,
        company_key: source.digisense_company_key,
        limit: source.limit,
        max_timestamp: source.max_timestamp,
        metadata: source.metadata,
        ingest_options: Some(IngestOptions {
            force_duplicate_logical_identity: source.force.unwrap_or(false),
        }),
        max_pages: None,
    };
    if !valid(&input) {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    }
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    match tokio::task::spawn_blocking(move || {
        let _permit = permit;
        if workspace {
            workspace_response(&state, &cookie, &input)
        } else {
            response(
                StatusCode::OK,
                json!({"ok":true,"result":poll(&state,&slug,&cookie,&input)}),
            )
        }
    })
    .await
    {
        Ok(r) => r,
        Err(_) => refused(StatusCode::SERVICE_UNAVAILABLE, "outcome_unknown"),
    }
}
fn workspace_response(state: &Product, cookie: &VerifiedCookie, input: &Input) -> Response {
    // Reuse all-target preflight and the admitted per-company workflow.
    workspace_result(state, cookie, input, true)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original717_digisense_receive_flat_source_mapping() {
        let v = json!({"digisenseCompanyKey":"synthetic-key","limit":2,"maxTimestamp":"2026-01-01T00:00:00Z","metadata":{"invoiceNo":"synthetic"},"force":true,"confirm":true});
        let source: SourceInput = serde_json::from_value(v).unwrap();
        let input = Input {
            confirm: source.confirm == Some(true),
            company_key: source.digisense_company_key,
            limit: source.limit,
            max_timestamp: source.max_timestamp,
            metadata: source.metadata,
            ingest_options: Some(IngestOptions {
                force_duplicate_logical_identity: source.force == Some(true),
            }),
            max_pages: None,
        };
        assert!(valid(&input));
        assert_eq!(input.company_key.as_deref(), Some("synthetic-key"));
        assert!(
            input
                .ingest_options
                .unwrap()
                .force_duplicate_logical_identity
        );
        for bad in [
            json!({"confirm":"true"}),
            json!({"confirm":true,"body":"{}"}),
            json!({"confirm":true,"companyKey":"native-field"}),
        ] {
            assert!(serde_json::from_value::<SourceInput>(bad).is_err());
        }
        assert!(
            serde_json::from_value::<SourceInput>(json!({}))
                .unwrap()
                .confirm
                .is_none()
        );
        assert_eq!(
            serde_json::from_value::<SourceInput>(json!({"confirm":false}))
                .unwrap()
                .confirm,
            Some(false)
        );
    }
}
