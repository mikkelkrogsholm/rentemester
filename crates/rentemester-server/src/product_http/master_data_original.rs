//! Frozen flat source HTTP projections; provider calls release the admitted Owner.
use super::documents::{Credential, credential};
use super::*;
use rentemester_application::master_data_original::{Output, Request as Action};
use rentemester_storage::master_data_original::{Create, Kind};
enum Command {
    List(Kind, bool),
    Create(Kind, Create),
    Cvr(String),
    Vat(String),
}
fn invoke(
    state: &Product,
    slug: &str,
    credential: &Credential,
    action: Action<'_>,
) -> Result<Output, Box<Response>> {
    let mut owner = state.native.lock_company(slug).map_err(|_| {
        Box::new(refused(
            StatusCode::SERVICE_UNAVAILABLE,
            "owner_unavailable",
        ))
    })?;
    match credential {
        Credential::Human(cookie) => {
            let attempt = owner
                .master_data_original(cookie, &SystemClock, action)
                .map_err(|_| {
                    Box::new(refused(
                        StatusCode::SERVICE_UNAVAILABLE,
                        "owner_unavailable",
                    ))
                })?;
            let retired = attempt.retired;
            match attempt.outcome {
                Ok(value) => admit_output(value.value, retired, value.control_reusable),
                Err(f) => Err(Box::new(original_failure(
                    &f.error,
                    f.stage == FailureStage::OutcomeUnknown,
                    f.control_reusable && f.company_reusable,
                ))),
            }
        }
        Credential::Service(key) => {
            let attempt = owner
                .master_data_original_service(key, &SystemClock, action)
                .map_err(|_| {
                    Box::new(refused(
                        StatusCode::SERVICE_UNAVAILABLE,
                        "owner_unavailable",
                    ))
                })?;
            let retired = attempt.retired;
            match attempt.outcome {
                Ok(value) => admit_output(value.result, retired, value.control_reusable),
                Err(f) => Err(Box::new(original_failure(&f.error, f.usage == rentemester_application::service_company::UsageStage::OutcomeUnknown || f.business == rentemester_application::service_company::BusinessStage::OutcomeUnknown, f.control_reusable && f.company_reusable))),
            }
        }
    }
}
// Retirement after a known final commit does not erase that success. A
// preparatory admission, however, must never authorize new provider work.
fn admit_output(value: Output, retired: bool, reusable: bool) -> Result<Output, Box<Response>> {
    if matches!(&value, Output::Admission(_)) && (retired || !reusable) {
        Err(Box::new(refused(
            StatusCode::SERVICE_UNAVAILABLE,
            "owner_unavailable",
        )))
    } else {
        Ok(value)
    }
}
fn original_failure(error: &rentemester_engine::Error, unknown: bool, reusable: bool) -> Response {
    if unknown || !reusable {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "outcome_unconfirmed");
    }
    match error.code {
        "invalid_credentials" => refused(StatusCode::UNAUTHORIZED, "unauthorized"),
        "access_denied" => refused(StatusCode::FORBIDDEN, "access_denied"),
        _ => response(
            StatusCode::UNPROCESSABLE_ENTITY,
            json!({"ok":false,"code":error.code,"errors":[error.message]}),
        ),
    }
}
fn execute(
    state: &Product,
    slug: &str,
    credential: &Credential,
    command: &Command,
) -> Result<Value, Box<Response>> {
    let action = match command {
        Command::List(kind, all) => Action::List(*kind, *all),
        Command::Create(kind, input) => {
            match input.from_cvr.as_deref().filter(|s| !s.trim().is_empty()) {
                Some(cvr) => Action::PrepareCvr(cvr, Some((*kind, input))),
                None => Action::Create(*kind, input),
            }
        }
        Command::Cvr(cvr) => Action::PrepareCvr(cvr, None),
        Command::Vat(vat) => Action::PrepareVat(vat),
    };
    let admission = match invoke(state, slug, credential, action)? {
        Output::Value(v) => return Ok(v),
        Output::Admission(a) => a,
    };
    let observation = if let Some(identity) = admission.provider_identity() {
        if admission.is_vat() {
            state.native.vies.validate(identity).map_err(|e|Box::new(response(StatusCode::UNPROCESSABLE_ENTITY,json!({"ok":false,"code":e.code(),"errors":[e.code()],"appliedRules":["DK-VAT-REVERSE-CHARGE-001"]}))))?
        } else {
            use rentemester_runtime::contact_import::Lookup;
            match rentemester_runtime::contact_import::OperatorLookup::default().lookup(identity) {
                Ok(v) if serde_json::to_vec(&v).is_ok_and(|b| b.len() <= 4 * 1024 * 1024) => v,
                Ok(_) => {
                    json!({"providerError":"CVR returnerede et ugyldigt eller for stort svar"})
                }
                Err(e) => json!({"providerError":e.message()}),
            }
        }
    } else {
        Value::Null
    };
    match invoke(
        state,
        slug,
        credential,
        Action::Finish(admission, &observation),
    )? {
        Output::Value(v) => Ok(v),
        Output::Admission(_) => Err(Box::new(refused(
            StatusCode::SERVICE_UNAVAILABLE,
            "invalid_admission",
        ))),
    }
}
pub(super) async fn handle(State(state): State<Product>, request: Request) -> Response {
    // GET-backed HEAD must refuse before credentials, body parsing or mutation dispatch.
    if request.method() == axum::http::Method::HEAD {
        let mut response = refused(StatusCode::METHOD_NOT_ALLOWED, "method_not_allowed");
        response.headers_mut().insert("allow", axum::http::HeaderValue::from_static("GET"));
        return response;
    }
    let mutation = request.method() != axum::http::Method::GET;
    let credential = match credential(&state, &request, mutation) {
        Ok(c) => c,
        Err(r) => return *r,
    };
    if !selected(&state, &request) {
        return refused(StatusCode::NOT_FOUND, "company_not_found");
    }
    let slug = request
        .uri()
        .path()
        .split('/')
        .nth(3)
        .unwrap_or("")
        .to_owned();
    let path = request.uri().path().to_owned();
    if request.headers().contains_key("content-encoding")
        || request.headers().contains_key("idempotency-key")
    {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    }
    let kind = if path.ends_with("/customers") {
        Kind::Customer
    } else {
        Kind::Vendor
    };
    let command = if mutation {
        if request.uri().query().is_some()
            || !one(request.headers(), "content-type").is_some_and(|s| {
                s.eq_ignore_ascii_case("application/json")
                    || s.eq_ignore_ascii_case("application/json; charset=utf-8")
            })
        {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        }
        let Ok(bytes) = to_bytes(request.into_body(), 256 * 1024).await else {
            return refused(StatusCode::PAYLOAD_TOO_LARGE, "body_too_large");
        };
        if path.ends_with("/customer-validate-vat") {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Vat {
                cvr: String,
                confirm: bool,
            }
            match serde_json::from_slice::<Vat>(&bytes) {
                Ok(v) if v.confirm => Command::Vat(v.cvr),
                _ => return refused(StatusCode::BAD_REQUEST, "invalid_input"),
            }
        } else {
            match serde_json::from_slice::<Create>(&bytes) {
                Ok(v) if v.validate(kind).is_ok() => Command::Create(kind, v),
                _ => return refused(StatusCode::BAD_REQUEST, "invalid_input"),
            }
        }
    } else {
        let command = if path.ends_with("/cvr-lookup") {
            let mut pairs =
                url::form_urlencoded::parse(request.uri().query().unwrap_or("").as_bytes());
            match (pairs.next(), pairs.next()) {
                (Some((k, v)), None) if k == "cvr" && !v.trim().is_empty() => {
                    Command::Cvr(v.into_owned())
                }
                _ => return refused(StatusCode::BAD_REQUEST, "invalid_query"),
            }
        } else {
            match request.uri().query() {
                None | Some("archived=false") => Command::List(kind, false),
                Some("archived=true") => Command::List(kind, true),
                _ => return refused(StatusCode::BAD_REQUEST, "invalid_query"),
            }
        };
        if to_bytes(request.into_body(), 0).await.is_err() {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        }
        command
    };
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    match tokio::task::spawn_blocking(move || {
        let _permit = permit;
        execute(&state, &slug, &credential, &command)
    })
    .await
    {
        Ok(Ok(value)) => response(StatusCode::OK, value),
        Ok(Err(r)) => *r,
        _ => refused(StatusCode::SERVICE_UNAVAILABLE, "outcome_unconfirmed"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn master_data_original_flat_business_errors_and_known_final_success() {
        let e = rentemester_engine::Error::new("contact_refused", "actual source business refusal");
        let r = original_failure(&e, false, true);
        assert_eq!(r.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body: Value =
            serde_json::from_slice(&to_bytes(r.into_body(), 4096).await.unwrap()).unwrap();
        assert_eq!(
            body,
            json!({"ok":false,"code":"contact_refused","errors":["actual source business refusal"]})
        );
        for (unknown, reusable) in [(true, true), (false, false)] {
            assert_eq!(
                original_failure(&e, unknown, reusable).status(),
                StatusCode::SERVICE_UNAVAILABLE
            );
        }
        assert!(
            admit_output(
                Output::Value(json!({"ok":true,"customerId":1})),
                true,
                false
            )
            .is_ok()
        );
    }
}
