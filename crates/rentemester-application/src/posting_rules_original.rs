//! Flat source operations parsed only behind current human/service authority.
use crate::{
    access::Permission,
    control_fence::{self, Committed, CookieAuthorization, Failure, SessionRequirement},
    documents::Context,
    service_company,
};
use rentemester_engine::Error;
use rentemester_runtime::{auth::Clock, namespace::NamespaceGuard};
pub use rentemester_storage::posting_rules_original::Operation;
use rentemester_storage::{idempotency::PrincipalKind, posting_rules_original as store};
use rusqlite::{Connection, Transaction};
use serde::{
    Deserialize,
    de::{self, MapAccess, SeqAccess, Visitor},
};
use serde_json::{Value, json};
use std::fmt;
pub fn write(op: Operation) -> bool {
    !matches!(op, Operation::Explain | Operation::Test)
}
fn permission(op: Operation) -> Permission {
    if write(op) {
        Permission::CompanyReview
    } else {
        Permission::CompanyRead
    }
}
pub const MAX_BODY: usize = 65_536;
pub struct Request<'a> {
    pub body: &'a [u8],
    pub query: Option<&'a str>,
    pub valid_transport: bool,
}
fn invalid() -> Error {
    Error::new("invalid_input", "Invalid original posting rules request")
}
fn business(e: Error) -> Error {
    Error::new(
        "posting_rules_original_business",
        json!({"ok":false,"errors":[e.message]}).to_string(),
    )
}
// A Value visitor retaining duplicate-key rejection at every nesting level.
struct Strict(Value);
impl<'de> Deserialize<'de> for Strict {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Strict;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("unique-key JSON")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut a: A) -> Result<Strict, A::Error> {
                let mut o = serde_json::Map::new();
                while let Some(k) = a.next_key::<String>()? {
                    if o.contains_key(&k) {
                        return Err(de::Error::custom("duplicate key"));
                    }
                    o.insert(k, a.next_value::<Strict>()?.0);
                }
                Ok(Strict(Value::Object(o)))
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut a: A) -> Result<Strict, A::Error> {
                let mut v = vec![];
                while let Some(x) = a.next_element::<Strict>()? {
                    v.push(x.0);
                }
                Ok(Strict(Value::Array(v)))
            }
            fn visit_bool<E: de::Error>(self, v: bool) -> Result<Strict, E> {
                Ok(Strict(json!(v)))
            }
            fn visit_i64<E: de::Error>(self, v: i64) -> Result<Strict, E> {
                Ok(Strict(json!(v)))
            }
            fn visit_u64<E: de::Error>(self, v: u64) -> Result<Strict, E> {
                Ok(Strict(json!(v)))
            }
            fn visit_f64<E: de::Error>(self, v: f64) -> Result<Strict, E> {
                Ok(Strict(json!(v)))
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Strict, E> {
                Ok(Strict(json!(v)))
            }
            fn visit_unit<E: de::Error>(self) -> Result<Strict, E> {
                Ok(Strict(Value::Null))
            }
        }
        d.deserialize_any(V)
    }
}
fn keys(v: &Value, allowed: &[&str]) -> Result<(), Error> {
    let o = v.as_object().ok_or_else(invalid)?;
    if o.keys().any(|k| !allowed.contains(&k.as_str())) {
        return Err(invalid());
    }
    Ok(())
}
fn parse(r: &Request<'_>, op: Operation, actor: &str) -> Result<Value, Error> {
    if !r.valid_transport || r.body.len() > MAX_BODY || r.query.is_some() {
        return Err(invalid());
    }
    let Strict(mut v) = serde_json::from_slice(r.body).map_err(|_| invalid())?;
    let o = v.as_object_mut().ok_or_else(invalid)?;
    if write(op) {
        for k in ["actor", "principal"] {
            if let Some(assertion) = o.remove(k) {
                if assertion.as_str() != Some(actor) {
                    return Err(Error::new(
                        "access_denied",
                        "Current principal assertion did not match",
                    ));
                }
            }
        }
        if o.get("confirm") != Some(&Value::Bool(true)) {
            return Err(invalid());
        }
    }
    match op {
        Operation::Propose => {
            keys(&v, &["proposal", "confirm"])?;
            let p = &v["proposal"];
            keys(
                p,
                &[
                    "ruleId",
                    "effectiveFrom",
                    "effectiveTo",
                    "conditions",
                    "outcome",
                    "provenance",
                    "rationale",
                ],
            )?;
            keys(
                &p["conditions"],
                &[
                    "supplierIdentity",
                    "documentType",
                    "supplierCountry",
                    "supplierVat",
                    "currency",
                    "vat",
                    "amount",
                    "reverseChargeWording",
                    "attributes",
                ],
            )?;
            if let Some(a) = p["conditions"].get("amount") {
                keys(a, &["min", "max"])?;
            }
            keys(
                &p["outcome"],
                &["account", "vatTreatment", "textTemplate", "dimensions"],
            )?;
        }
        Operation::Explain | Operation::Test => {
            keys(&v, &["context", "at"])?;
            keys(
                &v["context"],
                &[
                    "supplierIdentity",
                    "documentType",
                    "supplierCountry",
                    "supplierVat",
                    "currency",
                    "vatAmount",
                    "amount",
                    "reverseChargeWording",
                    "attributes",
                    "documentId",
                    "changedBuyer",
                    "changedSupplier",
                    "changedVat",
                    "changedCurrency",
                    "unusualAmount",
                    "contradictoryEvidence",
                ],
            )?;
        }
        _ => keys(
            &v,
            &[
                "ruleId",
                "version",
                "expectedPayloadHash",
                "rationale",
                "provenance",
                "effectiveAt",
                "confirm",
            ],
        )?,
    }
    Ok(v)
}
fn execute_tx(
    tx: &Transaction<'_>,
    _guard: &mut NamespaceGuard,
    subject: &str,
    clock: &dyn Clock,
    r: Request<'_>,
    op: Operation,
    kind: PrincipalKind,
) -> Result<Value, Error> {
    let actor = format!(
        "{}:{subject}",
        match kind {
            PrincipalKind::User => "user",
            PrincipalKind::ServiceAccount => "service_account",
        }
    );
    let v = parse(&r, op, &actor)?;
    let now = rentemester_engine::company_knowledge::timestamp(
        clock
            .now_unix_ms()
            .map_err(|_| Error::new("clock_unavailable", "Current server time unavailable"))?,
    )?;
    let value = store::execute(tx, op, &v, &actor, &now).map_err(business)?;
    if value.get("ok").and_then(Value::as_bool) == Some(false) {
        return Err(Error::new(
            "posting_rules_original_business",
            value.to_string(),
        ));
    }
    Ok(value)
}
pub fn execute(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: Context<'_>,
    r: Request<'_>,
    op: Operation,
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
        permission(op),
        |tx, subject| {
            execute_tx(
                tx,
                guard,
                subject,
                context.clock,
                r,
                op,
                PrincipalKind::User,
            )
        },
    )
}
pub fn execute_service(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: service_company::Context<'_>,
    r: Request<'_>,
    op: Operation,
) -> Result<service_company::ServiceResult<Value>, service_company::ServiceFailure> {
    let clock = context.clock;
    service_company::execute_operation(
        control,
        company,
        context,
        permission(op),
        write(op),
        |tx, subject, _| {
            Ok(service_company::OperationResult {
                result: execute_tx(
                    tx,
                    guard,
                    subject,
                    clock,
                    r,
                    op,
                    PrincipalKind::ServiceAccount,
                )?,
                receipt: None,
            })
        },
    )
}

#[cfg(test)]
mod tests;
