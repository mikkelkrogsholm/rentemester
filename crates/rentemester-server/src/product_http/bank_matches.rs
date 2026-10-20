use super::*;
use rentemester_application::bank_matches::Input;

fn selectors(query: Option<&str>) -> Result<Input, ()> {
    let query = query.unwrap_or("");
    if query.len() > 256
        || query.bytes().enumerate().any(|(i, b)| {
            b == b'%'
                && (i + 2 >= query.len()
                    || !query.as_bytes()[i + 1].is_ascii_hexdigit()
                    || !query.as_bytes()[i + 2].is_ascii_hexdigit())
        })
    {
        return Err(());
    }
    let mut input = Input {
        bank_transaction_id: None,
        max: None,
    };
    for (key, value) in url::form_urlencoded::parse(query.as_bytes()) {
        if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
            return Err(());
        }
        match key.as_ref() {
            "bankTransactionId" if input.bank_transaction_id.is_none() => {
                input.bank_transaction_id = Some(
                    value
                        .parse::<i64>()
                        .ok()
                        .filter(|id| *id > 0 && *id <= 9_007_199_254_740_991)
                        .ok_or(())?,
                );
            }
            "max" if input.max.is_none() => {
                input.max = Some(
                    value
                        .parse::<usize>()
                        .ok()
                        .filter(|max| *max > 0)
                        .ok_or(())?,
                );
            }
            _ => return Err(()),
        }
    }
    Ok(input)
}
pub(super) async fn handle(State(state): State<Product>, request: Request) -> Response {
    let slug = request
        .uri()
        .path()
        .split('/')
        .nth(3)
        .unwrap_or("")
        .to_owned();
    let cookie = match proof(&state, &request, false) {
        Ok(cookie) => cookie,
        Err(r) => return *r,
    };
    if !selected(&state, &request) {
        return refused(StatusCode::NOT_FOUND, "company_not_found");
    }
    let input = match selectors(request.uri().query()) {
        Ok(input) => input,
        Err(()) => return refused(StatusCode::BAD_REQUEST, "invalid_query"),
    };
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let mut owner = state.native.lock_company(&slug).map_err(|_| ())?;
        owner
            .bank_matches(&cookie, &SystemClock, &input)
            .map_err(|_| ())
    })
    .await;
    match result {
        Ok(Ok(attempt)) => match super::documents::outcome(attempt) {
            Ok(value) => response(StatusCode::OK, value),
            Err(r) => *r,
        },
        _ => refused(StatusCode::SERVICE_UNAVAILABLE, "outcome_unconfirmed"),
    }
}
#[cfg(test)]
mod tests {
    use super::selectors;
    #[test]
    fn selectors_are_exact_unique_positive_and_max_is_per_row_not_a_row_cap() {
        let input = selectors(Some("bankTransactionId=7&max=12")).unwrap();
        assert_eq!(input.bank_transaction_id, Some(7));
        assert_eq!(input.max, Some(12));
        let defaults = selectors(None).unwrap();
        assert!(defaults.max.is_none() && defaults.bank_transaction_id.is_none());
        assert!(selectors(Some("max=100000")).is_ok());
        for q in [
            "max=0",
            "max=-1",
            "max=1.5",
            "max=1&max=2",
            "bankTransactionId=0",
            "bankTransactionId=9007199254740992",
            "bankTransactionId=1&bankTransactionId=2",
            "actor=agent:x",
            "year=2026",
            "bankTransactionId=%ZZ",
            "max=%2B5",
            "max=",
        ] {
            assert!(selectors(Some(q)).is_err(), "{q}");
        }
    }
}
