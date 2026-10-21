//! Original confirmed lifecycle transport over the existing admitted sales owner.
use super::*;
use rentemester_storage::invoice_lifecycle::{self as storage, Kind};
use serde::de::{self, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};
use std::fmt;
struct Unique(Value);
impl<'de> Deserialize<'de> for Unique {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Unique;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("bounded JSON without duplicate keys")
            }
            fn visit_bool<E: de::Error>(self, v: bool) -> Result<Unique, E> {
                Ok(Unique(json!(v)))
            }
            fn visit_i64<E: de::Error>(self, v: i64) -> Result<Unique, E> {
                Ok(Unique(json!(v)))
            }
            fn visit_u64<E: de::Error>(self, v: u64) -> Result<Unique, E> {
                Ok(Unique(json!(v)))
            }
            fn visit_f64<E: de::Error>(self, v: f64) -> Result<Unique, E> {
                Ok(Unique(json!(v)))
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Unique, E> {
                Ok(Unique(json!(v)))
            }
            fn visit_string<E: de::Error>(self, v: String) -> Result<Unique, E> {
                Ok(Unique(json!(v)))
            }
            fn visit_unit<E: de::Error>(self) -> Result<Unique, E> {
                Ok(Unique(Value::Null))
            }
            fn visit_none<E: de::Error>(self) -> Result<Unique, E> {
                Ok(Unique(Value::Null))
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut a: A) -> Result<Unique, A::Error> {
                let mut values = vec![];
                while let Some(v) = a.next_element::<Unique>()? {
                    values.push(v.0);
                }
                Ok(Unique(Value::Array(values)))
            }
            fn visit_map<A: MapAccess<'de>>(self, mut a: A) -> Result<Unique, A::Error> {
                let mut values = serde_json::Map::new();
                while let Some(k) = a.next_key::<String>()? {
                    if values.contains_key(&k) {
                        return Err(de::Error::custom("duplicate JSON field"));
                    }
                    values.insert(k, a.next_value::<Unique>()?.0);
                }
                Ok(Unique(Value::Object(values)))
            }
        }
        d.deserialize_any(V)
    }
}

fn parse(path: &str, bytes: &[u8]) -> Result<(Kind, Value), &'static str> {
    let kind = match path.rsplit('/').next().unwrap_or("") {
        "issue" => Kind::Issue,
        "create" => Kind::Create,
        "render" => Kind::Render,
        "post" => Kind::Post,
        "settle-bank" => Kind::SettleBank,
        "credit-note" => Kind::CreditNote,
        "refund-bank" => Kind::RefundBank,
        _ => return Err("invalid_input"),
    };
    if bytes.len() > 256 * 1024 {
        return Err("invalid_input");
    }
    let Unique(value) = serde_json::from_slice(bytes).map_err(|_| "invalid_input")?;
    storage::validate(kind, &value).map_err(|e| {
        if e.code == "confirmation_required" {
            "confirmation_required"
        } else {
            "invalid_input"
        }
    })?;
    Ok((kind, value))
}
pub(super) async fn handle(State(state): State<Product>, request: Request) -> Response {
    let credential = match documents::credential(&state, &request, true) {
        Ok(c) => c,
        Err(r) => return *r,
    };
    if !selected(&state, &request) {
        return refused(StatusCode::NOT_FOUND, "company_not_found");
    }
    if request.method() != axum::http::Method::POST
        || request.uri().query().is_some()
        || request.headers().contains_key("content-encoding")
        || request.headers().contains_key("transfer-encoding")
        || request.headers().contains_key("idempotency-key")
        || !one(request.headers(), "content-type")
            .is_some_and(|s| s.eq_ignore_ascii_case("application/json"))
    {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    }
    let path = request.uri().path().to_owned();
    let slug = path.split('/').nth(3).unwrap_or("").to_owned();
    let Ok(bytes) = to_bytes(request.into_body(), 256 * 1024).await else {
        return refused(StatusCode::PAYLOAD_TOO_LARGE, "invalid_input");
    };
    let (kind, body) = match parse(&path, &bytes) {
        Ok(v) => v,
        Err(code) => return refused(StatusCode::BAD_REQUEST, code),
    };
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let mut owner = state.native.lock_company(&slug).map_err(|_| ())?;
        let action = rentemester_application::sales::Action::Lifecycle(kind, &body);
        match credential {
            documents::Credential::Human(cookie) => {
                let attempt = owner
                    .sales(&cookie, &SystemClock, action, "", "", None)
                    .map_err(|_| ())?;
                Ok::<_, ()>(documents::outcome(attempt))
            }
            documents::Credential::Service(key) => {
                let attempt = owner
                    .sales_service(
                        &key,
                        &SystemClock,
                        rentemester_application::sales::Request {
                            action,
                            today: "",
                            now: "",
                            year: None,
                        },
                    )
                    .map_err(|_| ())?;
                Ok(match attempt.outcome {
                    Ok(done) => Ok(done.result),
                    Err(f) => Err(Box::new(service_company::failure(f))),
                })
            }
        }
    })
    .await;
    match result {
        Ok(Ok(Ok(rentemester_application::sales::Output::Json(value)))) => {
            response(StatusCode::OK, value)
        }
        Ok(Ok(Err(r))) => *r,
        _ => refused(StatusCode::SERVICE_UNAVAILABLE, "unavailable"),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_lifecycle_confirm_unknown_duplicate_null_identity_and_selector_wire() {
        for raw in [
            r#"{"confirm":true,"documentId":1}"#,
            r#"{"confirm":true,"invoiceNumber":"2026-1","revenueAccountNo":"S"}"#,
        ] {
            assert!(parse("/post", raw.as_bytes()).is_ok());
        }
        for (path, raw) in [
            ("/post", r#"{"confirm":false,"documentId":1}"#),
            (
                "/render",
                r#"{"confirm":true,"documentId":1,"revenueAccountNo":"S"}"#,
            ),
            ("/post", r#"{"confirm":true,"documentId":null}"#),
            ("/post", r#"{"confirm":true,"documentId":1,"documentId":2}"#),
            (
                "/post",
                r#"{"confirm":true,"documentId":1,"actor":"user:other"}"#,
            ),
            (
                "/settle-bank",
                r#"{"confirm":true,"payload":{"invoiceDocumentId":1,"bankTransactionId":2,"amount":"125"}}"#,
            ),
            (
                "/issue",
                r#"{"confirm":true,"payload":{"invoiceType":"full","issueDate":"2026-01-01","lines":[{"description":"a","description":"b"}]}}"#,
            ),
        ] {
            assert!(parse(path, raw.as_bytes()).is_err(), "{raw}");
        }
    }
}
