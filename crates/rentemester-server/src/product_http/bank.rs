use super::*;
use base64::{Engine, engine::general_purpose::STANDARD};
use rentemester_application::bank::{AccountInput, Action};
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Import {
    confirm: bool,
    #[serde(default, deserialize_with = "non_null_text")]
    csv_content: Option<String>,
    #[serde(default, deserialize_with = "non_null_text")]
    csv_base64: Option<String>,
    account: Option<String>,
    profile: Option<String>,
}
struct OriginalImport {
    bytes: Vec<u8>,
    account: Option<String>,
    profile: Option<String>,
}
fn non_null_text<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    String::deserialize(d).map(Some)
}
fn original(input: Import) -> Result<OriginalImport, ()> {
    const MAX: usize = 25 * 1024 * 1024;
    let bytes = match (input.csv_content, input.csv_base64) {
        (Some(text), None) => text.into_bytes(),
        (None, Some(encoded)) if encoded.len() <= MAX.div_ceil(3) * 4 => {
            let bytes = STANDARD.decode(&encoded).map_err(|_| ())?;
            if STANDARD.encode(&bytes) != encoded {
                return Err(());
            }
            bytes
        }
        _ => return Err(()),
    };
    if bytes.is_empty() || bytes.len() > MAX {
        return Err(());
    }
    Ok(OriginalImport {
        bytes,
        account: input.account,
        profile: input.profile,
    })
}
enum Credential {
    Human(rentemester_runtime::auth::VerifiedCookie),
    Service(rentemester_runtime::service_key::ServiceKey),
}
enum Command {
    Accounts,
    Read(Option<String>),
    Create(AccountInput),
    Import(OriginalImport),
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
    let credential = if super::service_company::requested(&request) {
        match super::service_company::proof(&state, &request, mutation) {
            Ok(key) => Credential::Service(key),
            Err(r) => return *r,
        }
    } else {
        match proof(&state, &request, mutation) {
            Ok(cookie) => Credential::Human(cookie),
            Err(r) => return *r,
        }
    };
    if matches!(credential, Credential::Service(_))
        && !mutation
        && (request.headers().contains_key("content-encoding")
            || request.headers().contains_key("transfer-encoding")
            || request.headers().contains_key("idempotency-key")
            || one(request.headers(), "content-length").is_some_and(|v| v != "0"))
    {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    }
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
            let Ok(input) = original(input) else {
                return refused(StatusCode::BAD_REQUEST, "invalid_input");
            };
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
            Command::Import(input) => Action::ImportBytes {
                content: &input.bytes,
                account: input.account.as_deref(),
                profile: input.profile.as_deref(),
                now: &timestamp,
            },
        };
        let mut owner = state.native.lock_company(&company_slug).map_err(|_| ())?;
        match credential {
            Credential::Human(cookie) => {
                let attempt = owner.bank(&cookie, &SystemClock, action).map_err(|_| ())?;
                Ok::<_, ()>(super::documents::outcome(attempt).map(|value| (value, None)))
            }
            Credential::Service(key) => {
                let attempt = owner
                    .bank_service(&key, &SystemClock, action)
                    .map_err(|_| ())?;
                Ok::<_, ()>(match attempt.outcome {
                    Ok(value) => Ok((value.result, Some((value.usage, value.business)))),
                    Err(f) => Err(Box::new(super::service_company::failure(f))),
                })
            }
        }
    })
    .await;
    match result {
        Ok(Ok(Ok((value, stages)))) => {
            let mut body = json!({"ok":true,key:value});
            if let Some((usage, business)) = stages {
                body["usage"] = json!(usage);
                body["business"] = json!(business);
            }
            response(StatusCode::OK, body)
        }
        Ok(Ok(Err(response))) => *response,
        _ => refused(StatusCode::SERVICE_UNAVAILABLE, "outcome_unconfirmed"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn raw_envelope_preserves_bytes_and_refuses_conflicting_or_noncanonical_sources() {
        let bytes = b"Dato;Tekst;Bel\xf8b\r\n01.04.2026;\xe6\xf8\xe5\x80;-1,00\r\n";
        let input: Import = serde_json::from_value(
            json!({"confirm":true,"csvBase64":STANDARD.encode(bytes),"profile":"danske-bank"}),
        )
        .unwrap();
        assert_eq!(original(input).unwrap().bytes, bytes);
        for body in [
            json!({"confirm":true}),
            json!({"confirm":true,"csvContent":"csv","csvBase64":"Y3N2"}),
            json!({"confirm":true,"csvBase64":"YQ"}),
            json!({"confirm":true,"csvBase64":"YR=="}),
            json!({"confirm":true,"csvBase64":"YQ==\n"}),
            json!({"confirm":true,"csvBase64":""}),
            json!({"confirm":true,"csvContent":""}),
        ] {
            assert!(original(serde_json::from_value(body).unwrap()).is_err());
        }
        for body in [
            json!({"confirm":true,"csvContent":null,"csvBase64":"YQ=="}),
            json!({"confirm":true,"csvBase64":null,"csvContent":"source"}),
            json!({"confirm":true,"csvBase64":123}),
        ] {
            assert!(serde_json::from_value::<Import>(body).is_err());
        }
        let input: Import =
            serde_json::from_value(json!({"confirm":true,"csvContent":"x".repeat(25*1024*1024+1)}))
                .unwrap();
        assert!(original(input).is_err());
    }
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
