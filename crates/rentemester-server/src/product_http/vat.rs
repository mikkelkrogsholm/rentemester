use super::*;
pub(super) async fn read(State(state): State<Product>, request: Request) -> Response {
    let cookie = match proof(&state, &request, false) {
        Ok(p) => p,
        Err(r) => return *r,
    };
    if !selected(&state, &request) {
        return refused(StatusCode::NOT_FOUND, "company_not_found");
    }
    let query = match query(&request) {
        Ok(q) => q,
        Err(r) => return *r,
    };
    let export = request.uri().path().ends_with("/export");
    if query.account.is_some()
        || (!export && query.format.is_some())
        || (export && query.format.as_deref() != Some("pdf"))
    {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    }
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let today = chrono::DateTime::<chrono::Utc>::from(std::time::SystemTime::now())
        .format("%Y-%m-%d")
        .to_string();
    let export_date = today.clone();
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let mut owner = state.native.owner.lock().map_err(|_| ())?;
        owner
            .vat_report(&cookie, &SystemClock, &today, query.year.as_deref())
            .map_err(|_| ())
    })
    .await;
    let value = match result {
        Ok(Ok(attempt)) => match super::documents::outcome(attempt) {
            Ok(value) => value,
            Err(r) => return *r,
        },
        _ => return refused(StatusCode::SERVICE_UNAVAILABLE, "read_unconfirmed"),
    };
    if !export {
        return response(StatusCode::OK, json!({"ok":true,"vat":value}));
    }
    let Some(export) = crate::report_export::build("vat", "pdf", &value, &export_date) else {
        return refused(StatusCode::UNPROCESSABLE_ENTITY, "vat_export_unavailable");
    };
    let disposition = format!("attachment; filename*=UTF-8''{}", export.filename);
    let mut reply = (StatusCode::OK, export.bytes).into_response();
    reply.headers_mut().insert(
        "content-type",
        axum::http::HeaderValue::from_static(export.content_type),
    );
    reply.headers_mut().insert(
        "content-disposition",
        axum::http::HeaderValue::from_str(&disposition).expect("sanitized filename"),
    );
    reply.headers_mut().insert(
        "cache-control",
        axum::http::HeaderValue::from_static("private, no-store"),
    );
    reply
}
