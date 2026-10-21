//! Native Dinero actions use the same admitted owner and namespace guard as documents.
use super::*;
use rentemester_application::dinero_import::{self as app, Action, Output};
impl Owner {
    pub fn dinero_import(
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
    rentemester_storage::dinero_import::validate_history(db)?;
    for reference in rentemester_storage::dinero_import::originals(db)? {
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
                    "dinero_source_unavailable",
                    "Signed archive Dinero source missing",
                )
            })?;
        rentemester_storage::dinero_import::verify_original(db, &reference, entry.bytes())?;
    }
    for reference in rentemester_storage::dinero_import::supporting_originals(db)? {
        let path = format!("{slug}/{}", reference.path);
        let _entry = artifact
            .entries()
            .iter()
            .find(|e| {
                e.path() == path
                    && matches!(e.kind(),EntryKind::DocumentOriginal{company}if company==slug)
            })
            .ok_or_else(|| {
                rentemester_engine::Error::new(
                    "dinero_source_unavailable",
                    "Signed archive Dinero supporting original missing",
                )
            })?;
        // VerifiedArtifact has already authenticated the exact manifest bytes.
        if !artifact
            .manifest()
            .inventory
            .iter()
            .any(|e| e.path == path && e.sha256 == reference.hash)
        {
            return Err(rentemester_engine::Error::new(
                "dinero_source_unavailable",
                "Signed archive Dinero supporting original differs",
            ));
        }
    }
    Ok(())
}
