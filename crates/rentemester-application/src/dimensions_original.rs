//! Flat source operations parsed only behind current human/service authority.
use crate::{
    access::Permission,
    control_fence::{self, Committed, CookieAuthorization, Failure, SessionRequirement},
    documents::Context,
    service_company,
};
use rentemester_engine::Error;
use rentemester_runtime::{auth::Clock, namespace::NamespaceGuard};
use rentemester_storage::{
    dimensions as native, dimensions_original as budget, idempotency::PrincipalKind,
};
use rusqlite::{Connection, Transaction};
use serde::{
    Deserialize,
    de::{self, MapAccess, SeqAccess, Visitor},
};
use serde_json::{Value, json};
use std::fmt;
#[derive(Clone, Copy)]
pub enum Operation {
    Define,
    Member,
    DefinitionLifecycle,
    MemberLifecycle,
    Definitions,
    Members,
    Plan,
    Apply,
    Replace,
    Supersede,
    List,
    BudgetPlan,
    BudgetApply,
    Budgets,
}
impl Operation {
    pub fn write(self) -> bool {
        matches!(
            self,
            Self::Define
                | Self::Member
                | Self::DefinitionLifecycle
                | Self::MemberLifecycle
                | Self::Apply
                | Self::Replace
                | Self::Supersede
                | Self::BudgetApply
        )
    }
    fn permission(self) -> Permission {
        match self {
            Self::Define | Self::Member | Self::DefinitionLifecycle | Self::MemberLifecycle => {
                Permission::CompanyMasterData
            }
            Self::Apply => Permission::CompanyDraftWrite,
            Self::Replace | Self::Supersede | Self::BudgetApply => Permission::CompanyReview,
            _ => Permission::CompanyRead,
        }
    }
    pub fn post(self) -> bool {
        self.write() || matches!(self, Self::Plan | Self::BudgetPlan)
    }
}
pub const MAX_BODY: usize = 65_536;
pub struct Request<'a> {
    pub body: &'a [u8],
    pub query: Option<&'a str>,
    pub valid_transport: bool,
}
fn invalid() -> Error {
    Error::new("invalid_input", "Invalid original dimensions request")
}
fn business(e: Error) -> Error {
    Error::new(
        "dimensions_business",
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
fn fields(op: Operation) -> (&'static [&'static str], &'static [&'static str]) {
    use Operation::*;
    match op {
        Define => (&["dimensionId", "kind", "name", "confirm"], &[]),
        Member => (
            &["dimensionId", "memberId", "name", "confirm"],
            &["status", "effectiveFrom", "effectiveTo"],
        ),
        DefinitionLifecycle => (
            &["dimensionId", "action", "confirm"],
            &["name", "supersedesId"],
        ),
        MemberLifecycle => (
            &["dimensionId", "memberId", "action", "confirm"],
            &["name", "supersedesId"],
        ),
        Plan => (
            &["journalLineId", "allocations"],
            &["source", "reviewedImport", "sourceRef"],
        ),
        Apply => (
            &["journalLineId", "allocations", "planHash", "confirm"],
            &["source", "reviewedImport", "sourceRef", "idempotencyKey"],
        ),
        Replace => (
            &[
                "journalLineId",
                "allocations",
                "planHash",
                "confirm",
                "expectedAssignmentId",
                "reason",
            ],
            &["source", "reviewedImport", "sourceRef", "idempotencyKey"],
        ),
        Supersede => (&["assignmentId", "reason", "confirm"], &[]),
        BudgetPlan => (&["accountNo", "period", "allocations", "sourceRef"], &[]),
        BudgetApply => (
            &[
                "accountNo",
                "period",
                "allocations",
                "sourceRef",
                "planHash",
                "confirm",
            ],
            &["idempotencyKey"],
        ),
        _ => (&[], &[]),
    }
}
fn parse(r: &Request<'_>, op: Operation, actor: &str) -> Result<Value, Error> {
    let Strict(mut v) = serde_json::from_slice(r.body).map_err(|_| invalid())?;
    let o = v.as_object_mut().ok_or_else(invalid)?;
    if op.write() {
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
    }
    let (required, optional) = fields(op);
    if required.iter().any(|k| !o.contains_key(*k))
        || o.keys()
            .any(|k| !required.contains(&k.as_str()) && !optional.contains(&k.as_str()))
    {
        return Err(invalid());
    }
    Ok(v)
}
fn query(r: &Request<'_>, key: &str, required: bool) -> Result<Option<String>, Error> {
    let mut out = None;
    if let Some(q) = r.query {
        for pair in q.split('&') {
            let (k, v) = pair.split_once('=').ok_or_else(invalid)?;
            if decode(k)? != key || out.is_some() {
                return Err(invalid());
            }
            out = Some(decode(v)?);
        }
    }
    if required && out.is_none() {
        return Err(invalid());
    }
    Ok(out)
}
fn execute_tx(
    tx: &Transaction<'_>,
    _guard: &mut NamespaceGuard,
    subject: &str,
    _clock: &dyn Clock,
    r: Request<'_>,
    op: Operation,
    kind: PrincipalKind,
) -> Result<Value, Error> {
    if !r.valid_transport || r.body.len() > MAX_BODY || (op.post() && r.query.is_some()) {
        return Err(invalid());
    }
    let actor = format!(
        "{}:{subject}",
        match kind {
            PrincipalKind::User => "user",
            PrincipalKind::ServiceAccount => "service_account",
        }
    );
    use Operation::*;
    if !op.post() {
        if !r.body.is_empty() {
            return Err(invalid());
        }
        return match op {
            Definitions | Budgets => {
                if r.query.is_some() {
                    return Err(invalid());
                }
                if matches!(op, Definitions) {
                    native::definitions(tx)
                } else {
                    budget::budgets(tx)
                }
            }
            Members => native::members(tx, query(&r, "dimensionId", false)?.as_deref()),
            List => {
                let s = query(&r, "journalLineId", true)?.ok_or_else(invalid)?;
                if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
                    return Err(invalid());
                }
                native::assignments(tx, s.parse().map_err(|_| invalid())?)
            }
            _ => Err(invalid()),
        }
        .map_err(business);
    }
    let v = parse(&r, op, &actor)?;
    let result = match op {
        Define | Member | DefinitionLifecycle | MemberLifecycle => native::master(
            tx,
            match op {
                Define => "define",
                Member => "member",
                DefinitionLifecycle => "definition-lifecycle",
                _ => "member-lifecycle",
            },
            &serde_json::from_value(v).map_err(|_| invalid())?,
            &actor,
        ),
        Plan => native::plan(tx, &serde_json::from_value(v).map_err(|_| invalid())?),
        Apply | Replace => {
            let mut result = native::apply(
                tx,
                &serde_json::from_value(v).map_err(|_| invalid())?,
                matches!(op, Replace),
                &actor,
            )
            .map_err(business)?;
            if matches!(op, Apply) {
                result
                    .as_object_mut()
                    .ok_or_else(invalid)?
                    .remove("supersededId");
            }
            Ok(result)
        }
        Supersede => native::supersede(
            tx,
            &serde_json::from_value(v).map_err(|_| invalid())?,
            &actor,
        ),
        BudgetPlan => budget::budget_plan(tx, &serde_json::from_value(v).map_err(|_| invalid())?),
        BudgetApply => budget::budget_apply(
            tx,
            &serde_json::from_value(v).map_err(|_| invalid())?,
            &actor,
        ),
        _ => Err(invalid()),
    };
    result.map_err(business)
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
        op.permission(),
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
        op.permission(),
        op.write(),
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn flat_strict_identity_and_operation_boundaries() {
        for body in [
   br#"{"dimensionId":"p","kind":"project","name":"P","confirm":true,"confirm":true}"#.as_slice(),
   br#"{"dimensionId":"p","kind":"project","name":"P","confirm":true,"principal":"user:other"}"#,
   br#"{"dimensionId":"p","kind":"project","name":"P","confirm":true,"actor":null}"#,
   br#"{"dimensionId":"p","kind":"project","name":"P","confirm":true,"source":"reviewed"}"#,
  ] {assert!(parse(&Request{body,query:None,valid_transport:true},Operation::Define,"user:synthetic").is_err());}
        let body=br#"{"dimensionId":"p","kind":"project","name":"P","confirm":true,"actor":"service_account:synthetic","principal":"service_account:synthetic"}"#;
        assert!(
            parse(
                &Request {
                    body,
                    query: None,
                    valid_transport: true
                },
                Operation::Define,
                "service_account:synthetic"
            )
            .is_ok()
        );
        assert!(
            parse(
                &Request {
                    body,
                    query: None,
                    valid_transport: true
                },
                Operation::Define,
                "user:synthetic"
            )
            .is_err()
        );
        for q in ["dimensionId=a&dimensionId=b", "actor=x", "dimensionId=%GG"] {
            assert!(
                query(
                    &Request {
                        body: &[],
                        query: Some(q),
                        valid_transport: true
                    },
                    "dimensionId",
                    false
                )
                .is_err()
            );
        }
        assert!(
            serde_json::from_slice::<Strict>(br#"{"allocations":[{"amount":1,"amount":2}]}"#)
                .is_err()
        );
    }
}
