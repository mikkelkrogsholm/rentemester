//! Fixed invoice commands under current role and owner fences, with immutable issue-time evidence.
use crate::{
    access::Permission,
    control_fence::{self, Committed, CookieAuthorization, Failure, SessionRequirement},
    documents::Context,
};
use rentemester_engine::{Error, fiscal_year::Date, sales_invoice::Input};
use rentemester_runtime::{document::Prepared, namespace::NamespaceGuard};
use rentemester_storage::sales::{self, Artifact, Customer, Post};
use rusqlite::Connection;
use serde_json::Value;
pub enum Action<'a> {
    OriginalClaims(
        rentemester_storage::invoice_claims_original::Kind,
        &'a Value,
    ),
    Lifecycle(rentemester_storage::invoice_lifecycle::Kind, &'a Value),
    Original(
        rentemester_storage::invoice_original::Kind,
        &'a rentemester_storage::invoice_original::Query,
    ),
    Contacts,
    DeleteCustomer(i64, bool),
    DeleteVendor(i64, bool),
    Customers(&'a Customer),
    Vendor(&'a rentemester_storage::vendors::Vendor),
    UpdateVendor(i64, &'a Value),
    UpdateCustomer(i64, &'a Value),
    List,
    Issue(&'a Input),
    Preview(&'a Input),
    Post(&'a Post),
    Settle(&'a rentemester_storage::invoice_settlement::Input),
    Credit(&'a rentemester_storage::credit_note::Input),
    Refund(&'a rentemester_storage::invoice_settlement::RefundInput),
    Pdf(i64),
}
pub enum Output {
    Json(Value),
    Pdf(Vec<u8>),
}
fn evidence_error() -> Error {
    Error::new(
        "invoice_evidence_unavailable",
        "immutable invoice evidence unavailable or integrity mismatch",
    )
}
fn original(
    tx: &rusqlite::Transaction<'_>,
    guard: &mut NamespaceGuard,
    id: i64,
) -> Result<Vec<u8>, Error> {
    let e = rentemester_storage::documents::evidence(tx, id)?;
    guard
        .read_document(&e.path, &e.hash)
        .map_err(|_| evidence_error())
}
pub(crate) fn verify_invoice(
    tx: &rusqlite::Transaction<'_>,
    guard: &mut NamespaceGuard,
    id: i64,
) -> Result<(), Error> {
    let bytes = original(tx, guard, id)?;
    let payload: String = tx
        .query_row(
            "SELECT payload_json FROM documents WHERE id=?1",
            [id],
            |r| r.get(0),
        )
        .map_err(|_| evidence_error())?;
    if bytes != payload.as_bytes() {
        return Err(evidence_error());
    }
    original(tx, guard, sales::pdf_id(tx, id)?)?;
    let mut q=tx.prepare("SELECT c.id,c.payload_json FROM credit_note_postings p JOIN documents c ON c.id=p.credit_note_document_id WHERE p.original_invoice_document_id=?1").map_err(|_|evidence_error())?;
    let rows = q
        .query_map([id], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))
        .map_err(|_| evidence_error())?;
    for row in rows {
        let (id, payload) = row.map_err(|_| evidence_error())?;
        if original(tx, guard, id)? != payload.as_bytes() {
            return Err(evidence_error());
        }
    }
    Ok(())
}
pub struct Request<'a> {
    pub action: Action<'a>,
    pub today: &'a str,
    pub now: &'a str,
    pub year: Option<&'a str>,
}
pub fn execute(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: Context<'_>,
    request: Request<'_>,
) -> Result<Committed<Output>, Failure> {
    let Request {
        action,
        today,
        now,
        year,
    } = request;
    let permission = match action {
        Action::OriginalClaims(kind, _) => crate::invoice_claims_original::permission(kind),
        Action::Lifecycle(kind, _) => crate::invoice_lifecycle::permission(kind),
        Action::Contacts | Action::List | Action::Original(..) => Permission::CompanyRead,
        Action::DeleteCustomer(_, _)
        | Action::DeleteVendor(_, _)
        | Action::Customers(_)
        | Action::UpdateCustomer(_, _)
        | Action::Vendor(_)
        | Action::UpdateVendor(_, _) => Permission::CompanyMasterData,
        Action::Issue(_) | Action::Preview(_) => Permission::CompanyDraftWrite,
        Action::Post(_) | Action::Settle(_) | Action::Credit(_) | Action::Refund(_) => {
            Permission::CompanyLedgerPost
        }
        Action::Pdf(_) => Permission::CompanyDocumentsRead,
    };
    let slug = context.selection.slug;
    let clock = context.clock;
    control_fence::execute_cookie_subject_mode(
        control,
        company,
        CookieAuthorization {
            cookie: context.cookie,
            clock: context.clock,
            requirement: SessionRequirement::Current,
        },
        context.selection,
        permission,
        matches!(
            action,
            Action::OriginalClaims(kind, _) if kind.readonly()
        ) || matches!(
            action,
            Action::Preview(_)
                | Action::Lifecycle(rentemester_storage::invoice_lifecycle::Kind::Render, _)
        ),
        |tx, actor| {
            Ok(match action {
                Action::OriginalClaims(kind, body) => {
                    Output::Json(crate::invoice_claims_original::apply(
                        tx,
                        guard,
                        kind,
                        body,
                        crate::invoice_lifecycle::Context {
                            slug,
                            subject: actor,
                            principal: rentemester_storage::idempotency::PrincipalKind::User,
                            clock,
                        },
                    )?)
                }
                Action::Lifecycle(kind, body) => Output::Json(crate::invoice_lifecycle::apply(
                    tx,
                    guard,
                    kind,
                    body,
                    crate::invoice_lifecycle::Context {
                        slug,
                        subject: actor,
                        principal: rentemester_storage::idempotency::PrincipalKind::User,
                        clock,
                    },
                )?),
                Action::Original(kind, query) => Output::Json(crate::invoice_original::authorized(
                    tx, guard, kind, query, clock,
                )?),
                Action::Contacts => Output::Json(sales::contacts(tx, slug, today)?),
                Action::DeleteCustomer(id, confirm) => Output::Json(
                    rentemester_storage::contact_delete::delete(tx, id, false, confirm, actor)?,
                ),
                Action::DeleteVendor(id, confirm) => Output::Json(
                    rentemester_storage::contact_delete::delete(tx, id, true, confirm, actor)?,
                ),
                Action::Vendor(input) => {
                    Output::Json(rentemester_storage::vendors::create(tx, input, actor)?)
                }
                Action::UpdateVendor(id, input) => {
                    Output::Json(rentemester_storage::vendors::update(tx, id, input, actor)?)
                }
                Action::Customers(input) => Output::Json(sales::create_customer(tx, input, actor)?),
                Action::UpdateCustomer(id, input) => {
                    Output::Json(sales::update_customer(tx, id, input, actor)?)
                }
                Action::List => Output::Json(sales::list(tx, slug, today, year)?),
                Action::Issue(input) | Action::Preview(input) => {
                    let issue = matches!(action, Action::Issue(_));
                    let payload = sales::prepare(tx, input, now, issue)?;
                    if !issue {
                        Output::Pdf(rentemester_engine::invoice_pdf::build(&payload))
                    } else {
                        Output::Json(issue_payload(tx, guard, &payload, actor)?)
                    }
                }
                Action::Post(input) => {
                    verify_invoice(tx, guard, input.invoice_document_id)?;
                    Output::Json(sales::post(tx, input, actor, Date::parse(today)?)?)
                }
                Action::Settle(input) => {
                    verify_invoice(tx, guard, input.invoice_document_id)?;
                    Output::Json(rentemester_storage::invoice_settlement::settle(
                        tx,
                        input,
                        actor,
                        Date::parse(today)?,
                    )?)
                }
                Action::Refund(input) => {
                    verify_invoice(tx, guard, input.invoice_document_id)?;
                    Output::Json(rentemester_storage::invoice_settlement::refund(
                        tx,
                        input,
                        actor,
                        Date::parse(today)?,
                    )?)
                }
                Action::Credit(input) => {
                    verify_invoice(tx, guard, input.invoice_document_id)?;
                    let prepared = rentemester_storage::credit_note::prepare(tx, input, now)?;
                    let no = prepared.payload["creditNoteNumber"]
                        .as_str()
                        .ok_or_else(evidence_error)?;
                    let file = Prepared::new(
                        format!("{no}.json"),
                        serde_json::to_vec_pretty(&prepared.payload)
                            .map_err(|_| evidence_error())?,
                    )
                    .map_err(|_| evidence_error())?;
                    let path = guard
                        .publish_document(&file)
                        .map_err(|_| evidence_error())?;
                    let latest = rentemester_runtime::posting_time::latest_date(
                        clock.now_unix_ms().map_err(|_| evidence_error())?,
                        45,
                    )
                    .map_err(|_| evidence_error())?;
                    Output::Json(rentemester_storage::credit_note::register_and_post(
                        tx,
                        prepared,
                        Artifact {
                            filename: file.filename(),
                            path: &path,
                            hash: file.hash(),
                            mime: file.mime(),
                        },
                        actor,
                        Date::parse(&latest)?,
                    )?)
                }
                Action::Pdf(id) => Output::Pdf(original(tx, guard, sales::pdf_id(tx, id)?)?),
            })
        },
    )
}

/// Publish the same immutable JSON/PDF pair for ordinary and recurring issuance.
pub(crate) fn issue_payload(
    tx: &rusqlite::Transaction<'_>,
    guard: &mut NamespaceGuard,
    payload: &Value,
    actor: &str,
) -> Result<Value, Error> {
    issue_payload_with_principal(
        tx,
        guard,
        payload,
        actor,
        rentemester_storage::idempotency::PrincipalKind::User,
    )
}
pub(crate) fn issue_payload_with_principal(
    tx: &rusqlite::Transaction<'_>,
    guard: &mut NamespaceGuard,
    payload: &Value,
    actor: &str,
    principal: rentemester_storage::idempotency::PrincipalKind,
) -> Result<Value, Error> {
    let pdf = rentemester_engine::invoice_pdf::build(payload);
    let no = payload["invoiceNumber"]
        .as_str()
        .ok_or_else(evidence_error)?;
    let json = Prepared::new(
        format!("{no}.json"),
        serde_json::to_vec_pretty(&payload).map_err(|_| evidence_error())?,
    )
    .map_err(|_| evidence_error())?;
    let pdf = Prepared::new(format!("{no}.pdf"), pdf).map_err(|_| evidence_error())?;
    let json_path = guard
        .publish_document(&json)
        .map_err(|_| evidence_error())?;
    let pdf_path = guard.publish_document(&pdf).map_err(|_| evidence_error())?;
    sales::register_with_principal(
        tx,
        payload,
        Artifact {
            filename: json.filename(),
            path: &json_path,
            hash: json.hash(),
            mime: json.mime(),
        },
        Artifact {
            filename: pdf.filename(),
            path: &pdf_path,
            hash: pdf.hash(),
            mime: pdf.mime(),
        },
        actor,
        principal,
    )
}

/// Explicit service sales slice, using the same domain/evidence logic as human sales.
pub fn execute_service(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: crate::service_company::Context<'_>,
    request: Request<'_>,
) -> Result<crate::service_company::ServiceResult<Output>, crate::service_company::ServiceFailure> {
    use crate::service_company::{self, OperationResult};
    use rentemester_storage::idempotency::PrincipalKind;
    let Request {
        action,
        today,
        now,
        year,
    } = request;
    let (permission, mutation) = match action {
        Action::OriginalClaims(kind, _) => (
            crate::invoice_claims_original::permission(kind),
            !kind.readonly(),
        ),
        Action::Lifecycle(kind, _) => (
            crate::invoice_lifecycle::permission(kind),
            !matches!(kind, rentemester_storage::invoice_lifecycle::Kind::Render),
        ),
        Action::Contacts | Action::List | Action::Original(..) => (Permission::CompanyRead, false),
        Action::Customers(_) => (Permission::CompanyMasterData, true),
        Action::Preview(_) => (Permission::CompanyDraftWrite, false),
        Action::Issue(_) => (Permission::CompanyDraftWrite, true),
        Action::Post(_) | Action::Settle(_) | Action::Credit(_) | Action::Refund(_) => {
            (Permission::CompanyLedgerPost, true)
        }
        Action::Pdf(_) => (Permission::CompanyDocumentsRead, false),
        _ => {
            return Err(service_company::initial(Error::new(
                "access_denied",
                "service sales action not supported",
            )));
        }
    };
    let slug = context.selection.slug;
    let clock = context.clock;
    service_company::execute_operation(
        control,
        company,
        context,
        permission,
        mutation,
        |tx, subject, _| {
            let output = match action {
                Action::OriginalClaims(kind, body) => {
                    Output::Json(crate::invoice_claims_original::apply(
                        tx,
                        guard,
                        kind,
                        body,
                        crate::invoice_lifecycle::Context {
                            slug,
                            subject: subject,
                            principal: PrincipalKind::ServiceAccount,
                            clock,
                        },
                    )?)
                }
                Action::Lifecycle(kind, body) => Output::Json(crate::invoice_lifecycle::apply(
                    tx,
                    guard,
                    kind,
                    body,
                    crate::invoice_lifecycle::Context {
                        slug,
                        subject,
                        principal: PrincipalKind::ServiceAccount,
                        clock,
                    },
                )?),
                Action::Original(kind, query) => Output::Json(crate::invoice_original::authorized(
                    tx, guard, kind, query, clock,
                )?),
                Action::Contacts => Output::Json(sales::contacts(tx, slug, today)?),
                Action::Customers(input) => Output::Json(sales::create_customer_with_principal(
                    tx,
                    input,
                    subject,
                    PrincipalKind::ServiceAccount,
                )?),
                Action::List => Output::Json(sales::list(tx, slug, today, year)?),
                Action::Preview(input) => {
                    let payload = sales::prepare(tx, input, now, false)?;
                    Output::Pdf(rentemester_engine::invoice_pdf::build(&payload))
                }
                Action::Issue(input) => {
                    let payload = sales::prepare(tx, input, now, true)?;
                    Output::Json(issue_payload_with_principal(
                        tx,
                        guard,
                        &payload,
                        subject,
                        PrincipalKind::ServiceAccount,
                    )?)
                }
                Action::Post(input) => {
                    verify_invoice(tx, guard, input.invoice_document_id)?;
                    Output::Json(sales::post_with_principal(
                        tx,
                        input,
                        subject,
                        Date::parse(today)?,
                        PrincipalKind::ServiceAccount,
                    )?)
                }
                Action::Settle(input) => {
                    verify_invoice(tx, guard, input.invoice_document_id)?;
                    Output::Json(
                        rentemester_storage::invoice_settlement::settle_with_principal(
                            tx,
                            input,
                            subject,
                            Date::parse(today)?,
                            PrincipalKind::ServiceAccount,
                        )?,
                    )
                }
                Action::Credit(input) => {
                    let latest = rentemester_runtime::posting_time::latest_date(
                        clock.now_unix_ms().map_err(|_| evidence_error())?,
                        45,
                    )
                    .map_err(|_| evidence_error())?;
                    Output::Json(crate::invoice_lifecycle::credit(
                        tx,
                        guard,
                        input,
                        now,
                        subject,
                        Date::parse(&latest)?,
                        PrincipalKind::ServiceAccount,
                    )?)
                }
                Action::Refund(input) => {
                    verify_invoice(tx, guard, input.invoice_document_id)?;
                    Output::Json(
                        rentemester_storage::invoice_settlement::refund_with_principal(
                            tx,
                            input,
                            subject,
                            Date::parse(today)?,
                            PrincipalKind::ServiceAccount,
                        )?,
                    )
                }
                Action::Pdf(id) => Output::Pdf(original(tx, guard, sales::pdf_id(tx, id)?)?),
                _ => unreachable!("service action admitted above"),
            };
            Ok(OperationResult {
                result: output,
                receipt: None,
            })
        },
    )
}

#[cfg(test)]
mod tests;
