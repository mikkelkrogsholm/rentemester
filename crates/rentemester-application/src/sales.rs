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
    Contacts,
    Customers(&'a Customer),
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
fn verify_invoice(
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
        Action::Contacts | Action::List => Permission::CompanyRead,
        Action::Customers(_) | Action::UpdateCustomer(_, _) => Permission::CompanyMasterData,
        Action::Issue(_) | Action::Preview(_) => Permission::CompanyDraftWrite,
        Action::Post(_) | Action::Settle(_) | Action::Credit(_) | Action::Refund(_) => {
            Permission::CompanyLedgerPost
        }
        Action::Pdf(_) => Permission::CompanyDocumentsRead,
    };
    let slug = context.selection.slug;
    let clock = context.clock;
    control_fence::execute_cookie_subject(
        control,
        company,
        CookieAuthorization {
            cookie: context.cookie,
            clock: context.clock,
            requirement: SessionRequirement::Current,
        },
        context.selection,
        permission,
        |tx, actor| {
            Ok(match action {
                Action::Contacts => Output::Json(sales::contacts(tx, slug, today)?),
                Action::Customers(input) => Output::Json(sales::create_customer(tx, input, actor)?),
                Action::UpdateCustomer(id, input) => {
                    Output::Json(sales::update_customer(tx, id, input, actor)?)
                }
                Action::List => Output::Json(sales::list(tx, slug, today, year)?),
                Action::Issue(input) | Action::Preview(input) => {
                    let issue = matches!(action, Action::Issue(_));
                    let payload = sales::prepare(tx, input, now, issue)?;
                    let pdf = rentemester_engine::invoice_pdf::build(&payload);
                    if !issue {
                        Output::Pdf(pdf)
                    } else {
                        let no = payload["invoiceNumber"]
                            .as_str()
                            .ok_or_else(evidence_error)?;
                        let json = Prepared::new(
                            format!("{no}.json"),
                            serde_json::to_vec_pretty(&payload).map_err(|_| evidence_error())?,
                        )
                        .map_err(|_| evidence_error())?;
                        let pdf = Prepared::new(format!("{no}.pdf"), pdf)
                            .map_err(|_| evidence_error())?;
                        let json_path = guard
                            .publish_document(&json)
                            .map_err(|_| evidence_error())?;
                        let pdf_path =
                            guard.publish_document(&pdf).map_err(|_| evidence_error())?;
                        Output::Json(sales::register(
                            tx,
                            &payload,
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
                        )?)
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
