//! Fixed original budget routes; malformed input is evaluated after current authority.
use super::*;
use rentemester_application::budget_original::{Action, Monthly, Set, Weekly};
fn checked<T>(
    attempt: rentemester_host::Attempt<
        Result<
            rentemester_application::control_fence::Committed<T>,
            rentemester_application::control_fence::Failure,
        >,
    >,
) -> Result<T, Box<Response>> {
    if let Err(f) = &attempt.outcome
        && matches!(f.error.code, "invalid_credentials" | "access_denied")
    {
        return Err(Box::new(domain_error(f.error.code, false)));
    }
    if attempt.retired {
        return Err(Box::new(domain_error("", true)));
    }
    match attempt.outcome {
        Ok(value) if value.control_reusable => Ok(value.value),
        Ok(_) => Err(Box::new(domain_error("", true))),
        Err(f) => Err(Box::new(domain_error(
            f.error.code,
            f.stage == FailureStage::OutcomeUnknown || !f.control_reusable || !f.company_reusable,
        ))),
    }
}
fn service_checked<T>(
    attempt: rentemester_host::Attempt<
        Result<
            rentemester_application::service_company::ServiceResult<T>,
            rentemester_application::service_company::ServiceFailure,
        >,
    >,
) -> Result<T, Box<Response>> {
    use rentemester_application::service_company::{BusinessStage, UsageStage};
    if let Err(f) = &attempt.outcome
        && matches!(f.error.code, "invalid_credentials" | "access_denied")
    {
        return Err(Box::new(domain_error(f.error.code, false)));
    }
    if attempt.retired {
        return Err(Box::new(domain_error("", true)));
    }
    match attempt.outcome {
        Ok(value) if value.control_reusable => Ok(value.result),
        Ok(_) => Err(Box::new(domain_error("", true))),
        Err(f) => Err(Box::new(domain_error(
            f.error.code,
            f.usage == UsageStage::OutcomeUnknown
                || f.business == BusinessStage::OutcomeUnknown
                || !f.control_reusable
                || !f.company_reusable,
        ))),
    }
}
fn domain_error(code: &str, unknown: bool) -> Response {
    let status = match code {
        "invalid_credentials" => StatusCode::UNAUTHORIZED,
        "access_denied" => StatusCode::FORBIDDEN,
        _ if unknown => StatusCode::SERVICE_UNAVAILABLE,
        "invalid_input" => StatusCode::BAD_REQUEST,
        _ => StatusCode::SERVICE_UNAVAILABLE,
    };
    let safe = match status {
        StatusCode::UNAUTHORIZED => "unauthorized",
        StatusCode::FORBIDDEN => "access_denied",
        StatusCode::BAD_REQUEST => "invalid_input",
        _ => "outcome_unknown",
    };
    response(status, json!({"ok":false,"errors":[safe]}))
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Write {
    confirm: bool,
    #[serde(default)]
    account_no: String,
    #[serde(default)]
    period: String,
    #[serde(default, deserialize_with = "number_present")]
    amount: Option<serde_json::Number>,
    notes: Option<String>,
}
fn number_present<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Option<serde_json::Number>, D::Error> {
    serde_json::Number::deserialize(d).map(Some)
}
enum Command {
    Set(Set),
    List(Option<String>, Option<String>),
    Compare(String, String),
    Monthly(Monthly),
    Weekly(Weekly),
    Invalid(bool),
}
impl Command {
    fn action(&self) -> Action<'_> {
        match self {
            Self::Set(v) => Action::Set(v),
            Self::List(p, a) => Action::List {
                period: p.as_deref(),
                account_no: a.as_deref(),
            },
            Self::Compare(f, t) => Action::Compare { from: f, to: t },
            Self::Monthly(v) => Action::Monthly(v),
            Self::Weekly(v) => Action::Weekly(v),
            Self::Invalid(write) => Action::Invalid { write: *write },
        }
    }
}
fn parse_write(bytes: &[u8]) -> Command {
    match serde_json::from_slice::<Write>(bytes) {
        Ok(v)
            if v.confirm
                && v.account_no.len() <= 128
                && v.period.len() <= 128
                && v.notes.as_ref().is_none_or(|n| n.len() <= 12000) =>
        {
            Command::Set(Set {
                account_no: v.account_no,
                period: v.period,
                amount: v.amount,
                notes: v.notes,
            })
        }
        _ => Command::Invalid(true),
    }
}
fn parse_read(route: &str, query: &str) -> Command {
    if query.len() > 4096 {
        return Command::Invalid(false);
    }
    let allowed: &[&str] = match route {
        "list" => &["period", "accountNo"],
        "vs-actual" => &["from", "to"],
        "forecast" => &["startDate", "months"],
        "forecast-13-week" => &["startDate", "weeks"],
        _ => return Command::Invalid(false),
    };
    let mut fields = std::collections::BTreeMap::new();
    for (k, v) in url::form_urlencoded::parse(query.as_bytes()) {
        if !allowed.contains(&k.as_ref())
            || v.len() > 128
            || v.chars().any(char::is_control)
            || fields.insert(k.into_owned(), v.into_owned()).is_some()
        {
            return Command::Invalid(false);
        }
    }
    match route {
        "list" => Command::List(fields.remove("period"), fields.remove("accountNo")),
        "vs-actual" => Command::Compare(
            fields.remove("from").unwrap_or_default(),
            fields.remove("to").unwrap_or_default(),
        ),
        "forecast" => Command::Monthly(Monthly {
            start_date: fields.remove("startDate").unwrap_or_default(),
            months: fields.remove("months").unwrap_or_default(),
        }),
        _ => Command::Weekly(Weekly {
            start_date: fields.remove("startDate").unwrap_or_default(),
            weeks: fields.remove("weeks"),
        }),
    }
}
pub(super) async fn handle(State(state): State<Product>, request: Request) -> Response {
    let write = request.method() == axum::http::Method::POST;
    let credential = match documents::credential(&state, &request, write) {
        Ok(v) => v,
        Err(r) => return *r,
    };
    if !selected(&state, &request) {
        return refused(StatusCode::NOT_FOUND, "company_not_found");
    }
    let path = request.uri().path().to_owned();
    let slug = path.split('/').nth(3).unwrap_or("").to_owned();
    let route = path.rsplit('/').next().unwrap_or("");
    let query = request.uri().query().unwrap_or("").to_owned();
    let valid = !request.headers().contains_key("content-encoding")
        && !request.headers().contains_key("idempotency-key")
        && (!write
            || (request.uri().query().is_none()
                && one(request.headers(), "content-type")
                    .is_some_and(|v| v.eq_ignore_ascii_case("application/json"))));
    let command = match to_bytes(request.into_body(), if write { 16384 } else { 1 }).await {
        Ok(bytes) if valid && write => parse_write(&bytes),
        Ok(bytes) if valid && bytes.is_empty() => parse_read(route, &query),
        _ => Command::Invalid(write),
    };
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let mut owner = state
            .native
            .lock_company(&slug)
            .map_err(|_| Box::new(domain_error("", true)))?;
        match credential {
            documents::Credential::Human(cookie) => checked(
                owner
                    .original_budget(&cookie, &SystemClock, command.action())
                    .map_err(|_| Box::new(domain_error("", true)))?,
            ),
            documents::Credential::Service(key) => service_checked(
                owner
                    .original_budget_service(&key, &SystemClock, command.action())
                    .map_err(|_| Box::new(domain_error("", true)))?,
            ),
        }
    })
    .await;
    match result {
        Ok(Ok(value)) => response(
            if value["ok"] == false {
                StatusCode::UNPROCESSABLE_ENTITY
            } else {
                StatusCode::OK
            },
            value,
        ),
        Ok(Err(r)) => *r,
        _ => domain_error("", true),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_selectors_preserve_semantic_invalids() {
        assert!(matches!(
            parse_read("forecast", "startDate=2026-01-15&months=1e1"),
            Command::Monthly(_)
        ));
        assert!(matches!(parse_read("forecast", ""), Command::Monthly(_)));
        assert!(matches!(
            parse_read("vs-actual", "from=bad"),
            Command::Compare(_, _)
        ));
        assert!(matches!(
            parse_read("forecast-13-week", "weeks="),
            Command::Weekly(Weekly { weeks: Some(_), .. })
        ));
        for q in [
            "months=1&months=2",
            "year=2026",
            "companySlug=other",
            "supplements=x",
        ] {
            assert!(matches!(parse_read("forecast", q), Command::Invalid(false)));
        }
    }
    #[test]
    fn strict_set_source_wire() {
        assert!(matches!(
            parse_write(br#"{"confirm":true,"accountNo":"2000","period":"2026-01","amount":0}"#),
            Command::Set(_)
        ));
        assert!(matches!(
            parse_write(br#"{"confirm":true}"#),
            Command::Set(_)
        ));
        for bytes in [
            br#"{"confirm":true,"confirm":true}"#.as_slice(),
            br#"{"confirm":true,"amount":"1"}"#,
            br#"{"confirm":false}"#,
            br#"{"confirm":true,"amount":null}"#,
            br#"{"confirm":true,"actor":"user:forged"}"#,
        ] {
            assert!(matches!(parse_write(bytes), Command::Invalid(true)));
        }
        assert_eq!(
            domain_error("access_denied", true).status(),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            domain_error("invalid_credentials", true).status(),
            StatusCode::UNAUTHORIZED
        );
    }
}
