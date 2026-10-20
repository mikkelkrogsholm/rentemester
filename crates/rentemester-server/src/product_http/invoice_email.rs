//! Explicit external SMTP side effect over the admitted owner's durable reservation.
use super::documents::{Credential, credential, service_envelope};
use super::*;
use rentemester_storage::invoice_email::{Reconcile, Retry, Send};
enum Command {
    Status,
    Send(Send),
    Reconcile(Reconcile),
    Retry(Retry),
}
pub(super) async fn handle(State(state): State<Product>, request: Request) -> Response {
    let path = request.uri().path().to_owned();
    let slug = path.split('/').nth(3).unwrap_or("").to_owned();
    let write = request.method() == axum::http::Method::POST;
    let credential = match credential(&state, &request, write) {
        Ok(v) => v,
        Err(r) => return *r,
    };
    if !selected(&state, &request) {
        return refused(StatusCode::NOT_FOUND, "company_not_found");
    }
    if request.uri().query().is_some() || request.headers().contains_key("content-encoding") {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    }
    let command = if write {
        if !one(request.headers(), "content-type")
            .is_some_and(|v| v.eq_ignore_ascii_case("application/json"))
        {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        }
        let Ok(bytes) = to_bytes(request.into_body(), 16 * 1024).await else {
            return refused(StatusCode::PAYLOAD_TOO_LARGE, "invalid_input");
        };
        let parsed = match path.rsplit('/').next() {
            Some("send-email") => serde_json::from_slice(&bytes).map(Command::Send),
            Some("reconcile") => serde_json::from_slice(&bytes).map(Command::Reconcile),
            Some("retry") => serde_json::from_slice(&bytes).map(Command::Retry),
            _ => return refused(StatusCode::NOT_FOUND, "not_found"),
        };
        match parsed {
            Ok(c) => c,
            Err(_) => return refused(StatusCode::BAD_REQUEST, "invalid_input"),
        }
    } else {
        match to_bytes(request.into_body(), 1).await {
            Ok(b) if b.is_empty() => {}
            _ => return refused(StatusCode::BAD_REQUEST, "invalid_input"),
        };
        Command::Status
    };
    let configured = state.native.invoice_mail.contains_key(&slug);
    // Missing operator configuration can never reserve or initiate a delivery.
    if matches!(command, Command::Send(_) | Command::Retry(_)) && !configured {
        return refused(StatusCode::UNPROCESSABLE_ENTITY, "smtp_config_required");
    }
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let status = matches!(command, Command::Status);
    let reconciliation = matches!(command, Command::Reconcile(_));
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let mut owner = state.native.lock_company(&slug).map_err(|_| ())?;
        let value = match &credential {
            Credential::Human(cookie) => {
                let attempt = match &command {
                    Command::Status => owner.invoice_email_status(cookie, &SystemClock),
                    Command::Reconcile(input) => {
                        owner.reconcile_invoice_email(cookie, &SystemClock, input)
                    }
                    Command::Send(_) | Command::Retry(_) => {
                        let mut sender = state
                            .native
                            .invoice_mail
                            .get(&slug)
                            .ok_or(())?
                            .lock()
                            .map_err(|_| ())?;
                        match &command {
                            Command::Send(input) => {
                                owner.send_invoice_email(cookie, &SystemClock, input, &mut *sender)
                            }
                            Command::Retry(input) => {
                                owner.retry_invoice_email(cookie, &SystemClock, input, &mut *sender)
                            }
                            _ => unreachable!(),
                        }
                    }
                }
                .map_err(|_| ())?;
                super::documents::outcome(attempt)
            }
            Credential::Service(key) => {
                let attempt = match &command {
                    Command::Status => owner.invoice_email_status_service(key, &SystemClock),
                    Command::Reconcile(input) => {
                        owner.reconcile_invoice_email_service(key, &SystemClock, input)
                    }
                    Command::Send(_) | Command::Retry(_) => {
                        let mut sender = state
                            .native
                            .invoice_mail
                            .get(&slug)
                            .ok_or(())?
                            .lock()
                            .map_err(|_| ())?;
                        match &command {
                            Command::Send(input) => owner.send_invoice_email_service(
                                key,
                                &SystemClock,
                                input,
                                &mut *sender,
                            ),
                            Command::Retry(input) => owner.retry_invoice_email_service(
                                key,
                                &SystemClock,
                                input,
                                &mut *sender,
                            ),
                            _ => unreachable!(),
                        }
                    }
                }
                .map_err(|_| ())?;
                service_envelope(attempt, None)
            }
        };
        Ok::<_, ()>(value)
    })
    .await;
    match result {
        Ok(Ok(value)) => match value {
            Ok(value) if status => {
                let mut body = json!({"ok":true,"invoiceEmail":{"configured":configured,"rows":value["rows"]}});
                for field in ["usage", "business"] {
                    if let Some(stage) = value.get(field) {
                        body[field] = stage.clone();
                    }
                }
                response(StatusCode::OK, body)
            }
            Ok(mut value) => {
                let delivery_state = value["delivery"]["state"].as_str().unwrap_or("unknown");
                let (http, code) = if reconciliation || delivery_state == "accepted" {
                    (StatusCode::OK, None)
                } else if delivery_state == "refused" {
                    (StatusCode::UNPROCESSABLE_ENTITY, Some("delivery_refused"))
                } else {
                    (StatusCode::CONFLICT, Some("delivery_unknown"))
                };
                value["ok"] = json!(code.is_none());
                if let Some(code) = code {
                    value["code"] = json!(code);
                    value["message"] = json!(if code == "delivery_unknown" {
                        "SMTP-resultatet er ukendt. Afklar den eksisterende afsendelse før et nyt forsøg."
                    } else {
                        "SMTP-serveren afviste afsendelsen. Et nyt forsøg kræver særskilt gennemgang."
                    });
                }
                response(http, value)
            }
            Err(r) => *r,
        },
        _ => refused(StatusCode::SERVICE_UNAVAILABLE, "outcome_unconfirmed"),
    }
}
