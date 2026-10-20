//! Dated read-only OSS base report. No VAT computation or tax submission.
use super::*;
use rentemester_engine::fiscal_year::Date;

fn dates(query: &str) -> Result<(Date, Date), ()> {
    if query.len() > 128
        || query.bytes().enumerate().any(|(i, b)| {
            b == b'%'
                && (i + 2 >= query.len()
                    || !query.as_bytes()[i + 1].is_ascii_hexdigit()
                    || !query.as_bytes()[i + 2].is_ascii_hexdigit())
        })
    {
        return Err(());
    }
    let mut from = None;
    let mut to = None;
    for (key, value) in url::form_urlencoded::parse(query.as_bytes()) {
        let slot = match key.as_ref() {
            "from" => &mut from,
            "to" => &mut to,
            _ => return Err(()),
        };
        if slot.is_some() {
            return Err(());
        }
        *slot = Some(Date::parse(&value).map_err(|_| ())?);
    }
    match (from, to) {
        (Some(from), Some(to)) if from <= to => Ok((from, to)),
        _ => Err(()),
    }
}
fn committed<T>(
    attempt: rentemester_host::Attempt<
        Result<
            rentemester_application::control_fence::Committed<T>,
            rentemester_application::control_fence::Failure,
        >,
    >,
) -> Result<T, Box<Response>> {
    if attempt.retired {
        return Err(Box::new(refused(
            StatusCode::SERVICE_UNAVAILABLE,
            "owner_unavailable",
        )));
    }
    super::documents::outcome(attempt)
}
pub(super) async fn handle(State(state): State<Product>, request: Request) -> Response {
    let cookie = match proof(&state, &request, false) {
        Ok(proof) => proof,
        Err(r) => return *r,
    };
    if !selected(&state, &request) {
        return refused(StatusCode::NOT_FOUND, "company_not_found");
    }
    if request.headers().contains_key("content-encoding") {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    }
    let slug = request
        .uri()
        .path()
        .split('/')
        .nth(3)
        .unwrap_or("")
        .to_owned();
    let Ok((from, to)) = dates(request.uri().query().unwrap_or("")) else {
        return refused(StatusCode::BAD_REQUEST, "invalid_query");
    };
    match to_bytes(request.into_body(), 1).await {
        Ok(bytes) if bytes.is_empty() => {}
        _ => return refused(StatusCode::BAD_REQUEST, "invalid_input"),
    }
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let Ok(mut owner) = state.native.lock_company(&slug) else {
            return refused(StatusCode::SERVICE_UNAVAILABLE, "owner_unavailable");
        };
        match owner.oss_report(&cookie, &SystemClock, from, to) {
            Ok(attempt) => match committed(attempt) {
                Ok(value) => response(StatusCode::OK, value),
                Err(r) => *r,
            },
            Err(_) => refused(StatusCode::SERVICE_UNAVAILABLE, "owner_unavailable"),
        }
    })
    .await
    .unwrap_or_else(|_| refused(StatusCode::SERVICE_UNAVAILABLE, "operation_unconfirmed"))
}

#[cfg(test)]
mod tests;
