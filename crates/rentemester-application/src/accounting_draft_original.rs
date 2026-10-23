//! Flat source operations parsed only behind current human/service authority.
use crate::{
    access::Permission,
    control_fence::{self, Committed, CookieAuthorization, Failure, SessionRequirement},
    documents::Context,
    service_company,
};
use rentemester_engine::Error;
use rentemester_runtime::{auth::Clock, namespace::NamespaceGuard};
pub use rentemester_storage::accounting_draft_original::Operation;
use rentemester_storage::{accounting_draft_original as store, idempotency::PrincipalKind};
use rusqlite::{Connection, Transaction};
use serde::{
    Deserialize,
    de::{self, MapAccess, SeqAccess, Visitor},
};
use serde_json::{Value, json};
use std::fmt;
pub fn write(op: Operation) -> bool {
    !matches!(op, Operation::Show | Operation::List)
}
fn permission(op: Operation) -> Permission {
    match op {
        Operation::Create | Operation::Revise | Operation::Submit => Permission::CompanyDraftWrite,
        Operation::Reject | Operation::ApproveAndPost => Permission::CompanyReview,
        Operation::Show | Operation::List => Permission::CompanyRead,
    }
}
pub const MAX_BODY: usize = 270 * 1024;
pub const MAX_PAYLOAD: usize = 262_144;
pub struct Request<'a> {
    pub body: &'a [u8],
    pub query: Option<&'a str>,
    pub valid_transport: bool,
}
fn invalid() -> Error {
    Error::new("invalid_input", "Invalid original accounting draft request")
}
fn business(e: Error) -> Error {
    Error::new(
        "accounting_draft_original_business",
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
fn id(value: &str) -> bool {
    let value = value.trim();
    !value.is_empty()
        && value.len() <= 64
        && value.as_bytes()[0].is_ascii_lowercase()
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}
fn decode(s: &str) -> Result<String, Error> {
    let mut out = vec![];
    let mut bytes = s.bytes();
    while let Some(b) = bytes.next() {
        match b {
            b'+' => out.push(b' '),
            b'%' => {
                let a = bytes
                    .next()
                    .and_then(|c| (c as char).to_digit(16))
                    .ok_or_else(invalid)?;
                let b = bytes
                    .next()
                    .and_then(|c| (c as char).to_digit(16))
                    .ok_or_else(invalid)?;
                out.push((a * 16 + b) as u8);
            }
            _ => out.push(b),
        }
    }
    String::from_utf8(out).map_err(|_| invalid())
}
fn parse(r: &Request<'_>, op: Operation, actor: &str) -> Result<Value, Error> {
    if !r.valid_transport || r.body.len() > MAX_BODY {
        return Err(invalid());
    }
    if !write(op) {
        if !r.body.is_empty() {
            return Err(invalid());
        }
        return match op {
            Operation::List if r.query.is_none() => Ok(json!({})),
            Operation::Show => {
                let q = r.query.ok_or_else(invalid)?;
                if q.contains('&') {
                    return Err(invalid());
                }
                let (key, value) = q.split_once('=').ok_or_else(invalid)?;
                let value = decode(value)?;
                if decode(key)? != "draftId" || !id(&value) {
                    return Err(invalid());
                }
                Ok(json!({"draftId":value}))
            }
            _ => Err(invalid()),
        };
    }
    if r.query.is_some() {
        return Err(invalid());
    }
    let Strict(mut v) = serde_json::from_slice(r.body).map_err(|_| invalid())?;
    let o = v.as_object_mut().ok_or_else(invalid)?;
    for k in ["actor", "principal"] {
        if let Some(assertion) = o.remove(k)
            && assertion.as_str() != Some(actor)
        {
            return Err(Error::new(
                "access_denied",
                "Current principal assertion did not match",
            ));
        }
    }
    let allowed: &[&str] = match op {
        Operation::Create => &["draftId", "payload", "confirm"],
        Operation::Revise => &["draftId", "expectedEventHash", "payload", "confirm"],
        Operation::Reject => &["draftId", "expectedEventHash", "reason", "confirm"],
        _ => &["draftId", "expectedEventHash", "confirm"],
    };
    keys(&v, allowed)?;
    if allowed.iter().any(|k| v.get(*k).is_none())
        || v["confirm"] != true
        || !v["draftId"].as_str().is_some_and(id)
    {
        return Err(invalid());
    }
    if let Some(hash) = v.get("expectedEventHash")
        && !hash.as_str().is_some_and(|s| {
            s.len() == 64
                && s.bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        })
    {
        return Err(invalid());
    }
    if let Some(reason) = v.get("reason")
        && !reason
            .as_str()
            .is_some_and(|s| !s.trim().is_empty() && s.trim().len() <= 1000)
    {
        return Err(invalid());
    }
    if let Some(payload) = v.get("payload") {
        let p = payload.as_object().ok_or_else(invalid)?;
        if p.contains_key("createdBy")
            || p.contains_key("createdByProgram")
            || serde_json::to_vec(payload).map_err(|_| invalid())?.len() > MAX_PAYLOAD
        {
            return Err(invalid());
        }
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
    let now = clock
        .now_unix_ms()
        .map_err(|_| Error::new("clock_unavailable", "Current server time unavailable"))?;
    let latest = rentemester_runtime::posting_time::latest_date(now, 45)
        .map_err(|_| Error::new("clock_unavailable", "Posting horizon unavailable"))?;
    let latest = rentemester_engine::fiscal_year::Date::parse(&latest)?;
    let value = store::execute(tx, op, &v, &actor, latest).map_err(business)?;
    if value.get("ok").and_then(Value::as_bool) == Some(false) {
        return Err(Error::new(
            "accounting_draft_original_business",
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
