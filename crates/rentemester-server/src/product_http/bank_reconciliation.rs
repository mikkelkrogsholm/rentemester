use super::*;
use rentemester_application::bank_reconciliation::Action;
enum Input {
    Link(rentemester_storage::bank_reconciliation::Apply),
    Correct(rentemester_storage::bank_reconciliation::correction::Apply),
}
pub(super) async fn handle(State(state): State<Product>, request: Request) -> Response {
    let company_slug=request.uri().path().split('/').nth(3).unwrap_or("").to_owned();
    let mutation = request.method() == axum::http::Method::POST;
    let cookie = match proof(&state, &request, mutation) {
        Ok(v) => v,
        Err(r) => return *r,
    };
    if !selected(&state, &request) {
        return refused(StatusCode::NOT_FOUND, "company_not_found");
    }
    let correction = request.uri().path().contains("reconciliation-correction");
    let mut bank = None;
    let mut replacement = None;
    for (key, value) in url::form_urlencoded::parse(request.uri().query().unwrap_or("").as_bytes())
    {
        let slot = match key.as_ref() {
            "bankTransactionId" => &mut bank,
            "replacementJournalEntryId" if correction => &mut replacement,
            _ => return refused(StatusCode::BAD_REQUEST, "invalid_query"),
        };
        if mutation || slot.is_some() || !value.bytes().all(|b| b.is_ascii_digit()) {
            return refused(StatusCode::BAD_REQUEST, "invalid_query");
        }
        *slot = value
            .parse::<i64>()
            .ok()
            .filter(|n| *n > 0 && *n <= 9_007_199_254_740_991);
        if slot.is_none() {
            return refused(StatusCode::BAD_REQUEST, "invalid_query");
        }
    }
    if !mutation && (bank.is_none() || (correction && replacement.is_none())) {
        return refused(StatusCode::BAD_REQUEST, "invalid_query");
    }
    let input = if mutation {
        if request.uri().query().is_some()
            || request.headers().contains_key("content-encoding")
            || !one(request.headers(), "content-type")
                .is_some_and(|v| v.eq_ignore_ascii_case("application/json"))
        {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        }
        let Ok(bytes) = to_bytes(request.into_body(), 16 * 1024).await else {
            return refused(StatusCode::PAYLOAD_TOO_LARGE, "invalid_input");
        };
        let parsed = if correction {
            serde_json::from_slice(&bytes).map(Input::Correct)
        } else {
            serde_json::from_slice(&bytes).map(Input::Link)
        };
        match parsed {
            Ok(v) => Some(v),
            Err(_) => return refused(StatusCode::BAD_REQUEST, "invalid_input"),
        }
    } else {
        None
    };
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let mut owner = state.native.lock_company(&company_slug).map_err(|_| ())?;
        let action = match &input {
            Some(Input::Link(i)) => Action::Apply(i),
            Some(Input::Correct(i)) => Action::Correct(i),
            None if correction => Action::CorrectionPlan {
                bank: bank.unwrap(),
                replacement: replacement.unwrap(),
            },
            None => Action::Candidates(bank.unwrap()),
        };
        owner
            .bank_reconciliation(&cookie, &SystemClock, action)
            .map_err(|_| ())
    })
    .await;
    match result {
        Ok(Ok(attempt)) => match super::documents::outcome(attempt) {
            Ok(v) => response(
                StatusCode::OK,
                json!({"ok":true,if correction {if mutation {"correction"} else {"plan"}} else if mutation {"reconciliation"}else{"reconciliationCandidates"}:v}),
            ),
            Err(r) => *r,
        },
        _ => refused(
            StatusCode::SERVICE_UNAVAILABLE,
            if mutation {
                "outcome_unconfirmed"
            } else {
                "read_unconfirmed"
            },
        ),
    }
}
