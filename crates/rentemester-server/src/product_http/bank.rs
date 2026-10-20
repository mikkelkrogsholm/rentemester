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
    let company_slug = request
        .uri()
        .path()
        .split('/')
        .nth(3)
        .unwrap_or("")
        .to_owned();
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
                profile: input.profile.as_deref(),
                now: &timestamp,
            },
        };
        let mut owner = state.native.lock_company(&company_slug).map_err(|_| ())?;
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn named_profile_is_preserved_exactly_and_not_a_transport_authority() {
        let input: Import=serde_json::from_value(json!({"confirm":true,"csvContent":"Dato;Tekst;Beløb\n03.04.2026;Synthetic;-1,00\n","account":"synthetic","profile":" DANSKE-BANK "})).unwrap();
        assert!(input.confirm);
        assert_eq!(input.account.as_deref(), Some("synthetic"));
        assert_eq!(input.profile.as_deref(), Some(" DANSKE-BANK "));
        // Engine owns named-profile admission. Unknown nonempty values must
        // reach it unchanged rather than silently becoming generic imports.
        let unknown: Import = serde_json::from_value(
            json!({"confirm":true,"csvContent":"source","profile":"unknown-bank"}),
        )
        .unwrap();
        assert_eq!(unknown.profile.as_deref(), Some("unknown-bank"));
        for body in [
            r#"{"confirm":true,"csvContent":"source","profile":"danske-bank","profile":"other"}"#,
            r#"{"confirm":true,"csvContent":"source","actor":"user:forged"}"#,
            r#"{"confirm":true,"csvContent":"source","profile":true}"#,
        ] {
            assert!(serde_json::from_str::<Import>(body).is_err());
        }
    }
    #[test]
    fn generic_none_empty_and_null_profiles_keep_existing_body_contract() {
        for body in [
            json!({"confirm":true,"csvContent":"source"}),
            json!({"confirm":true,"csvContent":"source","profile":null}),
            json!({"confirm":true,"csvContent":"source","profile":""}),
        ] {
            let input: Import = serde_json::from_value(body).unwrap();
            assert!(input.profile.as_deref().is_none_or(str::is_empty));
        }
        let input: Import = serde_json::from_value(
            json!({"confirm":false,"csvContent":"source","profile":"danske-bank"}),
        )
        .unwrap();
        assert!(!input.confirm);
    }
}
