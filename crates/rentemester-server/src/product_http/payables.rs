use super::*;
use rentemester_application::expense::Action;
enum Command {
    List { status: String, as_of: String },
    Register(rentemester_storage::payables::Register),
    Pay(i64, rentemester_storage::payables::Payment),
}
pub(super) async fn handle(State(state): State<Product>, request: Request) -> Response {
    let mutation = request.method() != axum::http::Method::GET;
    let cookie = match proof(&state, &request, mutation) {
        Ok(c) => c,
        Err(r) => return *r,
    };
    if !selected(&state, &request) {
        return refused(StatusCode::NOT_FOUND, "company_not_found");
    }
    let today = chrono::DateTime::<chrono::Utc>::from(std::time::SystemTime::now())
        .format("%Y-%m-%d")
        .to_string();
    let command = if mutation {
        if request.uri().query().is_some()
            || request.headers().contains_key("content-encoding")
            || !one(request.headers(), "content-type")
                .is_some_and(|s| s.eq_ignore_ascii_case("application/json"))
        {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        }
        let path = request.uri().path().to_owned();
        let Ok(bytes) = to_bytes(request.into_body(), 256 * 1024).await else {
            return refused(StatusCode::PAYLOAD_TOO_LARGE, "invalid_input");
        };
        if path.ends_with("/pay") {
            let Some(id) = path
                .rsplit('/')
                .nth(1)
                .and_then(|s| s.parse::<i64>().ok())
                .filter(|n| *n > 0)
            else {
                return refused(StatusCode::BAD_REQUEST, "invalid_input");
            };
            match serde_json::from_slice(&bytes) {
                Ok(input) => Command::Pay(id, input),
                Err(_) => return refused(StatusCode::BAD_REQUEST, "invalid_input"),
            }
        } else {
            match serde_json::from_slice(&bytes) {
                Ok(input) => Command::Register(input),
                Err(_) => return refused(StatusCode::BAD_REQUEST, "invalid_input"),
            }
        }
    } else {
        let mut status = None;
        let mut as_of = None;
        for (key, value) in
            url::form_urlencoded::parse(request.uri().query().unwrap_or("").as_bytes())
        {
            let slot = match key.as_ref() {
                "status" => &mut status,
                "asOf" => &mut as_of,
                _ => return refused(StatusCode::BAD_REQUEST, "invalid_query"),
            };
            if slot.is_some() || value.len() > 32 {
                return refused(StatusCode::BAD_REQUEST, "invalid_query");
            }
            *slot = Some(value.into_owned());
        }
        Command::List {
            status: status.unwrap_or_else(|| "open".into()),
            as_of: as_of.unwrap_or(today),
        }
    };
    let key = match command {
        Command::List { .. } => "payables",
        Command::Register(_) => "payable",
        Command::Pay(_, _) => "payment",
    };
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let mut owner = state.native.owner.lock().map_err(|_| ())?;
        let action = match &command {
            Command::List { status, as_of } => Action::Payables { as_of, status },
            Command::Register(input) => Action::Register(input),
            Command::Pay(id, input) => Action::Pay(*id, input),
        };
        owner.expense(&cookie, &SystemClock, action).map_err(|_| ())
    })
    .await;
    match result {
        Ok(Ok(attempt)) => match super::documents::outcome(attempt) {
            Ok(value) => response(StatusCode::OK, json!({"ok":true,key:value})),
            Err(r) => *r,
        },
        _ => refused(StatusCode::SERVICE_UNAVAILABLE, "outcome_unconfirmed"),
    }
}
