//! Additive source adapter: typed current authority before and after provider I/O.
use crate::{
    access::Permission,
    control_fence::{self, Committed, CookieAuthorization, Failure, SessionRequirement},
    documents::Context,
    service_company::{self, ServiceFailure, ServiceResult},
};
use rentemester_engine::Error;
use rentemester_storage::{
    idempotency::PrincipalKind,
    master_data_original::{self as store, Create, Kind},
};
use rusqlite::{Connection, Transaction};
use serde_json::{Value, json};
pub enum Request<'a> {
    List(Kind, bool),
    Create(Kind, &'a Create),
    PrepareCvr(&'a str, Option<(Kind, &'a Create)>),
    PrepareVat(&'a str),
    Finish(Admission, &'a Value),
}
pub enum Output {
    Value(Value),
    Admission(Admission),
}
pub struct Admission {
    subject: String,
    ledger: String,
    workspace: String,
    slug: String,
    pending: Pending,
    permit: Option<service_company::precharged::Permit>,
}
enum Pending {
    Cvr(
        Box<rentemester_storage::cvr::Prepared>,
        Option<(Kind, Value)>,
    ),
    Vat(String),
}
impl Admission {
    pub fn provider_identity(&self) -> Option<&str> {
        match &self.pending {
            Pending::Cvr(p, _) => p.needs_lookup.then_some(p.cvr.as_deref()).flatten(),
            Pending::Vat(v) => Some(v),
        }
    }
    pub fn is_vat(&self) -> bool {
        matches!(self.pending, Pending::Vat(_))
    }
}
fn error() -> Error {
    Error::new(
        "master_data_original_admission_changed",
        "Current provider subject, company or ledger differs",
    )
}
fn ledger(tx: &Transaction<'_>) -> Result<String, Error> {
    tx.query_row(
        "SELECT ledger_uuid FROM ledger_identity WHERE id=1",
        [],
        |r| r.get(0),
    )
    .map_err(|_| error())
}
fn permission(request: &Request<'_>) -> Permission {
    match request {
        Request::List(..) => Permission::CompanyRead,
        Request::Create(..) => Permission::CompanyMasterData,
        _ => Permission::CompanyExternalLookup,
    }
}
fn apply(
    tx: &Transaction<'_>,
    request: Request<'_>,
    subject: &str,
    principal: PrincipalKind,
    workspace: &str,
    slug: &str,
    now: &str,
) -> Result<Output, Error> {
    Ok(match request {
        Request::List(kind, archived) => Output::Value(store::list(tx, kind, archived)?),
        Request::Create(kind, input) => {
            input.validate(kind)?;
            Output::Value(store::create(tx, kind, &input.input, subject, principal)?)
        }
        Request::PrepareCvr(cvr, create) => {
            let create = create
                .map(|(k, i)| {
                    i.validate(k)?;
                    Ok((k, i.input.clone()))
                })
                .transpose()?;
            Output::Admission(Admission {
                subject: subject.into(),
                ledger: ledger(tx)?,
                workspace: workspace.into(),
                slug: slug.into(),
                pending: Pending::Cvr(
                    Box::new(rentemester_storage::cvr::prepare(
                        tx,
                        Some(cvr),
                        false,
                        now,
                    )?),
                    create,
                ),
                permit: None,
            })
        }
        Request::PrepareVat(vat) => {
            let compact: String = vat.chars().filter(char::is_ascii_alphanumeric).collect();
            let vat = rentemester_engine::vies::normalize(&compact)?.normalized;
            Output::Admission(Admission {
                subject: subject.into(),
                ledger: ledger(tx)?,
                workspace: workspace.into(),
                slug: slug.into(),
                pending: Pending::Vat(vat),
                permit: None,
            })
        }
        Request::Finish(admission, observation) => {
            if admission.subject != subject
                || admission.ledger != ledger(tx)?
                || admission.workspace != workspace
                || admission.slug != slug
            {
                return Err(error());
            }
            let value = match admission.pending {
                Pending::Cvr(prepared, create) => {
                    let outcome = if prepared.needs_lookup {
                        Some(
                            if let Some(message) = observation["providerError"].as_str() {
                                rentemester_storage::contact_import::LookupOutcome::Failed(
                                    message.into(),
                                )
                            } else {
                                rentemester_storage::contact_import::LookupOutcome::Found(
                                    observation.clone(),
                                )
                            },
                        )
                    } else {
                        None
                    };
                    let result = rentemester_storage::cvr::finish(
                        tx,
                        &prepared,
                        outcome.as_ref(),
                        subject,
                        now,
                    )?;
                    if let Some((kind, input)) = create {
                        if result["ok"] != true {
                            json!({"ok":false,"errors":result["errors"]})
                        } else {
                            store::create(
                                tx,
                                kind,
                                &store::compose(&input, Some(&result["company"]))?,
                                subject,
                                principal,
                            )?
                        }
                    } else {
                        result
                    }
                }
                Pending::Vat(vat) => {
                    if observation["normalized"].as_str() != Some(&vat) {
                        return Err(error());
                    }
                    let record = rentemester_engine::vies::record(observation, now)?;
                    let actor = format!(
                        "{}:{subject}",
                        match principal {
                            PrincipalKind::User => "user",
                            PrincipalKind::ServiceAccount => "service_account",
                        }
                    );
                    rentemester_storage::vies::store(tx, &record, &actor)?;
                    json!({"ok":true,"validation":record,"appliedRules":["DK-VAT-REVERSE-CHARGE-001"],"errors":[]})
                }
            };
            Output::Value(value)
        }
    })
}
fn stamp(clock: &dyn rentemester_runtime::auth::Clock) -> Result<String, Error> {
    rentemester_engine::company_knowledge::timestamp(clock.now_unix_ms().map_err(|_| error())?)
}
// ExternalLookup and MasterData currently have exactly the same owner/bookkeeper
// role policy. Both are explicitly checked for composed creates at both fences.
fn composed(request: &Request<'_>) -> bool {
    matches!(
        request,
        Request::PrepareCvr(_, Some(_))
            | Request::Finish(
                Admission {
                    pending: Pending::Cvr(_, Some(_)),
                    ..
                },
                _
            )
    )
}
pub fn execute(
    control: &mut Connection,
    company: &mut Connection,
    context: Context<'_>,
    request: Request<'_>,
) -> Result<Committed<Output>, Failure> {
    let permission = permission(&request);
    let requirement = if matches!(
        &request,
        Request::PrepareVat(_)
            | Request::Finish(
                Admission {
                    pending: Pending::Vat(_),
                    ..
                },
                _
            )
    ) {
        SessionRequirement::Fresh
    } else {
        SessionRequirement::Current
    };
    let workspace = context.selection.workspace;
    let slug = context.selection.slug;
    let now = stamp(context.clock).map_err(|error| Failure {
        stage: control_fence::FailureStage::NotCommitted,
        error,
        control_reusable: true,
        company_reusable: true,
    })?;
    if composed(&request) {
        control_fence::execute_cookie_subject(
            control,
            company,
            CookieAuthorization {
                cookie: context.cookie,
                clock: context.clock,
                requirement: SessionRequirement::Current,
            },
            control_fence::CompanySelection {
                workspace,
                slug,
                registered: context.selection.registered,
                archived: context.selection.archived,
            },
            Permission::CompanyMasterData,
            |_, _| Ok(()),
        )?;
    }
    control_fence::execute_cookie_subject(
        control,
        company,
        CookieAuthorization {
            cookie: context.cookie,
            clock: context.clock,
            requirement,
        },
        context.selection,
        permission,
        |tx, subject| {
            apply(
                tx,
                request,
                subject,
                PrincipalKind::User,
                workspace,
                slug,
                &now,
            )
        },
    )
}
pub fn execute_service(
    control: &mut Connection,
    company: &mut Connection,
    context: service_company::Context<'_>,
    mut request: Request<'_>,
) -> Result<ServiceResult<Output>, ServiceFailure> {
    let permission = permission(&request);
    let workspace = context.selection.workspace.to_owned();
    let slug = context.selection.slug.to_owned();
    let now = stamp(context.clock).map_err(service_company::initial)?;
    // The shared policy grants both permissions to precisely owner/bookkeeper;
    // no weaker permission or caller-inferred service subject is used here.
    let permit = match &mut request {
        Request::Finish(a, _) => a.permit.take(),
        _ => None,
    };
    if let Some(permit) = permit {
        return service_company::precharged::finish(
            control,
            company,
            context,
            permit,
            |tx, subject| {
                apply(
                    tx,
                    request,
                    subject,
                    PrincipalKind::ServiceAccount,
                    &workspace,
                    &slug,
                    &now,
                )
            },
        );
    }
    if matches!(request, Request::PrepareCvr(..) | Request::PrepareVat(_)) {
        let done = service_company::precharged::prepare(
            control,
            company,
            context,
            permission,
            |tx, subject| {
                apply(
                    tx,
                    request,
                    subject,
                    PrincipalKind::ServiceAccount,
                    &workspace,
                    &slug,
                    &now,
                )
            },
        )?;
        let (mut output, permit) = done.result;
        if let Output::Admission(a) = &mut output {
            a.permit = Some(permit);
        }
        Ok(ServiceResult {
            result: output,
            usage: done.usage,
            business: done.business,
            receipt: done.receipt,
            control_reusable: done.control_reusable,
        })
    } else {
        service_company::execute_operation(
            control,
            company,
            context,
            permission,
            !matches!(request, Request::List(..)),
            |tx, subject, _| {
                Ok(service_company::OperationResult {
                    result: apply(
                        tx,
                        request,
                        subject,
                        PrincipalKind::ServiceAccount,
                        &workspace,
                        &slug,
                        &now,
                    )?,
                    receipt: None,
                })
            },
        )
    }
}
#[cfg(test)]
mod tests;
