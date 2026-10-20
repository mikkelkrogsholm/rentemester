use super::*;
use rentemester_application::sales::{Action, Output};
use rentemester_engine::sales_invoice::Input;
use rentemester_storage::sales::{Customer, Post};
enum Credential {
    Human(rentemester_runtime::auth::VerifiedCookie),
    Service(rentemester_runtime::service_key::ServiceKey),
}
enum Command {
    Contacts,
    DeleteCustomer(i64, bool),
    DeleteVendor(i64, bool),
    Customer(Customer),
    Vendor(rentemester_storage::vendors::Vendor),
    UpdateVendor(i64, Value),
    UpdateCustomer(i64, Value),
    List,
    Issue(Input),
    Preview(Input),
    Post(Post),
    Settle(rentemester_storage::invoice_settlement::Input),
    Credit(rentemester_storage::credit_note::Input),
    Refund(rentemester_storage::invoice_settlement::RefundInput),
    Pdf(i64),
}
pub(super) async fn handle(State(state): State<Product>, request: Request) -> Response {
    let company_slug = request
        .uri()
        .path()
        .split('/')
        .nth(3)
        .unwrap_or("")
        .to_owned();
    let method = request.method().clone();
    let mutation = method != axum::http::Method::GET;
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
    let query = match query(&request) {
        Ok(q) => q,
        Err(r) => return *r,
    };
    if query.account.is_some()
        || query.format.is_some()
        || (query.year.is_some() && !path.ends_with("/invoices"))
    {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    }
    let command = if mutation {
        if request.headers().contains_key("content-encoding")
            || !one(request.headers(), "content-type")
                .is_some_and(|s| s.eq_ignore_ascii_case("application/json"))
        {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        }
        let Ok(bytes) = to_bytes(request.into_body(), 256 * 1024).await else {
            return refused(StatusCode::PAYLOAD_TOO_LARGE, "invalid_input");
        };
        if method == axum::http::Method::PATCH || method == axum::http::Method::DELETE {
            let Some(id) = path
                .rsplit('/')
                .next()
                .and_then(|s| s.parse::<i64>().ok())
                .filter(|n| *n > 0)
            else {
                return refused(StatusCode::BAD_REQUEST, "invalid_input");
            };
            if method == axum::http::Method::DELETE {
                #[derive(serde::Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Confirmation {
                    confirm: bool,
                }
                let input: Confirmation = match serde_json::from_slice(&bytes) {
                    Ok(input) => input,
                    Err(_) => return refused(StatusCode::BAD_REQUEST, "invalid_input"),
                };
                if path.contains("/vendors/") {
                    Command::DeleteVendor(id, input.confirm)
                } else {
                    Command::DeleteCustomer(id, input.confirm)
                }
            } else {
                match serde_json::from_slice::<Value>(&bytes) {
                    Ok(input) if input.is_object() => {
                        if path.contains("/vendors/") {
                            Command::UpdateVendor(id, input)
                        } else {
                            Command::UpdateCustomer(id, input)
                        }
                    }
                    _ => return refused(StatusCode::BAD_REQUEST, "invalid_input"),
                }
            }
        } else if path.ends_with("/vendors") {
            match serde_json::from_slice(&bytes) {
                Ok(input) => Command::Vendor(input),
                Err(_) => return refused(StatusCode::BAD_REQUEST, "invalid_input"),
            }
        } else if path.ends_with("/customers") {
            match serde_json::from_slice(&bytes) {
                Ok(input) => Command::Customer(input),
                Err(_) => return refused(StatusCode::BAD_REQUEST, "invalid_input"),
            }
        } else if path.ends_with("/refund") {
            match serde_json::from_slice(&bytes) {
                Ok(input) => Command::Refund(input),
                Err(_) => return refused(StatusCode::BAD_REQUEST, "invalid_input"),
            }
        } else if path.ends_with("/credit-note") {
            match serde_json::from_slice(&bytes) {
                Ok(input) => Command::Credit(input),
                Err(_) => return refused(StatusCode::BAD_REQUEST, "invalid_input"),
            }
        } else if path.ends_with("/settle") {
            match serde_json::from_slice(&bytes) {
                Ok(input) => Command::Settle(input),
                Err(_) => return refused(StatusCode::BAD_REQUEST, "invalid_input"),
            }
        } else if path.ends_with("/post") {
            match serde_json::from_slice(&bytes) {
                Ok(input) => Command::Post(input),
                Err(_) => return refused(StatusCode::BAD_REQUEST, "invalid_input"),
            }
        } else {
            let input = match serde_json::from_slice(&bytes) {
                Ok(input) => input,
                Err(_) => return refused(StatusCode::BAD_REQUEST, "invalid_input"),
            };
            if path.ends_with("/preview") {
                Command::Preview(input)
            } else {
                Command::Issue(input)
            }
        }
    } else if path.ends_with("/contacts") {
        Command::Contacts
    } else if path.ends_with("/pdf") {
        let id = path
            .rsplit('/')
            .nth(1)
            .and_then(|s| s.parse::<i64>().ok())
            .filter(|n| *n > 0);
        let Some(id) = id else {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        };
        Command::Pdf(id)
    } else {
        Command::List
    };
    if matches!(credential, Credential::Service(_))
        && !matches!(
            command,
            Command::Contacts
                | Command::Customer(_)
                | Command::List
                | Command::Preview(_)
                | Command::Issue(_)
                | Command::Post(_)
                | Command::Settle(_)
                | Command::Pdf(_)
        )
    {
        return refused(StatusCode::FORBIDDEN, "service_operation_unavailable");
    }
    let key = match command {
        Command::Contacts => "contacts",
        Command::Vendor(_) | Command::UpdateVendor(_, _) | Command::DeleteVendor(_, _) => "vendor",
        Command::Customer(_) | Command::UpdateCustomer(_, _) | Command::DeleteCustomer(_, _) => {
            "customer"
        }
        Command::List => "invoices",
        Command::Post(_) => "posting",
        Command::Settle(_) => "settlement",
        Command::Credit(_) => "creditNote",
        Command::Refund(_) => "refund",
        _ => "invoice",
    };
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let mut owner = state.native.lock_company(&company_slug).map_err(|_| ())?;
        let action = match &command {
            Command::Contacts => Action::Contacts,
            Command::DeleteCustomer(id, confirm) => Action::DeleteCustomer(*id, *confirm),
            Command::DeleteVendor(id, confirm) => Action::DeleteVendor(*id, *confirm),
            Command::Vendor(input) => Action::Vendor(input),
            Command::UpdateVendor(id, input) => Action::UpdateVendor(*id, input),
            Command::Customer(i) => Action::Customers(i),
            Command::UpdateCustomer(id, i) => Action::UpdateCustomer(*id, i),
            Command::List => Action::List,
            Command::Issue(i) => Action::Issue(i),
            Command::Preview(i) => Action::Preview(i),
            Command::Post(i) => Action::Post(i),
            Command::Settle(i) => Action::Settle(i),
            Command::Credit(i) => Action::Credit(i),
            Command::Refund(i) => Action::Refund(i),
            Command::Pdf(id) => Action::Pdf(*id),
        };
        let now = chrono::DateTime::<chrono::Utc>::from(std::time::SystemTime::now());
        match credential {
            Credential::Human(cookie) => {
                let attempt = owner
                    .sales(
                        &cookie,
                        &SystemClock,
                        action,
                        &now.format("%Y-%m-%d").to_string(),
                        &now.to_rfc3339(),
                        query.year.as_deref(),
                    )
                    .map_err(|_| ())?;
                Ok::<_, ()>(super::documents::outcome(attempt).map(|value| (value, None)))
            }
            Credential::Service(key) => {
                let attempt = owner
                    .sales_service(
                        &key,
                        &SystemClock,
                        rentemester_application::sales::Request {
                            action,
                            today: &now.format("%Y-%m-%d").to_string(),
                            now: &now.to_rfc3339(),
                            year: query.year.as_deref(),
                        },
                    )
                    .map_err(|_| ())?;
                Ok::<_, ()>(match attempt.outcome {
                    Ok(value) => Ok((value.result, Some((value.usage, value.business)))),
                    Err(f) => Err(Box::new(super::service_company::failure(f))),
                })
            }
        }
    })
    .await;
    let (result, stages) = match result {
        Ok(Ok(Ok(value))) => value,
        Ok(Ok(Err(response))) => return *response,
        _ => return refused(StatusCode::SERVICE_UNAVAILABLE, "outcome_unconfirmed"),
    };
    match result {
        Output::Json(value) => {
            let mut body = json!({"ok":true,key:value});
            if let Some((usage, business)) = stages {
                body["usage"] = json!(usage);
                body["business"] = json!(business);
            }
            response(StatusCode::OK, body)
        }
        Output::Pdf(bytes) => {
            let mut response = (StatusCode::OK, bytes).into_response();
            response.headers_mut().insert(
                "content-type",
                axum::http::HeaderValue::from_static("application/pdf"),
            );
            response.headers_mut().insert(
                "cache-control",
                axum::http::HeaderValue::from_static("private, no-store"),
            );
            response.headers_mut().insert(
                "x-content-type-options",
                axum::http::HeaderValue::from_static("nosniff"),
            );
            response
        }
    }
}
