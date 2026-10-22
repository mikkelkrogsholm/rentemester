//! Strict original mileage routes; no publication or retry.
use super::*;
use rentemester_application::{
    access::Permission,
    mileage_original::{Action, Log},
};
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
        "invalid_input" | "confirmation_required" => StatusCode::BAD_REQUEST,
        "mileage_refused"
        | "mileage_export_too_large"
        | "invalid_date"
        | "document_refused"
        | "account_role_refused"
        | "invoice_refused"
        | "sales_invoice_refused"
        | "journal_refused"
        | "document_evidence_unavailable"
        | "invalid_account"
        | "period_closed" => StatusCode::UNPROCESSABLE_ENTITY,
        _ => StatusCode::SERVICE_UNAVAILABLE,
    };
    let safe = match status {
        StatusCode::UNAUTHORIZED => "unauthorized",
        StatusCode::FORBIDDEN => "access_denied",
        StatusCode::BAD_REQUEST => "invalid_input",
        StatusCode::UNPROCESSABLE_ENTITY => "Mileage operation refused by current controls",
        _ => "outcome_unknown",
    };
    response(
        status,
        json!({"ok":false,"appliedRules":["DK-MILEAGE-LOG-001"],"errors":[safe]}),
    )
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Export {
    from: String,
    to: String,
    confirm: bool,
}
struct LogWire {
    input: Log,
    confirm: bool,
    expected: Option<String>,
}
impl<'de> Deserialize<'de> for LogWire {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = LogWire;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("strict mileage log")
            }
            fn visit_map<M: serde::de::MapAccess<'de>>(
                self,
                mut map: M,
            ) -> Result<LogWire, M::Error> {
                use serde::de::Error;
                let mut fields = serde_json::Map::new();
                while let Some((key, value)) = map.next_entry::<String, Value>()? {
                    if fields.insert(key, value).is_some() {
                        return Err(M::Error::custom("duplicate field"));
                    }
                }
                let confirm = fields
                    .remove("confirm")
                    .and_then(|v| v.as_bool())
                    .ok_or_else(|| M::Error::custom("confirmation required"))?;
                let expected = match fields.remove("expectedActor") {
                    None => None,
                    Some(Value::String(s))
                        if s.len() <= 160
                            && !s.is_empty()
                            && s.trim() == s
                            && !s.chars().any(char::is_control) =>
                    {
                        Some(s)
                    }
                    _ => return Err(M::Error::custom("invalid actor assertion")),
                };
                // Numbers may be missing, but explicit null is not a numeric JSON type.
                for key in ["kilometers", "ratePerKm"] {
                    if fields.get(key).is_some_and(|v| !v.is_number()) {
                        return Err(M::Error::custom("invalid numeric type"));
                    }
                }
                let input =
                    serde_json::from_value(Value::Object(fields)).map_err(M::Error::custom)?;
                Ok(LogWire {
                    input,
                    confirm,
                    expected,
                })
            }
        }
        d.deserialize_map(Visitor)
    }
}
enum Command {
    Log(Log),
    List,
    Report(String, String),
    Export(String, String),
    Invalid(Permission),
}
impl Command {
    fn action(&self) -> Action<'_> {
        match self {
            Self::Log(v) => Action::Log(v),
            Self::List => Action::List,
            Self::Report(f, t) => Action::Report(f, t),
            Self::Export(f, t) => Action::Export(f, t),
            Self::Invalid(p) => Action::Invalid(*p),
        }
    }
}
fn range(query: &str) -> Option<(String, String)> {
    if query.len() > 4096 {
        return None;
    }
    let bytes = query.as_bytes();
    for (i, byte) in bytes.iter().enumerate() {
        if *byte == b'%'
            && (i + 2 >= bytes.len()
                || !bytes[i + 1].is_ascii_hexdigit()
                || !bytes[i + 2].is_ascii_hexdigit())
        {
            return None;
        }
    }
    let mut from = None;
    let mut to = None;
    for (key, value) in url::form_urlencoded::parse(query.as_bytes()) {
        let slot = match key.as_ref() {
            "from" => &mut from,
            "to" => &mut to,
            _ => return None,
        };
        if slot.replace(value.into_owned()).is_some() {
            return None;
        }
    }
    Some((from.unwrap_or_default(), to.unwrap_or_default()))
}
fn parse(
    bytes: &[u8],
    operation: &str,
    query: Option<&str>,
    headers_valid: bool,
) -> (Command, Option<String>) {
    let permission = match operation {
        "log" => Permission::CompanyDraftWrite,
        "export" => Permission::CompanyExport,
        _ => Permission::CompanyRead,
    };
    let invalid = || (Command::Invalid(permission), None);
    if !headers_valid {
        return invalid();
    }
    match operation {
        "log" if query.is_none() => match serde_json::from_slice::<LogWire>(bytes) {
            Ok(v) if v.confirm => (Command::Log(v.input), v.expected),
            _ => invalid(),
        },
        "export" if query.is_none() => match serde_json::from_slice::<Export>(bytes) {
            Ok(v) if v.confirm => (Command::Export(v.from, v.to), None),
            _ => invalid(),
        },
        "list" if query.is_none() && bytes.is_empty() => (Command::List, None),
        "report" if bytes.is_empty() => match range(query.unwrap_or_default()) {
            Some((f, t)) => (Command::Report(f, t), None),
            None => invalid(),
        },
        _ => invalid(),
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
    let operation = path.rsplit('/').next().unwrap_or("");
    let query = request.uri().query().map(str::to_owned);
    let headers_valid = !request.headers().contains_key("content-encoding")
        && !request.headers().contains_key("idempotency-key")
        && (!write
            || one(request.headers(), "content-type")
                .is_some_and(|v| v.eq_ignore_ascii_case("application/json")));
    let bytes = to_bytes(request.into_body(), if write { 16_384 } else { 1 }).await;
    let (command, expected) = match bytes {
        Ok(b) => parse(&b, operation, query.as_deref(), headers_valid),
        Err(_) => parse(&[], operation, query.as_deref(), false),
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
                    .original_mileage(&cookie, &SystemClock, command.action(), expected.as_deref())
                    .map_err(|_| Box::new(domain_error("", true)))?,
            ),
            documents::Credential::Service(key) => service_checked(
                owner
                    .original_mileage_service(
                        &key,
                        &SystemClock,
                        command.action(),
                        expected.as_deref(),
                    )
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
    fn strict_wire_preserves_semantic_source_validation() {
        let log = json!({"confirm":true,"tripDate":"bad","purpose":"","fromLocation":"","toLocation":"","vehicle":"","driver":"","rateBasis":"","kilometers":-1,"ratePerKm":0});
        assert!(matches!(
            parse(&serde_json::to_vec(&log).unwrap(), "log", None, true).0,
            Command::Log(_)
        ));
        for key in ["actor", "outputDir", "idempotencyKey"] {
            let mut v = log.clone();
            v[key] = json!("forbidden");
            assert!(matches!(
                parse(&serde_json::to_vec(&v).unwrap(), "log", None, true).0,
                Command::Invalid(_)
            ));
        }
        for bytes in [
            br#"{"from":"","to":"","confirm":true,"outputDir":"/tmp"}"#.as_slice(),
            br#"{"from":"","to":"","confirm":true,"confirm":true}"#,
            br#"{"from":"","to":"","confirm":false}"#,
        ] {
            assert!(matches!(
                parse(bytes, "export", None, true).0,
                Command::Invalid(Permission::CompanyExport)
            ));
        }
        assert!(range("from=a&from=b").is_none());
        assert!(range("from=a&to=b&extra=c").is_none());
        assert_eq!(range(""), Some((String::new(), String::new())));
        assert!(matches!(
            parse(&[], "report", Some("from=bad"), true).0,
            Command::Report(..)
        ));
        assert_eq!(
            domain_error("access_denied", true).status(),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            domain_error("invalid_credentials", true).status(),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            domain_error("mileage_export_too_large", false).status(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }
}
