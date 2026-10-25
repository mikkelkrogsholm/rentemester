//! Current all-company authority and independent source-profile semantic admission.
use rentemester_engine::Error;
use rentemester_runtime::{
    auth::{Clock, CookieVerifier, VerifiedCookie},
    bootstrap::Operator,
    original716::transfer::TrustedArchive,
};
use rusqlite::Transaction;
pub fn authorize(
    tx: &Transaction<'_>,
    cookies: &CookieVerifier,
    clock: &dyn Clock,
    cookie: &VerifiedCookie,
    catalog: &rentemester_storage::workspace_context::Catalog,
    operator: &Operator,
) -> Result<(), Error> {
    crate::backup_original122::original717::restore_authorize(
        tx, cookies, clock, cookie, catalog, operator,
    )
}
pub fn validate(source: &TrustedArchive) -> Result<(), Error> {
    let a = source.archive();
    // Source business JSON has a different codec. Until explicitly mapped,
    // reject even structurally plausible sections rather than discard evidence.
    if [
        &a.manifest().company_knowledge,
        &a.manifest().ownership_graph,
        &a.manifest().workspace_registry,
        &a.manifest().intercompany_dispositions,
        &a.manifest().workspace_inbox,
    ]
    .iter()
    .any(|s| s.is_some())
    {
        return Err(rentemester_storage::portable_backup::refused(
            "source business section requires explicit versioned migration",
        ));
    }
    for c in &a.manifest().companies {
        let files = source.stage_files();
        let facts = rentemester_storage::original716::transfer::validate_ledger(
            &files[&format!("{}/data/ledger.sqlite", c.slug)],
        )?;
        let ledger = &files[&format!("{}/data/ledger.sqlite", c.slug)];
        let nested = rentemester_runtime::backup_artifact::original717::read_archive(
            &a.files()[&c.backup.path],
        )
        .map_err(|_| {
            rentemester_storage::portable_backup::refused("source company archive differs")
        })?;
        rentemester_storage::original716::transfer::validate_stats(
            ledger,
            &nested["manifest.json"],
        )?;
        for (path, hash) in facts.originals {
            if files.get(&format!("{}/{path}", c.slug)).is_none_or(|b| {
                rentemester_runtime::backup_artifact::original717::sha256(b) != hash
            }) {
                return Err(rentemester_storage::portable_backup::refused(
                    "source document inventory incomplete",
                ));
            }
        }
        // Source TAR has no bank source slot. Never manufacture one or discard it.
        if !facts.bank_hashes.is_empty() {
            return Err(rentemester_storage::portable_backup::refused(
                "source bank inventory requires explicit versioned migration",
            ));
        }
    }
    Ok(())
}
