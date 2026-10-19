//! Reservation, fresh authority, one SMTP handoff, durable outcome. Never a transaction during SMTP.
use crate::{
    access::Permission,
    control_fence::{
        self, Committed, CookieAuthorization, Failure, FailureStage, SessionRequirement,
    },
    documents::Context,
};
use rentemester_engine::Error;
use rentemester_runtime::{
    auth::{Clock, new_token},
    invoice_mail::{self, Handoff, Message, SenderIdentity, Transport},
    namespace::NamespaceGuard,
};
use rentemester_storage::invoice_email::{
    self as store, Reconcile, Reservation, Retry, Send, Sender, Source,
};
use rusqlite::{Connection, Transaction};
use serde_json::{Value, json};
fn fail(error: Error) -> Failure {
    Failure {
        stage: FailureStage::NotCommitted,
        error,
        control_reusable: true,
        company_reusable: true,
    }
}
fn at(clock: &dyn Clock) -> Result<String, Error> {
    store::timestamp(clock.now_unix_ms().map_err(|_| store::refused())?)
}
fn source_sender(s: &SenderIdentity) -> Sender {
    Sender {
        host: s.host.clone(),
        port: s.port,
        from_address: s.from_address.clone(),
        from_name: s.from_name.clone(),
    }
}
fn identity(s: &Sender) -> SenderIdentity {
    SenderIdentity {
        host: s.host.clone(),
        port: s.port,
        from_address: s.from_address.clone(),
        from_name: s.from_name.clone(),
    }
}
fn build(
    s: &SenderIdentity,
    to: &str,
    kind: &str,
    no: &str,
    pdf: &[u8],
    filename: &str,
) -> Result<Message, Error> {
    invoice_mail::build(s, to, kind, no, pdf, filename)
        .map_err(|e| Error::new(e.code(), "invoice email MIME/source refused"))
}
fn material(
    tx: &Transaction<'_>,
    guard: &mut NamespaceGuard,
    input: &Send,
    sender: &SenderIdentity,
) -> Result<(Source, Message), Error> {
    let (to, kind) = input.validate()?;
    let row:(String,String,String,String,String,String,String)=tx.query_row("SELECT d.invoice_no,d.sha256_hash,d.stored_path,p.sha256_hash,p.stored_path,p.original_filename,d.payload_json FROM documents d JOIN documents p ON p.document_type='issued_invoice_pdf' AND p.invoice_no=d.invoice_no AND p.payload_json=d.payload_json WHERE d.id=?1 AND d.document_type='issued_invoice'",[input.invoice_document_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?))).map_err(|_|store::refused())?;
    let json = guard
        .read_document(&row.2, &row.1)
        .map_err(|_| store::refused())?;
    let parsed: Value = serde_json::from_slice(&json).map_err(|_| store::refused())?;
    let canonical: Value = serde_json::from_str(&row.6).map_err(|_| store::refused())?;
    if parsed != canonical {
        return Err(store::refused());
    }
    let pdf = guard
        .read_document(&row.4, &row.3)
        .map_err(|_| store::refused())?;
    let message = build(sender, &to, &kind, &row.0, &pdf, &row.5)?;
    let ledger = tx
        .query_row(
            "SELECT ledger_uuid FROM ledger_identity WHERE id=1",
            [],
            |r| r.get(0),
        )
        .map_err(|_| store::refused())?;
    let s = Source {
        ledger_uuid: ledger,
        invoice_document_id: input.invoice_document_id,
        invoice_number: row.0,
        kind,
        recipient: to,
        subject: message.subject.clone(),
        message_id: message.message_id.clone(),
        body_sha256: message.body_sha256.clone(),
        json_sha256: row.1,
        pdf_sha256: row.3,
        json_path: row.2,
        pdf_path: row.4,
        pdf_filename: row.5,
        sender: source_sender(sender),
    };
    store::validate_source(tx, &s)?;
    Ok((s, message))
}
fn verify(tx: &Transaction<'_>, guard: &mut NamespaceGuard, s: &Source) -> Result<Message, Error> {
    let (m, msg) = material(
        tx,
        guard,
        &Send {
            invoice_document_id: s.invoice_document_id,
            to: s.recipient.clone(),
            kind: Some(s.kind.clone()),
            confirm: true,
        },
        &identity(&s.sender),
    )?;
    if m != *s {
        return Err(store::refused());
    }
    Ok(msg)
}
fn authorization<'a>(c: &Context<'a>) -> CookieAuthorization<'a> {
    CookieAuthorization {
        cookie: c.cookie,
        clock: c.clock,
        requirement: SessionRequirement::Fresh,
    }
}
pub fn status(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: Context<'_>,
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
        Permission::CompanyRead,
        |tx, _| {
            for s in store::sources(tx)? {
                verify(tx, guard, &s)?;
            }
            Ok(json!({"rows":store::rows(tx)?}))
        },
    )
}
pub fn reconcile(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: Context<'_>,
    i: &Reconcile,
) -> Result<Committed<Value>, Failure> {
    i.validate().map_err(fail)?;
    control_fence::execute_cookie_subject(
        control,
        company,
        authorization(&context),
        context.selection,
        Permission::CompanyExternalSend,
        |tx, actor| {
            let d = store::get(tx, &i.message_id)?.ok_or_else(store::refused)?;
            verify(tx, guard, &d.source)?;
            Ok(json!({"delivery":store::reconcile(tx,i,actor,&at(context.clock)?)?.row(false)}))
        },
    )
}
struct Pending {
    row: Value,
    dispatch: Option<(Source, Message, String)>,
}
enum Intent<'a> {
    Send(&'a Send),
    Retry(&'a Retry),
}
#[allow(clippy::too_many_arguments)]
fn execute(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: Context<'_>,
    intent: Intent<'_>,
    transport: &mut dyn Transport,
) -> Result<Committed<Value>, Failure> {
    match intent {
        Intent::Send(i) => {
            i.validate().map_err(fail)?;
        }
        Intent::Retry(i) => {
            i.validate().map_err(fail)?;
        }
    }
    let sender = transport.identity().clone();
    let selection = || control_fence::CompanySelection {
        workspace: context.selection.workspace,
        slug: context.selection.slug,
        registered: context.selection.registered,
        archived: context.selection.archived,
    };
    let reserved = control_fence::execute_cookie_subject(
        control,
        company,
        authorization(&context),
        selection(),
        Permission::CompanyExternalSend,
        |tx, actor| {
            let (s, msg, retry) = match intent {
                Intent::Send(i) => {
                    let (s, m) = material(tx, guard, i, &sender)?;
                    (s, m, None)
                }
                Intent::Retry(i) => {
                    let d = store::get(tx, &i.message_id)?.ok_or_else(store::refused)?;
                    let current = source_sender(&sender);
                    if current != d.source.sender {
                        return Err(store::refused());
                    }
                    let m = verify(tx, guard, &d.source)?;
                    (d.source, m, Some(i))
                }
            };
            let attempt = new_token().map_err(|_| store::refused())?;
            match store::reserve(
                tx,
                s.clone(),
                attempt.as_str(),
                actor,
                &at(context.clock)?,
                retry,
            )? {
                Reservation::Existing(row) => Ok(Pending {
                    row,
                    dispatch: None,
                }),
                Reservation::Reserved(d) => Ok(Pending {
                    row: d.row(false),
                    dispatch: Some((s, msg, d.state_hash)),
                }),
            }
        },
    )?;
    let Some((source, message, hash)) = reserved.value.dispatch else {
        return Ok(Committed {
            value: json!({"delivery":reserved.value.row}),
            control_reusable: reserved.control_reusable,
        });
    };
    if !reserved.control_reusable {
        return Ok(Committed {
            value: json!({"delivery":reserved.value.row}),
            control_reusable: false,
        });
    }
    // Re-resolve live human/current role and exact reservation under the retained Owner.
    // Commit/rollback both DB transactions BEFORE crossing the fixed SMTP port.
    let ready = control_fence::execute_cookie_subject(
        control,
        company,
        authorization(&context),
        selection(),
        Permission::CompanyExternalSend,
        |tx, _| {
            let d = store::get(tx, &source.message_id)?.ok_or_else(store::refused)?;
            if d.state_hash != hash
                || d.source != source
                || source_sender(transport.identity()) != source.sender
            {
                return Err(store::refused());
            }
            verify(tx, guard, &source)?;
            Ok(())
        },
    );
    match ready {
        Ok(r) if r.control_reusable => {}
        Ok(_) => {
            return Ok(Committed {
                value: json!({"delivery":reserved.value.row}),
                control_reusable: false,
            });
        }
        Err(mut f) => {
            f.stage = FailureStage::OutcomeUnknown;
            return Err(f);
        }
    }
    control_fence::connection_ready(control)
        .and_then(|_| control_fence::connection_ready(company))
        .map_err(|error| Failure {
            stage: FailureStage::OutcomeUnknown,
            error,
            control_reusable: false,
            company_reusable: false,
        })?;
    let outcome = match transport.send(&message) {
        Handoff::Accepted => "accepted",
        Handoff::Refused => "refused",
        Handoff::Unknown => "unknown",
    };
    let terminal = control_fence::execute_cookie_subject(
        control,
        company,
        authorization(&context),
        selection(),
        Permission::CompanyExternalSend,
        |tx, actor| {
            verify(tx, guard, &source)?;
            Ok(
                json!({"delivery":store::terminal(tx,&source.message_id,&hash,outcome,actor,&at(context.clock)?)?.row(false)}),
            )
        },
    );
    // A later SQL/authority failure cannot turn an attempted handoff into a safe retry signal.
    terminal.map_err(|mut f| {
        f.stage = FailureStage::OutcomeUnknown;
        f
    })
}
pub fn send(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: Context<'_>,
    input: &Send,
    transport: &mut dyn Transport,
) -> Result<Committed<Value>, Failure> {
    execute(
        control,
        company,
        guard,
        context,
        Intent::Send(input),
        transport,
    )
}
pub fn retry(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: Context<'_>,
    input: &Retry,
    transport: &mut dyn Transport,
) -> Result<Committed<Value>, Failure> {
    execute(
        control,
        company,
        guard,
        context,
        Intent::Retry(input),
        transport,
    )
}
/// Fixed original/MIME verification shared by the live namespace and signed-archive adapters.
/// No file opening, authority creation, transport invocation or writes.
pub fn verify_source_bytes(
    db: &Connection,
    s: &Source,
    json_bytes: &[u8],
    pdf: &[u8],
) -> Result<(), Error> {
    store::validate_source(db, s)?;
    if store::sha(json_bytes) != s.json_sha256 || store::sha(pdf) != s.pdf_sha256 {
        return Err(store::refused());
    }
    let stored: String = db
        .query_row(
            "SELECT payload_json FROM documents WHERE id=?1",
            [s.invoice_document_id],
            |r| r.get(0),
        )
        .map_err(|_| store::refused())?;
    let physical: Value = serde_json::from_slice(json_bytes).map_err(|_| store::refused())?;
    if physical != serde_json::from_str::<Value>(&stored).map_err(|_| store::refused())? {
        return Err(store::refused());
    }
    let message = build(
        &identity(&s.sender),
        &s.recipient,
        &s.kind,
        &s.invoice_number,
        pdf,
        &s.pdf_filename,
    )?;
    if message.message_id != s.message_id
        || message.body_sha256 != s.body_sha256
        || message.subject != s.subject
        || message.to != s.recipient
    {
        return Err(store::refused());
    }
    Ok(())
}
