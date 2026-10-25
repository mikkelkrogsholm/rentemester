use super::*;
use rentemester_application::mail_intake::original717::{Input, Message};
use rentemester_runtime::imap::{self, original717::ConnectionInput};
pub(crate) async fn handle(State(state): State<Product>, request: Request) -> Response {
    let cookie = match proof(&state, &request, true) {
        Ok(v) => v,
        Err(r) => return *r,
    };
    if !selected(&state, &request) {
        return refused(StatusCode::NOT_FOUND, "company_not_found");
    }
    if request.uri().query().is_some()
        || request.headers().contains_key("content-encoding")
        || !one(request.headers(), "content-type")
            .is_some_and(|s| s.eq_ignore_ascii_case("application/json"))
    {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    }
    let slug = request
        .uri()
        .path()
        .split('/')
        .nth(3)
        .unwrap_or("")
        .to_owned();
    let poll = request.uri().path().ends_with("original-imap-intake");
    let Ok(bytes) = to_bytes(request.into_body(), 90 * 1024 * 1024).await else {
        return refused(StatusCode::PAYLOAD_TOO_LARGE, "invalid_input");
    };
    let Ok(mut body) = serde_json::from_slice::<Value>(&bytes) else {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    };
    let connection = if poll {
        let Some(fields) = body.as_object_mut() else {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        };
        let mut connection = json!({});
        for k in [
            "imapHost",
            "imapPort",
            "imapUsername",
            "imapMailbox",
            "sinceUid",
        ] {
            if let Some(v) = fields.remove(k) {
                connection[k] = v;
            }
        }
        fields.insert("messages".into(), json!([]));
        match serde_json::from_value::<ConnectionInput>(connection) {
            Ok(c) => Some(c),
            Err(_) => return refused(StatusCode::BAD_REQUEST, "invalid_input"),
        }
    } else {
        None
    };
    let Ok(mut input) = serde_json::from_value::<Input>(body) else {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    };
    if !input.confirm {
        return response(
            StatusCode::OK,
            json!({"ok":true,"result":{"ok":false,"errors":["confirm: true is required"]}}),
        );
    }
    // The existing immediate source ingest has no clean-scanner bypass.
    if state.native.document_scanner.is_some() {
        return refused(
            StatusCode::UNPROCESSABLE_ENTITY,
            "document_scanner_required",
        );
    }
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    match tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let attempt = if let Some(connection) = connection {
            let config =
                match imap::original717::resolve(&connection, |key| std::env::var(key).ok()) {
                    Ok(c) => c,
                    Err(e) => {
                        return response(
                            StatusCode::OK,
                            json!({"ok":true,"result":{"ok":false,"errors":e}}),
                        );
                    }
                };
            let admission = {
                let Ok(mut owner) = state.native.lock_company(&slug) else {
                    return refused(StatusCode::SERVICE_UNAVAILABLE, "owner_unavailable");
                };
                match owner.original717_prepare_imap_intake_poll(
                    &state.native.cookies,
                    &cookie,
                    &SystemClock,
                    config,
                ) {
                    Ok(a) if !a.retired => match a.outcome {
                        Ok(c) if c.control_reusable => c.value,
                        Err(f) => return failed(f),
                        _ => return refused(StatusCode::SERVICE_UNAVAILABLE, "outcome_unknown"),
                    },
                    _ => return refused(StatusCode::SERVICE_UNAVAILABLE, "outcome_unknown"),
                }
            };
            let mut network = match NetworkClient::new(imap::OperatorConfig::default()) {
                Ok(c) => c,
                Err(_) => return refused(StatusCode::SERVICE_UNAVAILABLE, "imap_unavailable"),
            };
            // A UID floor is ephemeral; it never replaces configured-native state.
            let batch = match network.fetch(
                admission.config(),
                connection.since_uid.map(|uid| imap::Cursor {
                    uid_validity: 0,
                    last_uid: uid,
                }),
            ) {
                Ok(b) => b,
                Err(e) => {
                    return response(
                        StatusCode::OK,
                        json!({"ok":true,"result":{"ok":false,"errors":[e.code()]}}),
                    );
                }
            };
            input.messages = batch
                .messages
                .into_iter()
                .filter(|m| connection.since_uid.is_none_or(|floor| m.uid > floor))
                .map(|m| Message {
                    source: format!("imap:{}", m.uid),
                    base64: imap::original717::encode_eml(&m.raw),
                })
                .collect();
            let fetched = input.messages.len();
            let Ok(mut owner) = state.native.lock_company(&slug) else {
                return refused(StatusCode::SERVICE_UNAVAILABLE, "owner_unavailable");
            };
            let a = owner.original717_finish_imap_intake_poll(
                &state.native.cookies,
                &cookie,
                &SystemClock,
                admission,
                &input,
            );
            match a {
                Ok(mut a) => {
                    if let Ok(c) = a.outcome.as_mut() {
                        c.value["messagesFetched"] = json!(fetched);
                    }
                    Ok(a)
                }
                Err(e) => Err(e),
            }
        } else {
            let Ok(mut owner) = state.native.lock_company(&slug) else {
                return refused(StatusCode::SERVICE_UNAVAILABLE, "owner_unavailable");
            };
            owner.original717_mail_intake_ingest(
                &state.native.cookies,
                &cookie,
                &SystemClock,
                &input,
            )
        };
        match attempt {
            Ok(a) => match super::super::documents::outcome(a) {
                Ok(v) => response(StatusCode::OK, json!({"ok":true,"result":v})),
                Err(r) => *r,
            },
            Err(_) => refused(StatusCode::SERVICE_UNAVAILABLE, "outcome_unknown"),
        }
    })
    .await
    {
        Ok(r) => r,
        Err(_) => refused(StatusCode::SERVICE_UNAVAILABLE, "outcome_unknown"),
    }
}
