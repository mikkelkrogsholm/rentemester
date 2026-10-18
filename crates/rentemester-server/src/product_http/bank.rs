use super::*;
use rentemester_application::bank::{AccountInput, Action};
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Import {
    confirm: bool,
    csv_content: String,
    account: Option<String>,
    profile: Option<String>,
}
enum Command {
    Accounts,
    Read(Option<String>),
    Create(AccountInput),
    Import(Import),
}
pub(super) async fn handle(State(state): State<Product>, request: Request) -> Response {
    let mutation = request.method() == axum::http::Method::POST;
    let cookie = match proof(&state, &request, mutation) {
        Ok(p) => p,
        Err(r) => return *r,
    };
    if !selected(&state, &request) {
        return refused(StatusCode::NOT_FOUND, "company_not_found");
    }
    let path = request.uri().path().to_owned();
    let query = if mutation {
        if request.uri().query().is_some() {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        }
        None
    } else {
        match super::query(&request) {
            Ok(q) => Some(q),
            Err(r) => return *r,
        }
    };
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let command = if mutation {
        if request.headers().contains_key("content-encoding")
            || !one(request.headers(), "content-type")
                .is_some_and(|s| s.eq_ignore_ascii_case("application/json"))
        {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        }
        let Ok(bytes) = to_bytes(request.into_body(), 35 * 1024 * 1024).await else {
            return refused(StatusCode::PAYLOAD_TOO_LARGE, "upload_too_large");
        };
        if path.ends_with("/bank/import") {
            let Ok(input) = serde_json::from_slice::<Import>(&bytes) else {
                return refused(StatusCode::BAD_REQUEST, "invalid_input");
            };
            if !input.confirm {
                return refused(StatusCode::BAD_REQUEST, "confirmation_required");
            }
            if input.profile.as_ref().is_some_and(|p| !p.trim().is_empty()) {
                return response(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    json!({"ok":false,"code":"native_profile_unavailable","errors":["Named bank profiles are not ported yet; use generic UTF-8 CSV with ISO dates"]}),
                );
            }
            Command::Import(input)
        } else {
            let Ok(input) = serde_json::from_slice(&bytes) else {
                return refused(StatusCode::BAD_REQUEST, "invalid_input");
            };
            Command::Create(input)
        }
    } else if path.ends_with("/bank-accounts") {
        if request.uri().query().is_some() {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        }
        Command::Accounts
    } else {
        let q = query.unwrap();
        if q.account.is_some() || q.format.is_some() {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        }
        Command::Read(q.year)
    };
    let key = match command {
        Command::Accounts => "bankAccounts",
        Command::Read(_) => "bank",
        Command::Create(_) => "bankAccount",
        Command::Import(_) => "import",
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let now = chrono::DateTime::<chrono::Utc>::from(std::time::SystemTime::now());
        let today = now.format("%Y-%m-%d").to_string();
        let timestamp = now.to_rfc3339();
        let action = match &command {
            Command::Accounts => Action::Accounts,
            Command::Read(year) => Action::Read {
                today: &today,
                year: year.as_deref(),
            },
            Command::Create(input) => Action::Create(input),
            Command::Import(input) => Action::Import {
                content: &input.csv_content,
                account: input.account.as_deref(),
                now: &timestamp,
            },
        };
        let mut owner = state.native.owner.lock().map_err(|_| ())?;
        owner.bank(&cookie, &SystemClock, action).map_err(|_| ())
    })
    .await;
    match result {
        Ok(Ok(attempt)) => match super::documents::outcome(attempt) {
            Ok(value) => response(StatusCode::OK, json!({"ok":true,key:value})),
            Err(r) => *r,
        },
        _ => refused(StatusCode::SERVICE_UNAVAILABLE, "outcome_unconfirmed"),
    }
}
