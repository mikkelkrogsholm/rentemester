//! Fixed invoice email operations under the same Owner lease/mutex and guarded original namespace.
use super::*;
use rentemester_application::invoice_email as app;
use rentemester_engine::Error;
use rentemester_runtime::{backup_artifact, invoice_mail::Transport};
use rentemester_storage::invoice_email::{self as store, Reconcile, Retry, Send};
use serde_json::Value;
impl Owner {
    pub fn invoice_email_status(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
    ) -> Result<
        Attempt<Result<Committed<Value>, rentemester_application::control_fence::Failure>>,
        OwnerError,
    > {
        self.document_action(cookie, clock, |c, l, g, context| {
            app::status(c, l, g, context)
        })
    }
    pub fn send_invoice_email(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        input: &Send,
        transport: &mut dyn Transport,
    ) -> Result<
        Attempt<Result<Committed<Value>, rentemester_application::control_fence::Failure>>,
        OwnerError,
    > {
        self.document_action(cookie, clock, |c, l, g, context| {
            app::send(c, l, g, context, input, transport)
        })
    }
    pub fn reconcile_invoice_email(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        input: &Reconcile,
    ) -> Result<
        Attempt<Result<Committed<Value>, rentemester_application::control_fence::Failure>>,
        OwnerError,
    > {
        self.document_action(cookie, clock, |c, l, g, context| {
            app::reconcile(c, l, g, context, input)
        })
    }
    pub fn retry_invoice_email(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        input: &Retry,
        transport: &mut dyn Transport,
    ) -> Result<
        Attempt<Result<Committed<Value>, rentemester_application::control_fence::Failure>>,
        OwnerError,
    > {
        self.document_action(cookie, clock, |c, l, g, context| {
            app::retry(c, l, g, context, input, transport)
        })
    }
}
pub(super) fn validate_live_sources(
    db: &Connection,
    guard: &mut NamespaceGuard,
) -> Result<(), Error> {
    for s in store::sources(db)? {
        let json = guard
            .read_document(&s.json_path, &s.json_sha256)
            .map_err(|_| store::refused())?;
        let pdf = guard
            .read_document(&s.pdf_path, &s.pdf_sha256)
            .map_err(|_| store::refused())?;
        app::verify_source_bytes(db, &s, &json, &pdf)?;
    }
    Ok(())
}
pub(super) fn validate_archive_sources(
    db: &Connection,
    a: &backup_artifact::VerifiedArtifact,
    slug: &str,
) -> Result<(), Error> {
    for s in store::sources(db)? {
        let original = |path: &str| -> Result<&[u8], Error> {
            let p = format!("{slug}/{path}");
            a.entries().iter().find(|e|e.path()==p&&matches!(e.kind(),backup_artifact::EntryKind::DocumentOriginal{company}if company==slug)).map(|e|e.bytes()).ok_or_else(store::refused)
        };
        app::verify_source_bytes(db, &s, original(&s.json_path)?, original(&s.pdf_path)?)?;
    }
    Ok(())
}
#[cfg(test)]
mod tests;
