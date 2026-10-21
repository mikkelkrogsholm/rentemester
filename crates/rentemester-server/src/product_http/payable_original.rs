//! Strict original payable transport over the admitted expense owner.
use super::*;
use rentemester_storage::payable_original::{Action, Pay, Query, Register};
enum Command {
    List(Query),
    Register(Register),
    Pay(Pay),
}
impl Command {
    fn action(&self) -> Action<'_> {
        match self {
            Self::List(i) => Action::List(i),
            Self::Register(i) => Action::Register(i),
            Self::Pay(i) => Action::Pay(i),
        }
    }
}
fn query(raw: &str) -> Result<Query, ()> {
    if raw.len() > 4096 {
        return Err(());
    }
    let bytes = raw.as_bytes();
    for (i, b) in bytes.iter().enumerate() {
        if *b == b'%'
            && (i + 2 >= bytes.len()
                || !bytes[i + 1].is_ascii_hexdigit()
                || !bytes[i + 2].is_ascii_hexdigit())
        {
            return Err(());
        }
    }
    let mut q = Query::default();
    let mut seen = std::collections::BTreeSet::new();
    for (key, value) in url::form_urlencoded::parse(bytes) {
        if !seen.insert(key.to_string())
            || value.is_empty()
            || value.len() > 1024
            || value.chars().any(char::is_control)
        {
            return Err(());
        }
        let value = value.into_owned();
        match key.as_ref() {
            "status" => q.status = Some(value),
            "asOf" => q.as_of = Some(value),
            "asOfDate" => q.as_of_date = Some(value),
            "supplier" => q.supplier = Some(value),
            "from" => q.from = Some(value),
            "to" => q.to = Some(value),
            "vendorId" => {
                if !value.bytes().all(|b| b.is_ascii_digit()) {
                    return Err(());
                }
                let n = value.parse().map_err(|_| ())?;
                if !(1..=9_007_199_254_740_991).contains(&n) {
                    return Err(());
                }
                q.vendor_id = Some(n)
            }
            "minDays" => {
                if !value.bytes().all(|b| b.is_ascii_digit()) {
                    return Err(());
                }
                let n = value.parse().map_err(|_| ())?;
                if n > 9_007_199_254_740_991 {
                    return Err(());
                }
                q.min_days = Some(n)
            }
            _ => return Err(()),
        }
    }
    Ok(q)
}
fn body(path: &str, bytes: &[u8]) -> Result<Command, &'static str> {
    if path.ends_with("/register") {
        let i: Register = serde_json::from_slice(bytes).map_err(|_| "invalid_input")?;
        if !i.confirm {
            return Err("confirmation_required");
        }
        i.validate().map_err(|_| "invalid_input")?;
        Ok(Command::Register(i))
    } else {
        let i: Pay = serde_json::from_slice(bytes).map_err(|_| "invalid_input")?;
        if !i.confirm {
            return Err("confirmation_required");
        }
        i.validate().map_err(|_| "invalid_input")?;
        Ok(Command::Pay(i))
    }
}
pub(super) async fn handle(State(state): State<Product>, request: Request) -> Response {
    let mutation = request.method() == axum::http::Method::POST;
    let credential = match documents::credential(&state, &request, mutation) {
        Ok(c) => c,
        Err(r) => return *r,
    };
    if !selected(&state, &request) {
        return refused(StatusCode::NOT_FOUND, "company_not_found");
    }
    let path = request.uri().path().to_owned();
    let slug = path.split('/').nth(3).unwrap_or("").to_owned();
    let command = if mutation {
        if request.uri().query().is_some()
            || request.headers().contains_key("content-encoding")
            || request.headers().contains_key("idempotency-key")
            || !one(request.headers(), "content-type")
                .is_some_and(|s| s.eq_ignore_ascii_case("application/json"))
        {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        }
        let bytes = match to_bytes(request.into_body(), 16 * 1024).await {
            Ok(b) => b,
            Err(_) => return refused(StatusCode::PAYLOAD_TOO_LARGE, "body_too_large"),
        };
        match body(&path, &bytes) {
            Ok(i) => i,
            Err(code) => return refused(StatusCode::BAD_REQUEST, code),
        }
    } else {
        if request.method() != axum::http::Method::GET
            || request.headers().contains_key("content-encoding")
            || request.headers().contains_key("transfer-encoding")
            || request.headers().contains_key("idempotency-key")
            || one(request.headers(), "content-length").is_some_and(|s| s != "0")
        {
            return refused(StatusCode::BAD_REQUEST, "invalid_query");
        }
        let q = match query(request.uri().query().unwrap_or("")) {
            Ok(q) => q,
            Err(()) => return refused(StatusCode::BAD_REQUEST, "invalid_query"),
        };
        if !matches!(to_bytes(request.into_body(),0).await,Ok(b)if b.is_empty()) {
            return refused(StatusCode::BAD_REQUEST, "body_not_allowed");
        }
        Command::List(q)
    };
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let mut owner = state.native.lock_company(&slug).map_err(|_| ())?;
        let action = rentemester_application::expense::Action::Original(command.action());
        Ok::<_, ()>(match credential {
            documents::Credential::Human(cookie) => documents::outcome(
                owner
                    .expense(&cookie, &SystemClock, action)
                    .map_err(|_| ())?,
            ),
            documents::Credential::Service(key) => documents::service_outcome(
                owner
                    .expense_service(&key, &SystemClock, action)
                    .map_err(|_| ())?,
            ),
        })
    })
    .await;
    match result {
        Ok(Ok(Ok(v))) => response(StatusCode::OK, v),
        Ok(Ok(Err(r))) => *r,
        _ => refused(
            StatusCode::SERVICE_UNAVAILABLE,
            if mutation {
                "outcome_unknown"
            } else {
                "unavailable"
            },
        ),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_payable_fields_keys_nulls_and_query_alias_are_strict() {
        let register=br#"{"documentId":1,"billDate":"2026-01-01","dueDate":"2026-01-15","expenseAccount":"E","confirm":true,"idempotencyKey":" raw key "}"#;
        let Command::Register(i) = body("/register", register).unwrap() else {
            panic!()
        };
        assert_eq!(i.idempotency_key.as_deref(), Some(" raw key "));
        assert_eq!(i.native().expense_account_no, "E");
        let pay = json!({"payableId":1,"bankTransactionId":2,"date":"2026-01-03","amount":50,"paymentAccount":"B","confirm":true});
        let Command::Pay(i) = body("/pay", &serde_json::to_vec(&pay).unwrap()).unwrap() else {
            panic!()
        };
        assert_eq!(i.native().payment_account_no.as_deref(), Some("B"));
        assert!(i.idempotency_key.is_none());
        for field in ["amount", "date", "paymentAccount", "note", "idempotencyKey"] {
            let mut bad = pay.clone();
            bad[field] = Value::Null;
            assert!(body("/pay", &serde_json::to_vec(&bad).unwrap()).is_err());
        }
        let mut forged = pay.clone();
        forged["actor"] = json!("user:forged");
        assert!(body("/pay", &serde_json::to_vec(&forged).unwrap()).is_err());
        assert!(body("/register",br#"{"documentId":1,"documentId":2,"billDate":"2026-01-01","dueDate":"2026-01-15","expenseAccount":"E","confirm":true}"#).is_err());
        let q=query("asOf=2026-01-10&asOfDate=2025-01-01&status=overdue&supplier=Supplier&vendorId=1&from=2026-01-01&to=2026-02-01&minDays=2").unwrap();
        assert_eq!(q.as_of.as_deref(), Some("2026-01-10"));
        assert_eq!(q.min_days, Some(2));
        assert!(query("status=all&status=open").is_err());
        assert!(query("minDays=-1").is_err());
        assert!(query("unknown=1").is_err());
    }
}
