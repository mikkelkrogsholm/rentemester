//! Original bank operations over the same owner, current proof and native engine.
use super::*;
use rentemester_application::bank::Action as BankAction;
use rentemester_storage::{
    bank_matches::Input,
    bank_original::{Action, Correct, ExpectedPrincipal, Link},
    bank_reconciliation::correction::Apply,
};
enum Command {
    Suggestions(Input),
    Link(Link),
    Plan { bank: i64, replacement: i64 },
    Correct(Correct),
}
impl Command {
    fn action(&self) -> Action<'_> {
        match self {
            Self::Suggestions(i) => Action::Suggestions(i),
            Self::Link(i) => Action::Link(i),
            Self::Plan { bank, replacement } => Action::Plan {
                bank: *bank,
                replacement: *replacement,
            },
            Self::Correct(i) => Action::Correct(i),
        }
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Correction {
    confirm: bool,
    bank_transaction_id: i64,
    replacement_journal_entry_id: i64,
    expected_reconciliation_id: String,
    plan_hash: String,
    reason: String,
    idempotency_key: String,
    #[serde(default, deserialize_with = "principal")]
    expected_principal: Option<ExpectedPrincipal>,
}
fn principal<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Option<ExpectedPrincipal>, D::Error> {
    ExpectedPrincipal::deserialize(d).map(Some)
}
fn positive(raw: &str) -> Result<i64, ()> {
    if raw.is_empty() || !raw.bytes().all(|b| b.is_ascii_digit()) {
        return Err(());
    }
    let n = raw.parse().map_err(|_| ())?;
    if !(1..=9_007_199_254_740_991).contains(&n) {
        return Err(());
    }
    Ok(n)
}
fn query(path: &str, raw: &str) -> Result<Command, ()> {
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
    let mut fields = std::collections::BTreeMap::new();
    for (key, value) in url::form_urlencoded::parse(bytes) {
        if fields.insert(key.into_owned(), positive(&value)?).is_some() {
            return Err(());
        }
    }
    if path.ends_with("/suggestions") {
        let bank_transaction_id = fields.remove("bankTransactionId");
        let max = fields
            .remove("max")
            .map(usize::try_from)
            .transpose()
            .map_err(|_| ())?;
        if !fields.is_empty() {
            return Err(());
        }
        Ok(Command::Suggestions(Input {
            bank_transaction_id,
            max,
        }))
    } else {
        let bank = fields.remove("bankTransactionId").ok_or(())?;
        let replacement = fields.remove("replacementJournalEntryId").ok_or(())?;
        if !fields.is_empty() {
            return Err(());
        }
        Ok(Command::Plan { bank, replacement })
    }
}
fn body(path: &str, bytes: &[u8]) -> Result<Command, &'static str> {
    if path.ends_with("/link-journal") {
        let input: Link = serde_json::from_slice(bytes).map_err(|_| "invalid_input")?;
        if !input.confirm {
            return Err("confirmation_required");
        }
        input.validate().map_err(|_| "invalid_input")?;
        Ok(Command::Link(input))
    } else {
        let input: Correction = serde_json::from_slice(bytes).map_err(|_| "invalid_input")?;
        if !input.confirm {
            return Err("confirmation_required");
        }
        if input.bank_transaction_id <= 0
            || input.replacement_journal_entry_id <= 0
            || input.bank_transaction_id > 9_007_199_254_740_991
            || input.replacement_journal_entry_id > 9_007_199_254_740_991
            || input.expected_reconciliation_id.is_empty()
            || input.expected_reconciliation_id.len() > 128
            || input.plan_hash.len() != 64
            || !input
                .plan_hash
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            || input.reason.trim().is_empty()
            || input.reason.trim().chars().count() > 1000
            || input.idempotency_key.is_empty()
            || input.idempotency_key.len() > 200
        {
            return Err("invalid_input");
        }
        if let Some(expected) = &input.expected_principal {
            expected.validate().map_err(|_| "invalid_input")?;
        }
        Ok(Command::Correct(Correct {
            input: Apply {
                confirm: true,
                bank_transaction_id: input.bank_transaction_id,
                replacement_journal_entry_id: input.replacement_journal_entry_id,
                expected_reconciliation_id: input.expected_reconciliation_id,
                plan_hash: input.plan_hash,
                reason: input.reason,
                idempotency_key: input.idempotency_key,
            },
            expected_principal: input.expected_principal,
        }))
    }
}
pub(super) async fn handle(State(state): State<Product>, request: Request) -> Response {
    let mutation = request.method() == axum::http::Method::POST;
    let credential = match documents::credential(&state, &request, mutation) {
        Ok(value) => value,
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
            || !one(request.headers(), "content-type")
                .is_some_and(|v| v.eq_ignore_ascii_case("application/json"))
        {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        }
        let bytes = match to_bytes(request.into_body(), 16 * 1024).await {
            Ok(value) => value,
            Err(_) => return refused(StatusCode::PAYLOAD_TOO_LARGE, "body_too_large"),
        };
        match body(&path, &bytes) {
            Ok(value) => value,
            Err(code) => return refused(StatusCode::BAD_REQUEST, code),
        }
    } else {
        if request.method() != axum::http::Method::GET
            || request.headers().contains_key("content-encoding")
            || request.headers().contains_key("transfer-encoding")
            || request.headers().contains_key("idempotency-key")
            || one(request.headers(), "content-length").is_some_and(|v| v != "0")
        {
            return refused(StatusCode::BAD_REQUEST, "invalid_query");
        }
        let query = match query(&path, request.uri().query().unwrap_or("")) {
            Ok(value) => value,
            Err(()) => return refused(StatusCode::BAD_REQUEST, "invalid_query"),
        };
        if !matches!(to_bytes(request.into_body(),0).await,Ok(b) if b.is_empty()) {
            return refused(StatusCode::BAD_REQUEST, "body_not_allowed");
        }
        query
    };
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let mut owner = state.native.lock_company(&slug).map_err(|_| ())?;
        Ok::<_, ()>(match credential {
            documents::Credential::Human(cookie) => documents::outcome(
                owner
                    .bank(
                        &cookie,
                        &SystemClock,
                        BankAction::Original(command.action()),
                    )
                    .map_err(|_| ())?,
            ),
            documents::Credential::Service(key) => documents::service_outcome(
                owner
                    .bank_service(&key, &SystemClock, BankAction::Original(command.action()))
                    .map_err(|_| ())?,
            ),
        })
    })
    .await;
    match result {
        Ok(Ok(Ok(value))) => response(StatusCode::OK, value),
        Ok(Ok(Err(r))) => *r,
        _ => refused(StatusCode::SERVICE_UNAVAILABLE, "outcome_unconfirmed"),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_bank_wire_keeps_raw_intent_and_strict_principal_assertion() {
        let Command::Link(link)=body("/link-journal",br#"{"confirm":true,"bankTransactionId":1,"journalEntryId":2,"matchMethod":"source-reference","sourceReference":" raw "}"#).unwrap() else{panic!()};
        assert_eq!(link.source_reference.as_deref(), Some(" raw "));
        assert!(link.note.is_none());
        let raw = json!({"confirm":true,"bankTransactionId":1,"replacementJournalEntryId":3,"expectedReconciliationId":"append-only:1","planHash":"a".repeat(64),"reason":" raw reason ","idempotencyKey":" raw key ","expectedPrincipal":{"kind":"service-account","subjectId":"service_123"}});
        let Command::Correct(correct) =
            body("/correction-apply", &serde_json::to_vec(&raw).unwrap()).unwrap()
        else {
            panic!()
        };
        assert_eq!(correct.input.reason, " raw reason ");
        assert_eq!(correct.input.idempotency_key, " raw key ");
        assert_eq!(
            correct.expected_principal.unwrap().subject_id,
            "service_123"
        );
        for bad in [
            json!(null),
            json!("user:u"),
            json!({"kind":"agent","subjectId":"u"}),
            json!({"kind":"user","subjectId":" "}),
            json!({"kind":"user","subjectId":"u","actor":"forged"}),
        ] {
            let mut value = raw.clone();
            value["expectedPrincipal"] = bad;
            assert!(body("/correction-apply", &serde_json::to_vec(&value).unwrap()).is_err());
        }
        assert!(body("/link-journal",br#"{"confirm":true,"bankTransactionId":1,"journalEntryId":2,"matchMethod":"manual-review","note":null}"#).is_err());
        assert!(query("/suggestions", "max=1&max=2").is_err());
        assert!(query("/suggestions", "max=0").is_err());
        assert!(query("/correction-plan", "bankTransactionId=1").is_err());
    }
}
