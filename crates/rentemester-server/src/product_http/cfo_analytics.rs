//! Exact existing CFO selectors; read failures never become uncertain mutations.
use super::*;
use rentemester_engine::cfo_analytics::Query;
fn parse(raw: &str) -> Result<Query, ()> {
    if raw.len() > 16 * 1024 {
        return Err(());
    }
    let b = raw.as_bytes();
    for (i, c) in b.iter().enumerate() {
        if *c == b'%'
            && (i + 2 >= b.len() || !b[i + 1].is_ascii_hexdigit() || !b[i + 2].is_ascii_hexdigit())
        {
            return Err(());
        }
    }
    let mut values = std::collections::BTreeMap::new();
    let mut companies = vec![];
    for (key, value) in url::form_urlencoded::parse(b) {
        if key.contains('\u{fffd}') || value.contains('\u{fffd}') {
            return Err(());
        }
        let value = value.trim().to_owned();
        if key == "companySlug" {
            if value.is_empty() || companies.len() >= 1000 {
                return Err(());
            }
            companies.push(value);
            continue;
        }
        if ![
            "scope",
            "from",
            "to",
            "groupProfileId",
            "account",
            "party",
            "dimension",
            "currency",
            "aggregate",
            "cursor",
            "limit",
        ]
        .contains(&key.as_ref())
            || value.is_empty()
            || values.insert(key.into_owned(), value).is_some()
        {
            return Err(());
        }
    }
    let scope = values.remove("scope").ok_or(())?;
    if scope == "company" && companies.len() != 1 {
        return Err(());
    }
    let limit = match values.remove("limit") {
        Some(s) if s.bytes().all(|c| c.is_ascii_digit()) && !s.starts_with('0') => {
            s.parse().map_err(|_| ())?
        }
        None => 100,
        _ => return Err(()),
    };
    let query = Query {
        scope,
        from: values.remove("from").ok_or(())?,
        to: values.remove("to").ok_or(())?,
        company_slugs: companies,
        group_profile_id: values.remove("groupProfileId"),
        account: values.remove("account"),
        party: values.remove("party"),
        dimension: values.remove("dimension"),
        currency: values.remove("currency"),
        aggregate: values.remove("aggregate").unwrap_or_else(|| "none".into()),
        cursor: values.remove("cursor"),
        limit,
    };
    query.validate().map_err(|_| ())?;
    Ok(query)
}
pub(super) async fn handle(State(state): State<Product>, request: Request) -> Response {
    let cookie = match proof(&state, &request, false) {
        Ok(p) => p,
        Err(r) => return *r,
    };
    if request.method() != axum::http::Method::GET
        || request.headers().contains_key("content-encoding")
        || request.headers().contains_key("transfer-encoding")
        || request.headers().contains_key("idempotency-key")
        || one(request.headers(), "content-length").is_some_and(|v| v != "0")
    {
        return refused(StatusCode::BAD_REQUEST, "invalid_query");
    }
    let query = match parse(request.uri().query().unwrap_or("")) {
        Ok(q) => q,
        Err(()) => return refused(StatusCode::BAD_REQUEST, "invalid_query"),
    };
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let mut owner = state.native.owner.lock().map_err(|_| ())?;
        owner
            .cfo_analytics(&state.native.cookies, &SystemClock, &cookie, &query)
            .map_err(|_| ())
    })
    .await;
    match result {
        Ok(Ok(attempt)) => match attempt.outcome {
            Ok(mut value) => {
                value["ok"] = json!(true);
                response(StatusCode::OK, value)
            }
            Err(f) => refused(
                if matches!(f.error.code, "access_denied" | "invalid_credentials") {
                    StatusCode::FORBIDDEN
                } else if matches!(f.error.code, "cfo_analytics_invalid" | "invalid_date") {
                    StatusCode::BAD_REQUEST
                } else {
                    StatusCode::SERVICE_UNAVAILABLE
                },
                f.error.code,
            ),
        },
        _ => refused(StatusCode::SERVICE_UNAVAILABLE, "cfo_source_unavailable"),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_query_repeats_only_companies_and_rejects_ambiguous_inputs() {
        assert!(parse("scope=portfolio&from=2026-01-01&to=2026-12-31&companySlug=a&companySlug=b&account=1000&aggregate=sum&limit=1").is_ok());
        for raw in [
            "scope=company&from=2026-01-01&to=2026-12-31",
            "scope=company&from=2026-01-01&to=2026-12-31&companySlug=a&companySlug=b",
            "scope=portfolio&from=2026-02-30&to=2026-12-31",
            "scope=portfolio&from=2026-01-01&to=2026-12-31&limit=01",
            "scope=portfolio&from=2026-01-01&to=2026-12-31&account=1&account=2",
            "scope=portfolio&from=2026-01-01&to=2026-12-31&actor=user:a",
            "scope=portfolio&from=2026-01-01&to=2026-12-31&cursor=%xx",
            "scope=portfolio&from=2026-01-01&to=2026-12-31&currency=%FF",
        ] {
            assert!(parse(raw).is_err(), "{raw}");
        }
    }
}
