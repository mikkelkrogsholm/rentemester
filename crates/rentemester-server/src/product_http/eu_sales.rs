//! Explicit VIES lookup and read-only EU sales preparation, never tax submission.
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
    let lookup = request.method() == axum::http::Method::POST;
    let cookie = match proof(&state, &request, lookup) {
        Ok(proof) => proof,
        Err(r) => return *r,
    };
    if !selected(&state, &request) {
        return refused(StatusCode::NOT_FOUND, "company_not_found");
    }
    let slug = request
        .uri()
        .path()
        .split('/')
        .nth(3)
        .unwrap_or("")
        .to_owned();
    if request.headers().contains_key("content-encoding") {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    }
    let (vat, period) = if lookup {
        if request.uri().query().is_some()
            || !one(request.headers(), "content-type")
                .is_some_and(|v| v.eq_ignore_ascii_case("application/json"))
        {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        }
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct Input {
            vat_or_cvr: String,
        }
        let Ok(bytes) = to_bytes(request.into_body(), 4096).await else {
            return refused(StatusCode::PAYLOAD_TOO_LARGE, "invalid_input");
        };
        let Ok(input) = serde_json::from_slice::<Input>(&bytes) else {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        };
        (Some(input.vat_or_cvr), None)
    } else {
        let Ok(period) = dates(request.uri().query().unwrap_or("")) else {
            return refused(StatusCode::BAD_REQUEST, "invalid_query");
        };
        match to_bytes(request.into_body(), 1).await {
            Ok(bytes) if bytes.is_empty() => {}
            _ => return refused(StatusCode::BAD_REQUEST, "invalid_input"),
        }
        (None, Some(period))
    };
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        if let Some((from, to)) = period {
            let Ok(mut owner) = state.native.lock_company(&slug) else {
                return refused(StatusCode::SERVICE_UNAVAILABLE, "owner_unavailable");
            };
            return match owner.eu_sales(&cookie, &SystemClock, from, to) {
                Ok(attempt) => match committed(attempt) {
                    Ok(value) => response(StatusCode::OK, value),
                    Err(r) => *r,
                },
                Err(_) => refused(StatusCode::SERVICE_UNAVAILABLE, "owner_unavailable"),
            };
        }
        let admission = {
            let Ok(mut owner) = state.native.lock_company(&slug) else {
                return refused(StatusCode::SERVICE_UNAVAILABLE, "owner_unavailable");
            };
            match owner.start_vies(&cookie, &SystemClock, vat.as_deref().unwrap()) {
                Ok(attempt) => match committed(attempt) {
                    Ok(value) => value,
                    Err(r) => return *r,
                },
                Err(_) => return refused(StatusCode::SERVICE_UNAVAILABLE, "owner_unavailable"),
            }
        };
        // The ordinary explicit action alone calls VIES. No Owner or SQL is held.
        let observation = match state.native.vies.validate(admission.vat()) {
            Ok(value) => value,
            Err(error) => return refused(StatusCode::SERVICE_UNAVAILABLE, error.code()),
        };
        let Ok(mut owner) = state.native.lock_company(&slug) else {
            return refused(StatusCode::SERVICE_UNAVAILABLE, "owner_unavailable");
        };
        match owner.finish_vies(&cookie, &SystemClock, admission, &observation) {
            Ok(attempt) => match committed(attempt) {
                Ok(value) => response(StatusCode::OK, value),
                Err(r) => *r,
            },
            Err(_) => refused(StatusCode::SERVICE_UNAVAILABLE, "owner_unavailable"),
        }
    })
    .await;
    result.unwrap_or_else(|_| refused(StatusCode::SERVICE_UNAVAILABLE, "operation_unconfirmed"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn eu_sales_dates_require_exact_unique_valid_bounds() {
        assert!(dates("from=2026-01-01&to=2026-03-31").is_ok());
        for value in [
            "",
            "from=2026-01-01",
            "from=2026-03-31&to=2026-01-01",
            "from=2026-02-30&to=2026-03-31",
            "from=2026-01-01&from=2026-01-02&to=2026-03-31",
            "from=2026-01-01&to=2026-03-31&other=x",
            "from=%XX&to=2026-03-31",
        ] {
            assert!(dates(value).is_err(), "{value}");
        }
    }
}
