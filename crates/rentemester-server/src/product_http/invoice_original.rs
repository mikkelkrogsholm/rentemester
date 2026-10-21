//! Strict GET-only original invoice family on the same admitted sales owner.
use super::*;
use rentemester_storage::invoice_original::{Kind, Query};
fn parse(path: &str, raw: &str) -> Result<(Kind, Query), ()> {
    let kind = if path.ends_with("/list") {
        Kind::List
    } else if path.ends_with("/find") {
        Kind::Find
    } else if path.ends_with("/status") {
        Kind::Status
    } else if path.ends_with("/overdue") {
        Kind::Overdue
    } else {
        return Err(());
    };
    if raw.len() > 4096 {
        return Err(());
    }
    let bytes = raw.as_bytes();
    for (i, b) in bytes.iter().enumerate() {
        if *b == b'%'
            && (i + 2 >= bytes.len()
                || !bytes[i + 1].is_ascii_hexdigit()
                || !bytes[i + 2].is_ascii_hexdigit())
        {
            return Err(());
        }
    }
    let mut query = Query::default();
    let mut seen = std::collections::BTreeSet::new();
    for (key, value) in url::form_urlencoded::parse(bytes) {
        if !seen.insert(key.to_string())
            || value.is_empty()
            || value.len() > 1024
            || value.chars().any(char::is_control)
        {
            return Err(());
        }
        let allowed = match kind {
            Kind::List => matches!(
                key.as_ref(),
                "status"
                    | "from"
                    | "to"
                    | "customerCvr"
                    | "customer"
                    | "invoiceNumber"
                    | "minAmount"
                    | "maxAmount"
                    | "asOf"
            ),
            Kind::Find => matches!(
                key.as_ref(),
                "query"
                    | "customer"
                    | "invoiceNumber"
                    | "amount"
                    | "minAmount"
                    | "maxAmount"
                    | "asOf"
            ),
            Kind::Status => matches!(key.as_ref(), "documentId" | "invoiceNumber" | "asOf"),
            Kind::Overdue => matches!(key.as_ref(), "asOf" | "minDays"),
        };
        if !allowed {
            return Err(());
        }
        let value = value.into_owned();
        match key.as_ref() {
            "status" => {
                if !matches!(
                    value.as_str(),
                    "all"
                        | "open"
                        | "paid"
                        | "credited"
                        | "refunded"
                        | "overpaid"
                        | "written_off"
                        | "overdue"
                ) {
                    return Err(());
                }
                query.status = Some(value)
            }
            "from" => query.from = Some(value),
            "to" => query.to = Some(value),
            "customerCvr" => query.customer_cvr = Some(value),
            "customer" => query.customer = Some(value),
            "invoiceNumber" => query.invoice_number = Some(value),
            "query" => query.query = Some(value),
            "asOf" => query.as_of = Some(value),
            "documentId" => {
                if !value.bytes().all(|b| b.is_ascii_digit()) {
                    return Err(());
                }
                let n = value.parse().map_err(|_| ())?;
                if !(1..=9_007_199_254_740_991).contains(&n) {
                    return Err(());
                }
                query.document_id = Some(n)
            }
            "minDays" => {
                if !value.bytes().all(|b| b.is_ascii_digit()) {
                    return Err(());
                }
                let n = value.parse().map_err(|_| ())?;
                if n > 9_007_199_254_740_991 {
                    return Err(());
                }
                query.min_days = Some(n)
            }
            "amount" | "minAmount" | "maxAmount" => {
                let number: serde_json::Number = serde_json::from_str(&value).map_err(|_| ())?;
                if !number.as_f64().is_some_and(f64::is_finite) {
                    return Err(());
                }
                match key.as_ref() {
                    "amount" => query.amount = Some(number),
                    "minAmount" => query.min_amount = Some(number),
                    _ => query.max_amount = Some(number),
                }
            }
            _ => return Err(()),
        }
    }
    if matches!(kind, Kind::Status)
        && query.document_id.is_none()
        && query
            .invoice_number
            .as_deref()
            .is_none_or(|s| s.trim().is_empty())
    {
        return Err(());
    }
    for date in [&query.as_of, &query.from, &query.to].into_iter().flatten() {
        rentemester_engine::fiscal_year::Date::parse(date).map_err(|_| ())?;
    }
    Ok((kind, query))
}
pub(super) async fn handle(State(state): State<Product>, request: Request) -> Response {
    let credential = match documents::credential(&state, &request, false) {
        Ok(c) => c,
        Err(r) => return *r,
    };
    if !selected(&state, &request) {
        return refused(StatusCode::NOT_FOUND, "company_not_found");
    }
    if request.method() != axum::http::Method::GET
        || request.headers().contains_key("content-encoding")
        || request.headers().contains_key("transfer-encoding")
        || request.headers().contains_key("idempotency-key")
        || one(request.headers(), "content-length").is_some_and(|s| s != "0")
    {
        return refused(StatusCode::BAD_REQUEST, "invalid_query");
    }
    let path = request.uri().path().to_owned();
    let slug = path.split('/').nth(3).unwrap_or("").to_owned();
    let (kind, query) = match parse(&path, request.uri().query().unwrap_or("")) {
        Ok(q) => q,
        Err(()) => return refused(StatusCode::BAD_REQUEST, "invalid_query"),
    };
    if !matches!(to_bytes(request.into_body(),0).await,Ok(b)if b.is_empty()) {
        return refused(StatusCode::BAD_REQUEST, "body_not_allowed");
    }
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let mut owner = state.native.lock_company(&slug).map_err(|_| ())?;
        let action = rentemester_application::sales::Action::Original(kind, &query);
        match credential {
            documents::Credential::Human(cookie) => {
                let attempt = owner
                    .sales(&cookie, &SystemClock, action, "", "", None)
                    .map_err(|_| ())?;
                Ok::<_, ()>(documents::outcome(attempt))
            }
            documents::Credential::Service(key) => {
                let attempt = owner
                    .sales_service(
                        &key,
                        &SystemClock,
                        rentemester_application::sales::Request {
                            action,
                            today: "",
                            now: "",
                            year: None,
                        },
                    )
                    .map_err(|_| ())?;
                Ok(match attempt.outcome {
                    Ok(done) => Ok(done.result),
                    Err(f) => Err(Box::new(service_company::failure(f))),
                })
            }
        }
    })
    .await;
    match result {
        Ok(Ok(Ok(rentemester_application::sales::Output::Json(value)))) => {
            response(StatusCode::OK, value)
        }
        Ok(Ok(Err(r))) => *r,
        _ => refused(StatusCode::SERVICE_UNAVAILABLE, "unavailable"),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_invoice_query_fields_selectors_duplicates_and_numbers_are_strict() {
        let (_, q) = parse(
            "/status",
            "documentId=1&invoiceNumber=Other&asOf=2026-01-01",
        )
        .unwrap();
        assert_eq!(q.document_id, Some(1));
        assert_eq!(q.invoice_number.as_deref(), Some("Other"));
        let (_, q) = parse(
            "/find",
            "query=two+words&amount=125&minAmount=1&maxAmount=999",
        )
        .unwrap();
        assert_eq!(q.query.as_deref(), Some("two words"));
        assert_eq!(q.amount.unwrap().to_string(), "125");
        for (path, raw) in [
            ("/status", ""),
            ("/status", "documentId=0"),
            ("/status", "documentId=1&documentId=2"),
            ("/overdue", "minDays=-1"),
            ("/overdue", "customer=x"),
            ("/list", "query=x"),
            ("/find", "from=2026-01-01"),
            ("/list", "minAmount=NaN"),
            ("/list", "actor=user%3Au"),
            ("/list", "asOf=2026-02-30"),
            ("/list", "customer=%XX"),
        ] {
            assert!(parse(path, raw).is_err(), "{path}?{raw}");
        }
    }
}
