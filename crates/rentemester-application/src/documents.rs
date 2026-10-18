//! Document use cases share the same current-role fence as journal posting.
use crate::{
    access::Permission,
    control_fence::{
        self, Committed, CompanySelection, CookieAuthorization, Failure, SessionRequirement,
    },
};
use rentemester_engine::{Error, document::Metadata};
use rentemester_runtime::{
    auth::{Clock, VerifiedCookie},
    document::Prepared,
    namespace::NamespaceGuard,
};
use rusqlite::Connection;
use serde_json::Value;
fn unavailable() -> Error {
    Error::new(
        "document_evidence_unavailable",
        "document evidence unavailable or integrity mismatch",
    )
}
pub struct Ingest<'a> {
    pub file: &'a Prepared,
    pub metadata: &'a Metadata,
    pub today: &'a str,
}
pub struct Context<'a> {
    pub cookie: &'a VerifiedCookie,
    pub clock: &'a dyn Clock,
    pub selection: CompanySelection<'a>,
}
pub fn ingest(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: Context<'_>,
    input: Ingest<'_>,
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
        Permission::CompanyDocumentsUpload,
        |tx, subject| {
            input.metadata.validate()?;
            // Bytes are durable before registration. Rollback/unknown commit never
            // deletes a possible winner; an unregistered private blob can be retried.
            let path = guard
                .publish_document(input.file)
                .map_err(|_| unavailable())?;
            rentemester_storage::documents::register(
                tx,
                rentemester_storage::documents::Original {
                    filename: input.file.filename(),
                    hash: input.file.hash(),
                    path: &path,
                    mime: input.file.mime(),
                },
                input.metadata,
                subject,
                input.today,
            )
        },
    )
}
pub struct Original {
    pub bytes: Vec<u8>,
    pub filename: String,
    pub mime: String,
}
pub fn read(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: Context<'_>,
    id: i64,
) -> Result<Committed<Original>, Failure> {
    control_fence::execute_cookie_subject(
        control,
        company,
        CookieAuthorization {
            cookie: context.cookie,
            clock: context.clock,
            requirement: SessionRequirement::Current,
        },
        context.selection,
        Permission::CompanyDocumentsRead,
        |tx, _| {
            let evidence = rentemester_storage::documents::evidence(tx, id)?;
            let bytes = guard
                .read_document(&evidence.path, &evidence.hash)
                .map_err(|_| unavailable())?;
            Ok(Original {
                bytes,
                filename: evidence.filename,
                mime: evidence.mime,
            })
        },
    )
}
pub fn list(
    control: &mut Connection,
    company: &mut Connection,
    context: Context<'_>,
) -> Result<Committed<Value>, Failure> {
    let slug = context.selection.slug;
    control_fence::execute_cookie_subject(
        control,
        company,
        CookieAuthorization {
            cookie: context.cookie,
            clock: context.clock,
            requirement: SessionRequirement::Current,
        },
        context.selection,
        Permission::CompanyDocumentsRead,
        |tx, _| rentemester_storage::documents::list(tx, slug),
    )
}
