//! Additive cookie-only mileage posting routes. All accounting remains in the owner.
use super::*;
use rentemester_application::mileage_posting::Action;
use rentemester_storage::mileage_posting::{Configure, Post, Preview};
enum Command {
    RateVersions,
    Configure(Configure),
    Preview(Preview),
    Post(Post),
    Postings,
    Invalid,
}
impl Command {
    fn action(&self) -> Action<'_> {
        match self {
            Self::RateVersions | Self::Invalid => Action::RateVersions,
            Self::Configure(v) => Action::Configure(v),
            Self::Preview(v) => Action::Preview(v),
            Self::Post(v) => Action::Post(v),
            Self::Postings => Action::Postings,
        }
    }
}
// Top-level duplicate rejection is explicit even if the backend changes serde strategy.
struct Wire(Value);
impl<'de> Deserialize<'de> for Wire {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = Wire;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("unique mileage posting object")
            }
            fn visit_map<M: serde::de::MapAccess<'de>>(self, mut map: M) -> Result<Wire, M::Error> {
                let mut fields = serde_json::Map::new();
                while let Some((k, v)) = map.next_entry::<String, Value>()? {
                    if fields.insert(k, v).is_some() {
                        return Err(serde::de::Error::custom("duplicate field"));
                    }
                }
                Ok(Wire(Value::Object(fields)))
            }
        }
        d.deserialize_map(Visitor)
    }
}
fn parse(bytes: &[u8], suffix: &str) -> Command {
    let Ok(Wire(v)) = serde_json::from_slice(bytes) else {
        return Command::Invalid;
    };
    let fields: &[&str] = match suffix {
        "rate-versions" => &[
            "version",
            "validFrom",
            "validTo",
            "vehicle",
            "ratePerKm",
            "basis",
            "source",
            "expenseAccountNo",
            "liabilityAccountNo",
            "confirm",
        ],
        "post-preview" => &["mileageEntryId", "rateVersion", "documentId"],
        "post" => &[
            "mileageEntryId",
            "rateVersion",
            "documentId",
            "expectedPreviewHash",
            "operationKey",
            "confirm",
        ],
        _ => return Command::Invalid,
    };
    let m = v.as_object().unwrap();
    if m.len() != fields.len() || m.keys().any(|k| !fields.contains(&k.as_str())) {
        return Command::Invalid;
    }
    for (k, v) in m {
        let valid = match k.as_str() {
            "confirm" => v == true,
            "mileageEntryId" | "documentId" => v
                .as_i64()
                .is_some_and(|n| (1..=9_007_199_254_740_991).contains(&n)),
            _ => v.as_str().is_some_and(|s| {
                !s.trim().is_empty()
                    && !s.chars().any(char::is_control)
                    && s.len()
                        <= if matches!(
                            k.as_str(),
                            "version"
                                | "rateVersion"
                                | "operationKey"
                                | "expenseAccountNo"
                                | "liabilityAccountNo"
                        ) {
                            128
                        } else {
                            4096
                        }
            }),
        };
        if !valid {
            return Command::Invalid;
        }
    }
    if suffix == "post"
        && !v["expectedPreviewHash"].as_str().is_some_and(|s| {
            s.len() == 64
                && s.bytes()
                    .all(|b| b.is_ascii_digit() || matches!(b, b'a'..=b'f'))
        })
    {
        return Command::Invalid;
    }
    match suffix {
        "rate-versions" => serde_json::from_value(v)
            .map(Command::Configure)
            .unwrap_or(Command::Invalid),
        "post-preview" => serde_json::from_value(v)
            .map(Command::Preview)
            .unwrap_or(Command::Invalid),
        "post" => serde_json::from_value(v)
            .map(Command::Post)
            .unwrap_or(Command::Invalid),
        _ => Command::Invalid,
    }
}
pub(super) async fn handle(State(state): State<Product>, request: Request) -> Response {
    let post = request.method() == axum::http::Method::POST;
    let cookie = match proof(&state, &request, post) {
        Ok(v) => v,
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
    let suffix = request
        .uri()
        .path()
        .rsplit('/')
        .next()
        .unwrap_or("")
        .to_owned();
    let valid = request.uri().query().is_none()
        && !request.headers().contains_key("content-encoding")
        && !request.headers().contains_key("idempotency-key")
        && (!post
            || one(request.headers(), "content-type")
                .is_some_and(|s| s.eq_ignore_ascii_case("application/json")));
    let command = match to_bytes(request.into_body(), 32 * 1024).await {
        Ok(b) if valid && post => parse(&b, &suffix),
        Ok(b) if valid && b.is_empty() => match suffix.as_str() {
            "rate-versions" => Command::RateVersions,
            "postings" => Command::Postings,
            _ => Command::Invalid,
        },
        _ => Command::Invalid,
    };
    let Ok(now) = rentemester_runtime::auth::Clock::now_unix_ms(&SystemClock) else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "clock_unavailable");
    };
    let Ok(today) = rentemester_runtime::posting_time::latest_date(now, 0) else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "clock_unavailable");
    };
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let mut owner = state.native.lock_company(&slug).map_err(|_| ())?;
        let invalid = matches!(command, Command::Invalid);
        // Malformed requests still pass current company read admission; no mutation action is invoked.
        let attempt = owner
            .mileage_posting(&cookie, &SystemClock, command.action(), &today)
            .map_err(|_| ())?;
        if let Err(f) = &attempt.outcome {
            if matches!(f.error.code, "invalid_credentials" | "access_denied") {
                return Ok::<_, ()>(refused(
                    if f.error.code == "invalid_credentials" {
                        StatusCode::UNAUTHORIZED
                    } else {
                        StatusCode::FORBIDDEN
                    },
                    f.error.code,
                ));
            }
        }
        if attempt.retired
            || attempt.outcome.as_ref().is_ok_and(|v| !v.control_reusable)
            || attempt
                .outcome
                .as_ref()
                .is_err_and(|f| !f.control_reusable || !f.company_reusable)
        {
            return Ok(refused(StatusCode::SERVICE_UNAVAILABLE, "outcome_unknown"));
        }
        Ok::<_, ()>(match documents::outcome(attempt) {
            Ok(_) if invalid => refused(StatusCode::BAD_REQUEST, "invalid_input"),
            Ok(value) => response(StatusCode::OK, json!({"ok":true,"mileagePosting":value})),
            Err(r) => *r,
        })
    })
    .await;
    match result {
        Ok(Ok(r)) => r,
        _ => refused(StatusCode::SERVICE_UNAVAILABLE, "outcome_unknown"),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mileage_posting_strict_wire() {
        assert!(matches!(
            parse(
                br#"{"mileageEntryId":1,"rateVersion":"v1","documentId":2}"#,
                "post-preview"
            ),
            Command::Preview(_)
        ));
        for b in [
            br#"{"mileageEntryId":1,"mileageEntryId":1,"rateVersion":"v1","documentId":2}"#
                .as_slice(),
            br#"{"mileageEntryId":1,"rateVersion":"v1","documentId":2,"actor":"forged"}"#,
            br#"{"mileageEntryId":9007199254740992,"rateVersion":"v1","documentId":2}"#,
        ] {
            assert!(matches!(parse(b, "post-preview"), Command::Invalid));
        }
    }
}
