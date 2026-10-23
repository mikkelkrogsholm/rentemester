//! Parse and resolve original operations only inside the held current-authority transaction.
use crate::{
    access::Permission,
    control_fence::{self, Committed, CookieAuthorization, Failure, SessionRequirement},
    documents::Context,
    service_company,
};
use rentemester_engine::Error;
use rentemester_runtime::{auth::Clock, namespace::NamespaceGuard};
use rentemester_storage::{
    idempotency::PrincipalKind, supplier_commitment_original as store,
    supplier_commitments as native,
};
use rusqlite::{Connection, Transaction};
use serde::Deserialize;
use serde_json::{Value, json};
#[derive(Clone, Copy)]
pub enum Operation {
    Plan,
    Apply,
    List,
    Change,
    Match,
    Matches,
    Alerts,
}
impl Operation {
    pub fn write(self) -> bool {
        matches!(self, Self::Apply | Self::Change | Self::Match)
    }
    fn permission(self) -> Permission {
        if self.write() {
            Permission::CompanyDraftWrite
        } else {
            Permission::CompanyRead
        }
    }
}
pub const MAX_BODY: usize = 65_536;
pub struct Request<'a> {
    pub body: &'a [u8],
    pub query: Option<&'a str>,
    pub valid_transport: bool,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Plan {
    commitment: store::Input,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Apply {
    commitment: store::Input,
    payload_hash: String,
    idempotency_key: Option<String>,
    confirm: bool,
    #[serde(default, deserialize_with = "actor_field")]
    actor: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Change {
    commitment_id: String,
    action: String,
    reason: String,
    confirm: bool,
    #[serde(default, deserialize_with = "actor_field")]
    actor: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Match {
    commitment_id: String,
    occurrence_date: String,
    evidence: Evidence,
    confirm: bool,
    #[serde(default, deserialize_with = "actor_field")]
    actor: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Evidence {
    kind: String,
    id: String,
}
fn actor_field<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    let s = String::deserialize(d)?;
    if s.is_empty() || s.len() > 160 || s.trim() != s || s.chars().any(char::is_control) {
        return Err(serde::de::Error::custom("invalid actor"));
    }
    Ok(Some(s))
}
fn invalid() -> Error {
    Error::new(
        "invalid_input",
        "Invalid original supplier commitment request",
    )
}
fn parse<T: serde::de::DeserializeOwned>(r: &Request<'_>) -> Result<T, Error> {
    serde_json::from_slice(r.body).map_err(|_| invalid())
}
fn business(e: Error) -> Error {
    Error::new(
        "supplier_commitment_business",
        json!({"ok":false,"errors":[e.message]}).to_string(),
    )
}
fn assertion(actor: Option<&str>, subject: &str, kind: PrincipalKind) -> Result<(), Error> {
    let expected = format!(
        "{}:{subject}",
        match kind {
            PrincipalKind::User => "user",
            PrincipalKind::ServiceAccount => "service_account",
        }
    );
    if actor.is_some_and(|a| a != expected) {
        return Err(Error::new(
            "access_denied",
            "Current actor assertion did not match",
        ));
    }
    Ok(())
}
fn original(tx: &Transaction<'_>, guard: &mut NamespaceGuard, id: i64) -> Result<(), Error> {
    let source = rentemester_storage::documents::evidence(tx, id)?;
    guard
        .read_document(&source.path, &source.hash)
        .map_err(|_| {
            Error::new(
                "commitment_original_unavailable",
                "original missing or integrity mismatch",
            )
        })?;
    Ok(())
}
fn sources(
    tx: &Transaction<'_>,
    g: &mut NamespaceGuard,
    input: &native::Input,
) -> Result<(), Error> {
    for id in native::source_documents(input)? {
        original(tx, g, id)?;
    }
    Ok(())
}
fn query(r: &Request<'_>, key: &str, required: bool) -> Result<Option<String>, Error> {
    let mut value = None;
    if let Some(q) = r.query {
        for pair in q.split('&') {
            let (k, v) = pair.split_once('=').ok_or_else(invalid)?;
            if decode(k)? != key || value.is_some() {
                return Err(invalid());
            }
            value = Some(decode(v)?);
        }
    }
    if required && value.is_none() {
        return Err(invalid());
    }
    Ok(value)
}
fn execute_tx(
    tx: &Transaction<'_>,
    guard: &mut NamespaceGuard,
    subject: &str,
    clock: &dyn Clock,
    r: Request<'_>,
    op: Operation,
    kind: PrincipalKind,
) -> Result<Value, Error> {
    if !r.valid_transport || r.body.len() > MAX_BODY {
        return Err(invalid());
    }
    if op.write() || matches!(op, Operation::Plan) {
        if r.query.is_some() {
            return Err(invalid());
        }
    }
    let now = rentemester_engine::company_knowledge::timestamp(
        clock.now_unix_ms().map_err(|_| invalid())?,
    )?;
    let mut value = match op {
        Operation::Plan => {
            let p: Plan = parse(&r)?;
            let input = p.commitment.native().map_err(business)?;
            sources(tx, guard, &input).map_err(business)?;
            json!({"ok":true,"proposal":p.commitment.proposal(tx).map_err(business)?})
        }
        Operation::Apply => {
            let p: Apply = parse(&r)?;
            assertion(p.actor.as_deref(), subject, kind)?;
            if !p.confirm {
                return Err(invalid());
            }
            let input = p.commitment.native().map_err(business)?;
            sources(tx, guard, &input).map_err(business)?;
            p.commitment.proposal(tx).map_err(business)?;
            native::apply_principal(
                tx,
                &native::Apply {
                    commitment: input,
                    payload_hash: p.payload_hash,
                    confirm: true,
                    idempotency_key: p.idempotency_key,
                },
                subject,
                kind,
                &now,
            )
            .map_err(business)?
        }
        Operation::Change => {
            let p: Change = parse(&r)?;
            assertion(p.actor.as_deref(), subject, kind)?;
            if !p.confirm {
                return Err(invalid());
            }
            let mut v = native::change_current_principal(
                tx,
                &p.commitment_id,
                &p.action,
                &p.reason,
                subject,
                kind,
                &now,
            )
            .map_err(business)?;
            v = json!({"payloadHash":v["payloadHash"],"idempotent":false});
            v
        }
        Operation::Match => {
            let p: Match = parse(&r)?;
            assertion(p.actor.as_deref(), subject, kind)?;
            if !p.confirm {
                return Err(invalid());
            }
            let input = native::Match {
                commitment_id: p.commitment_id,
                occurrence_date: p.occurrence_date,
                evidence: serde_json::from_value(
                    json!({"kind":p.evidence.kind,"id":p.evidence.id}),
                )
                .map_err(|_| invalid())?,
                confirm: true,
            };
            if let Some(id) = native::evidence_document(tx, &input).map_err(business)? {
                original(tx, guard, id).map_err(business)?;
            } else {
                let id = input.evidence.id.parse::<i64>().map_err(|_| invalid())?;
                let hash = rentemester_storage::bank_reconciliation::source_hash(tx, id)
                    .map_err(business)?;
                let bytes = guard.read_bank_statement(&hash).map_err(|_| {
                    business(Error::new(
                        "source_unavailable",
                        "bank original unavailable",
                    ))
                })?;
                rentemester_storage::bank_reconciliation::verify_source(tx, id, &bytes)
                    .map_err(business)?;
            }
            native::match_occurrence_principal(tx, &input, subject, kind, &now).map_err(business)?
        }
        Operation::List => {
            if r.query.is_some() {
                return Err(invalid());
            }
            json!({"ok":true,"rows":store::list(tx).map_err(business)?,"errors":[]})
        }
        Operation::Matches => {
            json!({"ok":true,"rows":store::matches(tx,query(&r,"commitmentId",false)?.as_deref()).map_err(business)?,"errors":[]})
        }
        Operation::Alerts => {
            json!({"ok":true,"rows":store::alerts(tx,&query(&r,"asOf",true)?.ok_or_else(invalid)?).map_err(business)?,"errors":[]})
        }
    };
    value["ok"] = json!(true);
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
    fn strict_parser_and_query_boundaries() {
        for body in [
            br#"{"commitmentId":"a","action":"paused","reason":"reason","confirm":true,"confirm":true}"#.as_slice(),
            br#"{"commitmentId":"a","action":"paused","reason":"reason","confirm":true,"principal":"forged"}"#,
            br#"{"commitmentId":"a","action":"paused","reason":"reason","confirm":true,"actor":null}"#,
        ] {
            assert!(parse::<Change>(&Request{body,query:None,valid_transport:true}).is_err());
        }
        for q in [
            "asOf=a&asOf=b",
            "asOf=a&actor=forged",
            "asOf=%GG",
            "unknown=a",
        ] {
            assert!(
                query(
                    &Request {
                        body: &[],
                        query: Some(q),
                        valid_transport: true
                    },
                    "asOf",
                    true
                )
                .is_err()
            );
        }
        assert!(
            query(
                &Request {
                    body: &[],
                    query: None,
                    valid_transport: true
                },
                "asOf",
                true
            )
            .is_err()
        );
        assert_eq!(
            query(
                &Request {
                    body: &[],
                    query: Some("commitmentId=synthetic%20id"),
                    valid_transport: true
                },
                "commitmentId",
                false
            )
            .unwrap()
            .as_deref(),
            Some("synthetic id")
        );
        assert!(assertion(Some("user:synthetic"), "synthetic", PrincipalKind::User).is_ok());
        assert!(
            assertion(
                Some("user:synthetic"),
                "synthetic",
                PrincipalKind::ServiceAccount
            )
            .is_err()
        );
    }
}
