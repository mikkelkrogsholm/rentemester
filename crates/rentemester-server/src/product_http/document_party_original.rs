//! Source-shaped document party review; the plan POST is read-only and never writes evidence.
use super::*;
use rentemester_application::document_party_links::Action;
use rentemester_storage::document_party_links::{
    Apply, Internal, InternalSupersede, PlanInput, Supersede,
};
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Assertions {
    expected_actor: Option<String>,
    expected_principal: Option<String>,
}
enum Command {
    List(Option<String>, usize),
    History(i64),
    Plan(PlanInput),
    Apply(Apply),
    Supersede(Supersede),
    Internal(Internal),
    InternalSupersede(InternalSupersede),
}
pub(super) async fn handle(State(state): State<Product>, request: Request) -> Response {
    let path = request.uri().path().to_owned();
    let pieces: Vec<_> = path.split('/').collect();
    let slug = pieces.get(3).unwrap_or(&"").to_string();
    let operation = pieces.last().copied().unwrap_or("").to_owned();
    let post = request.method() == axum::http::Method::POST;
    let credential = match if super::service_company::requested(&request) {
        super::service_company::proof(&state, &request, post && operation != "plan")
            .map(documents::Credential::Service)
    } else {
        proof(&state, &request, post && operation != "plan").map(documents::Credential::Human)
    } {
        Ok(p) => p,
        Err(r) => return *r,
    };
    if !selected(&state, &request) {
        return refused(StatusCode::NOT_FOUND, "company_not_found");
    }
    let mut assertions = Assertions::default();
    let command = if post {
        if request.uri().query().is_some()
            || request.headers().contains_key("content-encoding")
            || !one(request.headers(), "content-type")
                .is_some_and(|s| s.eq_ignore_ascii_case("application/json"))
        {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        }
        let Ok(bytes) = to_bytes(request.into_body(), 16 * 1024).await else {
            return refused(StatusCode::PAYLOAD_TOO_LARGE, "invalid_input");
        };
        let Ok(mut value) = serde_json::from_slice::<Value>(&bytes) else {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        };
        if operation != "plan" {
            let Some(fields) = value.as_object_mut() else {
                return refused(StatusCode::BAD_REQUEST, "invalid_input");
            };
            let mut projected = serde_json::Map::new();
            for key in ["expectedActor", "expectedPrincipal"] {
                if let Some(v) = fields.remove(key) {
                    if !v
                        .as_str()
                        .is_some_and(|s| !s.trim().is_empty() && s.len() <= 160)
                    {
                        return refused(StatusCode::BAD_REQUEST, "invalid_input");
                    }
                    projected.insert(key.into(), v);
                }
            }
            assertions = serde_json::from_value(Value::Object(projected)).unwrap();
            if value["confirm"] != true {
                return refused(StatusCode::BAD_REQUEST, "CONFIRMATION_REQUIRED");
            }
        }
        if !value["documentId"]
            .as_i64()
            .is_some_and(|n| n > 0 && n <= 9_007_199_254_740_991)
        {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        }
        for key in ["planHash", "decisionHash"] {
            if let Some(v) = value.get(key) {
                if !v.as_str().is_some_and(|s| {
                    s.len() == 64
                        && s.bytes()
                            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                }) {
                    return refused(StatusCode::BAD_REQUEST, "invalid_input");
                }
            }
        }
        for (key, min, max) in [
            ("partyId", 3, 64),
            ("jurisdiction", 2, 2),
            ("identifier", 1, 160),
            ("legacyId", 1, 160),
            ("reviewedLegacyReference", 1, 500),
            ("reason", 1, 1000),
            ("idempotencyKey", 1, 128),
        ] {
            if let Some(v) = value.get(key) {
                if !v
                    .as_str()
                    .is_some_and(|s| s.len() >= min && s.len() <= max && !s.trim().is_empty())
                {
                    return refused(StatusCode::BAD_REQUEST, "invalid_input");
                }
            }
        }
        for (key, allowed) in [
            ("identifierKind", &["dk_cvr", "eu_vat", "non_eu"][..]),
            ("legacyKind", &["customer", "vendor"][..]),
            ("role", rentemester_engine::parties::ROLES),
        ] {
            if let Some(v) = value.get(key) {
                if !v.as_str().is_some_and(|s| allowed.contains(&s)) {
                    return refused(StatusCode::BAD_REQUEST, "invalid_input");
                }
            }
        }
        let bytes = serde_json::to_vec(&value).unwrap();
        let c = match operation.as_str() {
            "plan" => serde_json::from_slice(&bytes).map(Command::Plan),
            "apply" => serde_json::from_slice(&bytes).map(Command::Apply),
            "supersede" => serde_json::from_slice(&bytes).map(Command::Supersede),
            "internal" => serde_json::from_slice(&bytes).map(Command::Internal),
            "internal-supersede" => serde_json::from_slice(&bytes).map(Command::InternalSupersede),
            _ => return refused(StatusCode::NOT_FOUND, "not_found"),
        };
        match c {
            Ok(c) => c,
            Err(_) => return refused(StatusCode::BAD_REQUEST, "invalid_input"),
        }
    } else if pieces.len() == 7 {
        let mut status = None;

        for (k, v) in url::form_urlencoded::parse(request.uri().query().unwrap_or("").as_bytes()) {
            match k.as_ref() {
                "status"
                    if status.is_none()
                        && [
                            "linked",
                            "unlinked",
                            "resolved",
                            "unresolved",
                            "internal_no_external_party",
                        ]
                        .contains(&v.as_ref()) =>
                {
                    status = Some(v.into_owned())
                }
                _ => return refused(StatusCode::BAD_REQUEST, "invalid_query"),
            }
        }
        Command::List(status, 100)
    } else {
        if request.uri().query().is_some() {
            return refused(StatusCode::BAD_REQUEST, "invalid_query");
        }
        let Some(id) = pieces
            .get(7)
            .and_then(|s| s.parse::<i64>().ok())
            .filter(|id| *id > 0 && *id <= 9_007_199_254_740_991)
        else {
            return refused(StatusCode::BAD_REQUEST, "invalid_document");
        };
        Command::History(id)
    };
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let mut owner = state.native.lock_company(&slug).map_err(|_| ())?;
        let action = match &command {
            Command::List(s, l) => Action::List(s.as_deref(), *l),
            Command::History(id) => Action::History(*id),
            Command::Plan(i) => Action::Plan(i),
            Command::Apply(i) => Action::Apply(i),
            Command::Supersede(i) => Action::Supersede(i),
            Command::Internal(i) => Action::Internal(i),
            Command::InternalSupersede(i) => Action::InternalSupersede(i),
        };
        let asserted = Action::Asserted(
            &action,
            assertions.expected_actor.as_deref(),
            assertions.expected_principal.as_deref(),
        );
        let result = match credential {
            documents::Credential::Human(cookie) => documents::outcome(
                owner
                    .document_party_links(&cookie, &SystemClock, asserted)
                    .map_err(|_| ())?,
            ),
            documents::Credential::Service(key) => documents_original::service_outcome(
                owner
                    .document_party_links_service(&key, &SystemClock, asserted)
                    .map_err(|_| ())?,
            ),
        };
        Ok::<_, ()>(result.map(|mut v| {
            // Source projection only: native history keeps its established receipt key.
            if matches!(command, Command::History(_)) {
                if let Some(rows) = v["links"].as_array_mut() {
                    for row in rows {
                        if let Some(o) = row.as_object_mut() {
                            o.remove("idempotency_key");
                        }
                    }
                }
            }
            if matches!(command, Command::Supersede(_)) {
                if let Some(o) = v.as_object_mut() {
                    o.remove("planHash");
                }
            }
            v
        }))
    })
    .await;
    match result {
        Ok(Ok(attempt)) => match attempt {
            Ok(v) => response(StatusCode::OK, v),
            Err(r) => *r,
        },
        _ => refused(StatusCode::SERVICE_UNAVAILABLE, "outcome_unconfirmed"),
    }
}
