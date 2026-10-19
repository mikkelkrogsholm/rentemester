//! Fixed archive operations and signed-snapshot original assurance on the admitted Owner.
use super::*;
use rentemester_application::archive_import::{self as app, Action, Output};
impl Owner {
    pub fn archive_import(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        action: Action<'_>,
    ) -> Result<
        Attempt<Result<Committed<Output>, rentemester_application::control_fence::Failure>>,
        OwnerError,
    > {
        self.document_action(cookie, clock, |c, l, g, context| {
            app::execute(c, l, g, context, action)
        })
    }
}
pub(crate) fn validate_live_sources(
    db: &Connection,
    guard: &mut NamespaceGuard,
) -> Result<(), rentemester_engine::Error> {
    app::verify_physical(db, guard).map(|_| ())
}
pub(crate) fn validate_archive_sources(
    db: &Connection,
    artifact: &rentemester_runtime::backup_artifact::VerifiedArtifact,
    slug: &str,
) -> Result<(), rentemester_engine::Error> {
    use rentemester_runtime::backup_artifact::EntryKind;
    rentemester_storage::archive_import::validate_history(db)?;
    for reference in rentemester_storage::archive_import::originals(db)? {
        let path = format!("{slug}/{}", reference.path);
        let entry = artifact
            .entries()
            .iter()
            .find(|e| {
                e.path() == path
                    && matches!(e.kind(),EntryKind::DocumentOriginal{company}if company==slug)
            })
            .ok_or_else(|| {
                rentemester_engine::Error::new(
                    "archive_source_unavailable",
                    "Signed archive original missing",
                )
            })?;
        rentemester_storage::archive_import::verify_original(db, &reference, entry.bytes())?;
    }
    Ok(())
}
#[cfg(all(test, target_os = "macos"))]
mod tests;
