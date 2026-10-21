//! Original account commands and the existing browser PATCH share one updater.
use super::*;
use rentemester_application::bank::{AccountInput, AccountPatch, Action};
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Wire {
    confirm: bool,
    #[serde(default, deserialize_with = "text")]
    account: Option<Value>,
    #[serde(default, deserialize_with = "text")]
    slug: Option<Value>,
    #[serde(default, deserialize_with = "text")]
    name: Option<Value>,
    #[serde(default, deserialize_with = "text")]
    bank_name: Option<Value>,
    #[serde(default, deserialize_with = "text")]
    registration_no: Option<Value>,
    #[serde(default, deserialize_with = "text")]
    account_no: Option<Value>,
    #[serde(default, deserialize_with = "text")]
    iban: Option<Value>,
    #[serde(default, deserialize_with = "text")]
    bic: Option<Value>,
    #[serde(default, deserialize_with = "text")]
    account_owner: Option<Value>,
    #[serde(default, deserialize_with = "text")]
    customer_no: Option<Value>,
    #[serde(default, deserialize_with = "text")]
    currency: Option<Value>,
    #[serde(default, deserialize_with = "text")]
    ledger_account_no: Option<Value>,
    #[serde(default, deserialize_with = "boolean")]
    active: Option<bool>,
}
fn text<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<Value>, D::Error> {
    let value = Value::deserialize(d)?;
    if !value.is_string() && !value.is_null() {
        return Err(serde::de::Error::custom("expected string"));
    }
    Ok(Some(value))
}
fn boolean<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<bool>, D::Error> {
    bool::deserialize(d).map(Some)
}
enum Command {
    Create(AccountInput),
    Update {
        account: String,
        patch: AccountPatch,
        browser: bool,
    },
}
impl Command {
    fn action(&self) -> Action<'_> {
        match self {
            Self::Create(input) => Action::CreateOriginal(input),
            Self::Update {
                account,
                patch,
                browser: false,
            } => Action::UpdateOriginal { account, patch },
            Self::Update {
                account,
                patch,
                browser: true,
            } => Action::UpdateBrowser { account, patch },
        }
    }
}
fn selector(path: &str) -> Result<String, ()> {
    let value = path.rsplit('/').next().ok_or(())?;
    let bytes = value.as_bytes();
    for (i, b) in bytes.iter().enumerate() {
        if *b == b'%'
            && (i + 2 >= bytes.len()
                || !bytes[i + 1].is_ascii_hexdigit()
                || !bytes[i + 2].is_ascii_hexdigit())
        {
            return Err(());
        }
    }
    let value = percent_encoding::percent_decode_str(value)
        .decode_utf8()
        .map_err(|_| ())?;
    if value.is_empty()
        || value.len() > 128
        || value
            .chars()
            .any(|c| c.is_control() || matches!(c, '/' | '\\' | '\u{fffd}'))
    {
        return Err(());
    }
    Ok(value.into_owned())
}
fn parse(path: &str, bytes: &[u8], browser: bool) -> Result<Command, &'static str> {
    let mut wire: Wire = serde_json::from_slice(bytes).map_err(|_| "invalid_input")?;
    if !wire.confirm {
        return Err("confirmation_required");
    }
    // The pre-existing browser PATCH treats explicit null strings as clears;
    // original commands reject null. Derived Wire still rejects duplicate keys.
    for field in [
        &mut wire.account,
        &mut wire.slug,
        &mut wire.name,
        &mut wire.bank_name,
        &mut wire.registration_no,
        &mut wire.account_no,
        &mut wire.iban,
        &mut wire.bic,
        &mut wire.account_owner,
        &mut wire.customer_no,
        &mut wire.currency,
        &mut wire.ledger_account_no,
    ] {
        if let Some(value) = field
            && value.is_null()
        {
            if !browser {
                return Err("invalid_input");
            }
            *value = json!("");
        }
    }
    let text = |value: Option<Value>| {
        value.map(|v| v.as_str().expect("wire string normalized").to_owned())
    };
    if path.ends_with("/bank/accounts-original/create") && !browser {
        if wire.account.is_some() || wire.active.is_some() {
            return Err("invalid_input");
        }
        return Ok(Command::Create(AccountInput {
            name: text(wire.name).ok_or("invalid_input")?,
            slug: text(wire.slug),
            bank_name: text(wire.bank_name),
            registration_no: text(wire.registration_no),
            account_no: text(wire.account_no),
            iban: text(wire.iban),
            bic: text(wire.bic),
            account_owner: text(wire.account_owner),
            customer_no: text(wire.customer_no),
            currency: text(wire.currency),
            ledger_account_no: text(wire.ledger_account_no),
        }));
    }
    if wire.slug.is_some() || browser && wire.account.is_some() {
        return Err("invalid_input");
    }
    let account = if browser {
        selector(path).map_err(|_| "invalid_input")?
    } else {
        text(wire.account).ok_or("invalid_input")?
    };
    if account.is_empty() || account.len() > 128 || account.chars().any(char::is_control) {
        return Err("invalid_input");
    }
    Ok(Command::Update {
        account,
        browser,
        patch: AccountPatch {
            name: text(wire.name),
            bank_name: text(wire.bank_name),
            registration_no: text(wire.registration_no),
            account_no: text(wire.account_no),
            iban: text(wire.iban),
            bic: text(wire.bic),
            account_owner: text(wire.account_owner),
            customer_no: text(wire.customer_no),
            currency: text(wire.currency),
            ledger_account_no: text(wire.ledger_account_no),
            active: wire.active,
        },
    })
}
pub(super) async fn handle(State(state): State<Product>, request: Request) -> Response {
    let credential = match documents::credential(&state, &request, true) {
        Ok(value) => value,
        Err(r) => return *r,
    };
    if !selected(&state, &request) {
        return refused(StatusCode::NOT_FOUND, "company_not_found");
    }
    let browser = request.method() == axum::http::Method::PATCH;
    if !matches!(
        *request.method(),
        axum::http::Method::POST | axum::http::Method::PATCH
    ) || request.uri().query().is_some()
        || request.headers().contains_key("content-encoding")
        || !one(request.headers(), "content-type")
            .is_some_and(|v| v.eq_ignore_ascii_case("application/json"))
    {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    }
    let path = request.uri().path().to_owned();
    let slug = path.split('/').nth(3).unwrap_or("").to_owned();
    let bytes = match to_bytes(request.into_body(), 256 * 1024).await {
        Ok(bytes) => bytes,
        Err(_) => return refused(StatusCode::PAYLOAD_TOO_LARGE, "body_too_large"),
    };
    let command = match parse(&path, &bytes, browser) {
        Ok(command) => command,
        Err(code) => return refused(StatusCode::BAD_REQUEST, code),
    };
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let mut owner = state.native.lock_company(&slug).map_err(|_| ())?;
        Ok::<_, ()>(match credential {
            documents::Credential::Human(cookie) => documents::outcome(
                owner
                    .bank(&cookie, &SystemClock, command.action())
                    .map_err(|_| ())?,
            ),
            documents::Credential::Service(key) => documents::service_outcome(
                owner
                    .bank_service(&key, &SystemClock, command.action())
                    .map_err(|_| ())?,
            ),
        })
    })
    .await;
    match result {
        Ok(Ok(Ok(value))) => response(
            StatusCode::OK,
            if browser {
                json!({"ok":true,"bankAccount":value})
            } else {
                value
            },
        ),
        Ok(Ok(Err(r))) => *r,
        _ => refused(StatusCode::SERVICE_UNAVAILABLE, "outcome_unconfirmed"),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn account_command_wire_preserves_empty_omitted_and_browser_selector_boundaries() {
        let Command::Update {
            account,
            patch,
            browser,
        } = parse(
            "/api/companies/a/bank/accounts-original/update",
            br#"{"confirm":true,"account":" padded ","accountNo":"","active":false}"#,
            false,
        )
        .unwrap()
        else {
            panic!()
        };
        assert_eq!(account, " padded ");
        assert!(!browser);
        assert_eq!(patch.account_no.as_deref(), Some(""));
        assert!(patch.iban.is_none());
        assert_eq!(patch.active, Some(false));
        let Command::Update {
            account, browser, ..
        } = parse(
            "/api/companies/a/bank-accounts/%20padded%20",
            br#"{"confirm":true,"bic":"SYN"}"#,
            true,
        )
        .unwrap()
        else {
            panic!()
        };
        assert_eq!(account, " padded ");
        assert!(browser);
        let Command::Update { patch, .. } = parse(
            "/bank-accounts/a",
            br#"{"confirm":true,"accountNo":null,"iban":null}"#,
            true,
        )
        .unwrap() else {
            panic!()
        };
        assert_eq!(patch.account_no.as_deref(), Some(""));
        assert_eq!(patch.iban.as_deref(), Some(""));
        assert!(patch.bic.is_none());
        for body in [
            br#"{"confirm":true,"account":"a","accountNo":null}"#.as_slice(),
            br#"{"confirm":true,"account":"a","active":null}"#,
            br#"{"confirm":true,"account":"a","active":"false"}"#,
            br#"{"confirm":true,"account":"a","slug":"new"}"#,
            br#"{"confirm":true,"account":"a","actor":"forged"}"#,
            br#"{"confirm":true,"account":"a","name":"one","name":"two"}"#,
        ] {
            assert!(parse("/bank/accounts-original/update", body, false).is_err());
        }
        assert!(
            parse(
                "/bank-accounts/a",
                br#"{"confirm":true,"account":"other"}"#,
                true
            )
            .is_err()
        );
        assert!(
            parse(
                "/bank/accounts-original/create",
                br#"{"confirm":false,"name":"a"}"#,
                false
            )
            .is_err()
        );
        for path in [
            "/bank-accounts/%zz",
            "/bank-accounts/%00",
            "/bank-accounts/%FF",
            "/bank-accounts/%2F",
        ] {
            assert!(selector(path).is_err());
        }
    }
}
