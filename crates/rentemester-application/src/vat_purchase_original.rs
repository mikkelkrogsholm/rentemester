//! Original VAT writes: parse, selector, protected evidence and post share one current admission.
use crate::{
    access::Permission,
    control_fence::{self, Committed, CookieAuthorization, Failure, SessionRequirement},
    documents::Context,
    service_company,
};
use rentemester_engine::{Error, fiscal_year::Date};
use rentemester_runtime::{auth::Clock, namespace::NamespaceGuard};
use rentemester_storage::{idempotency::PrincipalKind, vat_purchase_original as store};
use rusqlite::{Connection, Transaction};
use serde::Deserialize;
use serde_json::{Value, json};
pub use store::Operation;

pub const MAX_BODY: usize = 16 * 1024 * 1024;
/// Transport defects are carried into the authorized callback, never parsed before authority.
pub struct Request<'a> {
    pub body: &'a [u8],
    pub valid_transport: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Wire {
    payload: store::Input,
    confirm: bool,
    #[serde(default, deserialize_with = "actor_field")]
    actor: Option<String>,
}
fn actor_field<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    let value = String::deserialize(d)?;
    if value.is_empty()
        || value.len() > 160
        || value.trim() != value
        || value.chars().any(char::is_control)
    {
        return Err(serde::de::Error::custom("invalid actor assertion"));
    }
    Ok(Some(value))
}
fn invalid() -> Error {
    Error::new("invalid_input", "Invalid original VAT purchase request")
}
fn unavailable() -> Error {
    Error::new(
        "document_evidence_unavailable",
        "Retained purchase source unavailable or changed",
    )
}
fn business(value: Value) -> Error {
    Error::new("vat_purchase_business", value.to_string())
}
fn refusal(error: Error) -> Error {
    business(json!({"ok":false,"appliedRules":[],"errors":[error.message]}))
}
fn parse(request: Request<'_>, operation: Operation) -> Result<Wire, Error> {
    if !request.valid_transport || request.body.len() > MAX_BODY {
        return Err(invalid());
    }
    let wire: Wire = serde_json::from_slice(request.body).map_err(|_| invalid())?;
    if !wire.confirm {
        return Err(invalid());
    }
    // Input is strict; these additional constraints distinguish the two fixed contracts.
    let raw: Value = serde_json::from_slice(request.body).map_err(|_| invalid())?;
    let payload = raw["payload"].as_object().ok_or_else(invalid)?;
    for key in ["documentId", "sourceBankTransactionId"] {
        if let Some(value) = payload.get(key) {
            if !value
                .as_i64()
                .is_some_and(|n| (1..=9_007_199_254_740_991).contains(&n))
            {
                return Err(invalid());
            }
        }
    }
    for key in [
        "transactionDate",
        "text",
        "expenseAccountNo",
        "paymentAccountNo",
        "invoiceNo",
        "currency",
    ] {
        if payload.get(key).is_some_and(|v| !v.is_string()) {
            return Err(invalid());
        }
    }
    for key in ["netAmount", "amountForeign", "amountDkk", "fxRateToDkk"] {
        if payload.get(key).is_some_and(|v| !v.is_number()) {
            return Err(invalid());
        }
    }
    match operation {
        Operation::EuService if !payload.contains_key("expenseAccountNo") => return Err(invalid()),
        Operation::Representation
            if payload.contains_key("invoiceNo") || !payload.contains_key("documentId") =>
        {
            return Err(invalid());
        }
        _ => {}
    }
    Ok(wire)
}
fn verify(tx: &Transaction<'_>, guard: &mut NamespaceGuard, id: i64) -> Result<(), Error> {
    rentemester_storage::documents_original::validate_history(tx)?;
    rentemester_storage::document_party_links::validate_backup(tx)?;
    rentemester_storage::invoice_extraction::validate_history(tx)?;
    let original = rentemester_storage::documents::evidence(tx, id)?;
    let bytes = guard
        .read_document(&original.path, &original.hash)
        .map_err(|_| unavailable())?;
    for retained in rentemester_storage::invoice_extraction::sources(tx)? {
        if retained.document_id == id
            && (retained.sha256 != original.hash || retained.pdf_bytes != bytes)
        {
            return Err(unavailable());
        }
    }
    Ok(())
}
#[allow(clippy::too_many_arguments)]
fn post_tx(
    tx: &Transaction<'_>,
    guard: &mut NamespaceGuard,
    subject: &str,
    clock: &dyn Clock,
    request: Request<'_>,
    operation: Operation,
    service: bool,
) -> Result<Value, Error> {
    let wire = parse(request, operation)?;
    let actor = format!(
        "{}:{subject}",
        if service { "service_account" } else { "user" }
    );
    if wire.actor.as_deref().is_some_and(|a| a != actor) {
        return Err(Error::new(
            "access_denied",
            "Current actor assertion did not match",
        ));
    }
    let id = store::resolve_document(tx, &wire.payload, operation).map_err(refusal)?;
    verify(tx, guard, id)?;
    if let Some(bank) = wire.payload.source_bank_transaction_id {
        rentemester_storage::bank_original::verify_journals(tx)?;
        let hash = rentemester_storage::bank_reconciliation::source_hash(tx, bank)?;
        let bytes = guard
            .read_bank_statement(&hash)
            .map_err(|_| unavailable())?;
        rentemester_storage::bank_reconciliation::verify_source(tx, bank, &bytes)?;
    }
    let now = clock.now_unix_ms().map_err(|_| unavailable())?;
    let stamp = rentemester_engine::company_knowledge::timestamp(now)?;
    let latest =
        rentemester_runtime::posting_time::latest_date(now, 45).map_err(|_| unavailable())?;
    let value = store::post(
        tx,
        &wire.payload,
        operation,
        subject,
        Date::parse(&latest)?,
        if service {
            PrincipalKind::ServiceAccount
        } else {
            PrincipalKind::User
        },
        &stamp,
    )
    .map_err(refusal)?;
    // A definite business false always rolls back, including any intermediate native effects.
    if value["ok"] != true {
        return Err(business(value));
    }
    Ok(value)
}
pub fn post(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: Context<'_>,
    request: Request<'_>,
    operation: Operation,
) -> Result<Committed<Value>, Failure> {
    control_fence::execute_cookie_subject(
        control,
        company,
        CookieAuthorization {
            cookie: context.cookie,
            clock: context.clock,
            requirement: SessionRequirement::Current,
        },
        context.selection,
        Permission::CompanyLedgerPost,
        |tx, subject| post_tx(tx, guard, subject, context.clock, request, operation, false),
    )
}
pub fn post_service(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: service_company::Context<'_>,
    request: Request<'_>,
    operation: Operation,
) -> Result<service_company::ServiceResult<Value>, service_company::ServiceFailure> {
    let clock = context.clock;
    service_company::execute_operation(
        control,
        company,
        context,
        Permission::CompanyLedgerPost,
        true,
        |tx, subject, _| {
            Ok(service_company::OperationResult {
                result: post_tx(tx, guard, subject, clock, request, operation, true)?,
                receipt: None,
            })
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn accepts(value: Value, operation: Operation) -> bool {
        let bytes = serde_json::to_vec(&value).unwrap();
        parse(
            Request {
                body: &bytes,
                valid_transport: true,
            },
            operation,
        )
        .is_ok()
    }
    fn eu() -> Value {
        json!({"confirm":true,"payload":{"transactionDate":"2026-01-02","text":"Synthetic service","netAmount":100,"expenseAccountNo":"synthetic-expense","documentId":1}})
    }
    #[test]
    fn strict_wire_and_operation_boundaries() {
        assert!(accepts(eu(), Operation::EuService));
        let mut representation = eu();
        representation["payload"]
            .as_object_mut()
            .unwrap()
            .remove("expenseAccountNo");
        assert!(accepts(representation.clone(), Operation::Representation));
        assert!(!accepts(representation.clone(), Operation::EuService));
        representation["payload"]["invoiceNo"] = json!("synthetic");
        assert!(!accepts(representation, Operation::Representation));
        for (key, value) in [
            ("actor", Value::Null),
            ("confirm", json!(false)),
            ("program", json!("forged")),
        ] {
            let mut input = eu();
            input[key] = value;
            assert!(!accepts(input, Operation::EuService), "{key}");
        }
        for (key, value) in [
            ("documentId", json!(0)),
            ("documentId", json!(9_007_199_254_740_992_i64)),
            ("sourceBankTransactionId", json!(1.5)),
            ("paymentAccountNo", Value::Null),
            ("createdBy", json!("user:forged")),
            ("purchaseVatLines", json!([])),
        ] {
            let mut input = eu();
            input["payload"][key] = value;
            assert!(!accepts(input, Operation::EuService), "{key}");
        }
    }
    #[test]
    fn semantic_invalid_values_reach_domain_but_duplicates_do_not() {
        let mut input = eu();
        input["payload"]["transactionDate"] = json!("not a date");
        input["payload"]["text"] = json!("");
        input["payload"]["netAmount"] = json!(-1);
        assert!(accepts(input, Operation::EuService));
        for body in [
            br#"{"confirm":true,"confirm":true,"payload":{}}"#.as_slice(),
            br#"{"confirm":true,"payload":{},"payload":{}}"#.as_slice(),
        ] {
            assert!(
                parse(
                    Request {
                        body,
                        valid_transport: true
                    },
                    Operation::EuService
                )
                .is_err()
            );
        }
        let bytes = serde_json::to_vec(&eu()).unwrap();
        assert!(
            parse(
                Request {
                    body: &bytes,
                    valid_transport: false
                },
                Operation::EuService
            )
            .is_err()
        );
    }
}
