//! Fixed source VAT GET routes; strict parsing runs only after current authority.
use super::*;
use rentemester_application::vat_original::Operation;
use rentemester_engine::Error;

fn invalid() -> Error {
    Error::new("invalid_input", "Invalid original VAT request")
}
fn dates(query: &str) -> Result<(String, String), Error> {
    if query.len() > 1024
        || query.bytes().enumerate().any(|(i, b)| {
            b == b'%'
                && (i + 2 >= query.len()
                    || !query.as_bytes()[i + 1].is_ascii_hexdigit()
                    || !query.as_bytes()[i + 2].is_ascii_hexdigit())
        })
    {
        return Err(invalid());
    }
    let mut from = None;
    let mut to = None;
    for (key, value) in url::form_urlencoded::parse(query.as_bytes()) {
        let slot = match key.as_ref() {
            "from" => &mut from,
            "to" => &mut to,
            _ => return Err(invalid()),
        };
        if slot.is_some()
            || value.len() > 128
            || value.chars().any(|c| c.is_control() || c == '\u{fffd}')
        {
            return Err(invalid());
        }
        *slot = Some(value.into_owned());
    }
    // Missing/empty/malformed dates are admitted domain inputs, not date parsing here.
    Ok((from.unwrap_or_default(), to.unwrap_or_default()))
}
fn source_response(value: Value) -> Response {
    response(
        if value["ok"] == false {
            StatusCode::UNPROCESSABLE_ENTITY
        } else {
            StatusCode::OK
        },
        value,
    )
}
pub(super) async fn handle(State(state): State<Product>, request: Request) -> Response {
    // Do not use documents::credential: its service structural preflight precedes authority.
    let key = if service_company::requested(&request) {
        match service_company::proof(&state, &request, false) {
            Ok(key) => Some(key),
            Err(r) => return *r,
        }
    } else {
        None
    };
    let cookie = if key.is_none() {
        match proof(&state, &request, false) {
            Ok(cookie) => Some(cookie),
            Err(r) => return *r,
        }
    } else {
        None
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
    let operation = match request.uri().path().rsplit('/').next().unwrap_or("") {
        "report" => Operation::Report,
        "eu-sales-list" => Operation::EuSalesList,
        "oss-report" => Operation::OssReport,
        "filing" => Operation::Filing,
        _ => return refused(StatusCode::NOT_FOUND, "not_found"),
    };
    let query = request.uri().query().unwrap_or("").to_owned();
    let headers_valid = !["content-encoding", "transfer-encoding", "idempotency-key"]
        .iter()
        .any(|key| request.headers().contains_key(*key))
        && (!request.headers().contains_key("content-length")
            || one(request.headers(), "content-length") == Some("0"));
    let body_valid =
        matches!(to_bytes(request.into_body(), 1).await, Ok(bytes) if bytes.is_empty());
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let Ok(mut owner) = state.native.lock_company(&slug) else {
            return refused(StatusCode::SERVICE_UNAVAILABLE, "owner_unavailable");
        };
        let parse = || {
            if !headers_valid || !body_valid {
                return Err(invalid());
            }
            dates(&query)
        };
        if let Some(key) = key {
            let Ok(attempt) = owner.original_vat_service(&key, &SystemClock, operation, parse)
            else {
                return refused(StatusCode::SERVICE_UNAVAILABLE, "owner_unavailable");
            };
            if attempt.retired {
                return refused(StatusCode::SERVICE_UNAVAILABLE, "owner_unavailable");
            }
            match attempt.outcome {
                Ok(value) => source_response(value.result),
                Err(f)
                    if f.control_reusable
                        && f.company_reusable
                        && f.error.code == "invalid_input" =>
                {
                    refused(StatusCode::BAD_REQUEST, "invalid_input")
                }
                Err(f) if f.error.code.contains("unavailable") => {
                    refused(StatusCode::SERVICE_UNAVAILABLE, f.error.code)
                }
                Err(f) => service_company::failure(f),
            }
        } else {
            let Ok(attempt) = owner.original_vat(
                cookie.as_ref().expect("human credential"),
                &SystemClock,
                operation,
                parse,
            ) else {
                return refused(StatusCode::SERVICE_UNAVAILABLE, "owner_unavailable");
            };
            if attempt.retired {
                return refused(StatusCode::SERVICE_UNAVAILABLE, "owner_unavailable");
            }
            match attempt.outcome {
                Ok(value) => source_response(value.value),
                Err(f)
                    if !f.control_reusable
                        || !f.company_reusable
                        || f.stage != FailureStage::NotCommitted =>
                {
                    refused(StatusCode::SERVICE_UNAVAILABLE, "read_unconfirmed")
                }
                Err(f) if f.error.code == "invalid_credentials" => {
                    refused(StatusCode::UNAUTHORIZED, "unauthorized")
                }
                Err(f) if f.error.code == "access_denied" => {
                    refused(StatusCode::FORBIDDEN, "access_denied")
                }
                Err(f) if f.error.code == "invalid_input" => {
                    refused(StatusCode::BAD_REQUEST, "invalid_input")
                }
                Err(f) => refused(StatusCode::SERVICE_UNAVAILABLE, f.error.code),
            }
        }
    })
    .await
    .unwrap_or_else(|_| refused(StatusCode::SERVICE_UNAVAILABLE, "read_unconfirmed"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn strict_keys_but_domain_dates() {
        for query in [
            "from=x&from=y",
            "to=x&unknown=y",
            "actor=x",
            "from=%",
            "to=%GG",
            "from=%00",
            "from=%FF",
        ] {
            assert!(dates(query).is_err(), "{query}");
        }
        assert_eq!(dates("").unwrap(), (String::new(), String::new()));
        assert_eq!(
            dates("from=bad&to=earlier").unwrap(),
            ("bad".into(), "earlier".into())
        );
        assert_eq!(dates("from=&to=").unwrap(), (String::new(), String::new()));
        assert!(dates(&format!("from={}", "a".repeat(129))).is_err());
    }
    #[test]
    fn source_false_is_422_without_rewriting_fields() {
        let response =
            source_response(json!({"ok":false,"errors":["selskabet er ikke momsregistreret"]}));
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }
}
