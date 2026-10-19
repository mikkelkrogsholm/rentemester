//! Seven exact evidence-only actions and a global read, with current two-company gates.
use super::*;
use rentemester_runtime::auth::Clock;
use rentemester_storage::intercompany_dispositions as storage;
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
fn parse(action: &str, bytes: &[u8]) -> Result<storage::Action, ()> {
    if bytes.len() > 32 * 1024 {
        return Err(());
    }
    let Unique(body) = serde_json::from_slice::<Unique>(bytes).map_err(|_| ())?;
    if action != "plan" && body["confirm"] != true {
        return Err(());
    }
    match action {
        "plan" => serde_json::from_value(body).map(storage::Action::Plan),
        "propose" => serde_json::from_value(body).map(storage::Action::Propose),
        "approve" => serde_json::from_value(body).map(storage::Action::Approve),
        "link" => serde_json::from_value(body).map(storage::Action::Link),
        "settle" => serde_json::from_value(body).map(storage::Action::Settle),
        "supersede" => serde_json::from_value(body).map(storage::Action::Supersede),
        "reopen" => serde_json::from_value(body).map(storage::Action::Reopen),
        _ => return Err(()),
    }
    .map_err(|_| ())
}
fn query(raw: Option<&str>) -> Result<Option<String>, ()> {
    let pairs: Vec<_> = url::form_urlencoded::parse(raw.unwrap_or("").as_bytes()).collect();
    match pairs.as_slice() {
        [] => Ok(None),
        [(k, v)] if k == "asOf" && rentemester_engine::fiscal_year::Date::parse(v).is_ok() => {
            Ok(Some(v.to_string()))
        }
        _ => Err(()),
    }
}
pub(super) async fn status(
    State(state): State<Product>,
    axum::extract::Path(args): axum::extract::Path<std::collections::HashMap<String, String>>,
    request: Request,
) -> Response {
    let cookie = match proof(&state, &request, false) {
        Ok(p) => p,
        Err(r) => return *r,
    };
    let as_of = match query(request.uri().query()) {
        Ok(Some(d)) => d,
        Ok(None) => match SystemClock
            .now_unix_ms()
            .ok()
            .and_then(|now| rentemester_runtime::posting_time::latest_date(now, 0).ok())
        {
            Some(d) => d,
            None => return refused(StatusCode::SERVICE_UNAVAILABLE, "internal"),
        },
        Err(()) => return refused(StatusCode::BAD_REQUEST, "invalid_query"),
    };
    // Axum Path decodes exactly once; never apply company-slug rules or decode a second time.
    let Some(id) = args.get("id") else {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    };
    dispatch(
        state,
        cookie,
        None,
        storage::Action::Status {
            disposition_id: id.clone(),
            as_of,
        },
    )
    .await
}
pub(super) async fn handle(
    State(state): State<Product>,
    axum::extract::Path(args): axum::extract::Path<std::collections::HashMap<String, String>>,
    request: Request,
) -> Response {
    let name = args
        .get("action")
        .map(String::as_str)
        .unwrap_or_else(|| request.uri().path().rsplit('/').next().unwrap_or(""));
    let write = name != "plan";
    let cookie = match proof(&state, &request, write) {
        Ok(p) => p,
        Err(r) => return *r,
    };
    if !selected(&state, &request) {
        return refused(StatusCode::NOT_FOUND, "company_not_found");
    }
    if request.uri().query().is_some()
        || request.headers().contains_key("content-encoding")
        || request.headers().contains_key("idempotency-key")
        || !one(request.headers(), "content-type")
            .is_some_and(|v| v.eq_ignore_ascii_case("application/json"))
    {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    }
    let Some(slug) = args.get("slug").cloned() else {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    };
    let name = name.to_owned();
    let bytes = match to_bytes(request.into_body(), 32 * 1024).await {
        Ok(b) => b,
        Err(_) => return refused(StatusCode::PAYLOAD_TOO_LARGE, "invalid_input"),
    };
    let action = match parse(&name, &bytes) {
        Ok(a) => a,
        Err(()) => return refused(StatusCode::BAD_REQUEST, "invalid_input"),
    };
    dispatch(state, cookie, Some(slug), action).await
}
async fn dispatch(
    state: Product,
    cookie: rentemester_runtime::auth::VerifiedCookie,
    anchor: Option<String>,
    action: storage::Action,
) -> Response {
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let mut owner = state.native.owner.lock().map_err(|_| ())?;
        owner
            .intercompany_dispositions(
                &state.native.cookies,
                &SystemClock,
                &cookie,
                anchor.as_deref(),
                action,
            )
            .map_err(|_| ())
    })
    .await;
    match result {
        Ok(Ok(attempt)) => match attempt.outcome {
            Ok(v) => response(StatusCode::OK, v),
            Err(f) => {
                use rentemester_application::password_login::FailureStage;
                let code = f.error.code;
                let status = if f.stage == FailureStage::OutcomeUnknown
                    || !f.connection_reusable
                    || matches!(
                        code,
                        "disposition_source_unavailable" | "control_busy_or_unavailable"
                    ) {
                    StatusCode::SERVICE_UNAVAILABLE
                } else if matches!(
                    code,
                    "invalid_credentials" | "session_not_found" | "session_expired"
                ) {
                    StatusCode::UNAUTHORIZED
                } else if code == "access_denied" {
                    StatusCode::FORBIDDEN
                } else if matches!(
                    code,
                    "disposition_not_found" | "party_not_found" | "corporate_record_not_found"
                ) {
                    StatusCode::NOT_FOUND
                } else {
                    StatusCode::UNPROCESSABLE_ENTITY
                };
                refused(
                    status,
                    if status == StatusCode::SERVICE_UNAVAILABLE {
                        "internal"
                    } else {
                        code
                    },
                )
            }
        },
        _ => refused(StatusCode::SERVICE_UNAVAILABLE, "internal"),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_action_confirm_and_duplicate_fields() {
        assert!(parse("approve",br#"{"dispositionId":"abc","payloadHash":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","confirm":true}"#).is_ok());
        for body in [br#"{"dispositionId":"abc","payloadHash":"hash","confirm":false}"#.as_slice(),br#"{"dispositionId":"abc","payloadHash":"hash","confirm":true,"actor":"user:guessed"}"#,br#"{"dispositionId":"abc","dispositionId":"other","payloadHash":"hash","confirm":true}"#]{assert!(parse("approve",body).is_err());}
        assert!(
            serde_json::from_slice::<Unique>(
                br#"{"disposition":{"left":{"companySlug":"x","companySlug":"y"}}}"#
            )
            .is_err()
        );
        assert!(parse("plan", &vec![b' '; 32 * 1024 + 1]).is_err());
        assert!(query(Some("asOf=2026-10-20&asOf=2026-10-20")).is_err());
        assert!(query(Some("actor=guess")).is_err());
        assert_eq!(
            query(Some("asOf=2026-10-20")).unwrap(),
            Some("2026-10-20".into())
        );
    }
}
