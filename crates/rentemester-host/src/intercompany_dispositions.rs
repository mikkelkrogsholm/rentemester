//! Fixed admitted two-company observations under the SAME control writer and Owner lease.
use super::*;
use rentemester_application::{
    intercompany_dispositions as app,
    password_login::{Failure, FailureStage},
};
use rentemester_engine::Error;
use rentemester_runtime::auth::CookieVerifier;
use rentemester_storage::intercompany_dispositions as storage;
use rusqlite::{Transaction, TransactionBehavior, params};
use serde_json::Value;
fn unavailable(_: impl std::fmt::Display) -> Error {
    Error::new(
        "disposition_source_unavailable",
        "Admitted disposition source unavailable",
    )
}
fn switch(
    company: &mut Option<Connection>,
    guard: &mut NamespaceGuard,
    root: &Path,
    slug: &str,
    current: &mut String,
) -> Result<(), Error> {
    if current == slug {
        return Ok(());
    }
    if let Some(db) = company.take()
        && let Err((db, e)) = db.close()
    {
        *company = Some(db);
        return Err(unavailable(e));
    }
    guard.select_company(slug).map_err(unavailable)?;
    *company = Some(
        Connection::open_with_flags(
            root.join(slug).join("data/ledger.sqlite"),
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NOFOLLOW,
        )
        .map_err(unavailable)?,
    );
    company
        .as_ref()
        .unwrap()
        .execute_batch("PRAGMA foreign_keys=ON;PRAGMA synchronous=FULL;PRAGMA trusted_schema=OFF;")
        .map_err(unavailable)?;
    *current = slug.into();
    Ok(())
}

fn collect(
    db: &Connection,
    slug: &str,
    req: &storage::Requirements,
) -> Result<storage::JournalSnapshot, Error> {
    let mut ids: std::collections::BTreeSet<i64> = req
        .linked_journal_ids
        .get(slug)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .collect();
    for d in &req.dispositions {
        let side = if d.left.company_slug == slug {
            &d.left
        } else if d.right.company_slug == slug {
            &d.right
        } else {
            continue;
        };
        let column = if side.expected_side == "receivable" {
            "debit_amount"
        } else {
            "credit_amount"
        };
        let query = format!(
            "SELECT j.id FROM journal_entries j WHERE j.transaction_date=?1 AND j.status='posted' AND j.reversal_of_entry_id IS NULL AND NOT EXISTS(SELECT 1 FROM journal_entries r WHERE r.reversal_of_entry_id=j.id) AND EXISTS(SELECT 1 FROM journal_lines l WHERE l.journal_entry_id=j.id AND l.{column}=CAST(?2 AS NUMERIC)) ORDER BY j.id"
        );
        let mut q = db.prepare(&query).map_err(unavailable)?;
        let mut rows = q
            .query(params![d.economic_date, d.amount.to_string()])
            .map_err(unavailable)?;
        let mut count = 0;
        while let Some(r) = rows.next().map_err(unavailable)? {
            count += 1;
            if count > 200 {
                return Err(Error::new(
                    "disposition_candidates_exceed_200",
                    "Complete candidate set exceeds bound",
                ));
            }
            ids.insert(r.get(0).map_err(unavailable)?);
        }
    }
    let mut entries = vec![];
    for id in ids {
        if let Some(e) = storage::journal_entry(db, id)? {
            entries.push(e);
        }
    }
    Ok(storage::JournalSnapshot {
        company_slug: slug.into(),
        ledger_uuid: db
            .query_row(
                "SELECT ledger_uuid FROM ledger_identity WHERE id=1",
                [],
                |r| r.get(0),
            )
            .map_err(unavailable)?,
        currency: db
            .query_row("SELECT currency FROM companies WHERE id=1", [], |r| {
                r.get(0)
            })
            .map_err(unavailable)?,
        ledger_head_hash: db
            .query_row(
                "SELECT entry_hash FROM journal_entries ORDER BY id DESC LIMIT 1",
                [],
                |r| r.get(0),
            )
            .optional()
            .map_err(unavailable)?,
        entries,
    })
}
use rusqlite::OptionalExtension;
impl Owner {
    pub fn intercompany_dispositions(
        &mut self,
        cookies: &CookieVerifier,
        clock: &dyn Clock,
        proof: &VerifiedCookie,
        anchor: Option<&str>,
        mut action: storage::Action,
    ) -> AuthAttempt<Value> {
        let catalog = self.catalog()?;
        self.admit_auth(cookies.workspace())?;
        let selected = self.slug.clone();
        let slots = self.held.0.as_mut().unwrap();
        let tx = slots
            .control
            .as_mut()
            .unwrap()
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| OwnerError::Database)?;
        let outcome = (|| {
            let auth = app::authorize(&tx, cookies, clock, proof, &catalog, anchor, &mut action)?;
            let mut snapshots = vec![];
            for slug in &auth.requirements().company_slugs {
                switch(
                    &mut slots.company,
                    &mut slots.guard,
                    &self.root,
                    slug,
                    &mut self.slug,
                )?;
                slots
                    .guard
                    .ensure_connections(&tx, slots.company.as_ref().unwrap())
                    .map_err(unavailable)?;
                let db = slots.company.as_mut().unwrap();
                let read = db
                    .transaction_with_behavior(TransactionBehavior::Deferred)
                    .map_err(unavailable)?;
                // Physical originals and canonical native chain are observations, never period decisions.
                if !rentemester_application::periods::verify_sources(&read, &mut slots.guard)
                    .map_err(unavailable)?
                    .is_empty()
                {
                    return Err(unavailable("physical journal source differs"));
                }
                snapshots.push(collect(&read, slug, auth.requirements())?);
                read.rollback().map_err(unavailable)?;
            }
            app::execute(&tx, &auth, &action, &snapshots)
        })();
        let restore = switch(
            &mut slots.company,
            &mut slots.guard,
            &self.root,
            &selected,
            &mut self.slug,
        )
        .and_then(|_| {
            slots
                .guard
                .ensure_connections(&tx, slots.company.as_ref().unwrap())
                .map_err(unavailable)
        });
        let outcome = match (outcome, restore) {
            (Ok(value), Ok(())) => {
                if app::is_write(&action) {
                    tx.commit().map(|_| value).map_err(|_| Failure {
                        stage: FailureStage::OutcomeUnknown,
                        error: Error::new(
                            "commit_outcome_unknown",
                            "Disposition commit outcome unknown",
                        ),
                        connection_reusable: false,
                    })
                } else {
                    tx.rollback().map(|_| value).map_err(|_| Failure {
                        stage: FailureStage::NotCommitted,
                        error: unavailable("rollback"),
                        connection_reusable: false,
                    })
                }
            }
            (result, restored) => {
                let reusable = tx.rollback().is_ok() && restored.is_ok();
                Err(Failure {
                    stage: FailureStage::NotCommitted,
                    error: restored.err().unwrap_or_else(|| result.unwrap_err()),
                    connection_reusable: reusable,
                })
            }
        };
        let reusable = outcome
            .as_ref()
            .map_or_else(|f| f.connection_reusable, |_| true);
        let retired = self.finish(reusable);
        Ok(Attempt { outcome, retired })
    }
}
/// Fixed snapshot/restore staging hook: validates historical identities and real chain-prefix heads.
/// Later postings or reversals remain legitimate history and do not invalidate signed preservation.
pub(crate) fn validate_historical_company(
    control: &Transaction<'_>,
    db: &Connection,
    slug: &str,
    ledger_uuid: &str,
) -> Result<(), Error> {
    let uuid: String = db
        .query_row(
            "SELECT ledger_uuid FROM ledger_identity WHERE id=1",
            [],
            |r| r.get(0),
        )
        .map_err(unavailable)?;
    if uuid != ledger_uuid || rentemester_storage::portable_backup::company(db)?.uuid != ledger_uuid
    {
        return Err(unavailable("historical company identity/chain"));
    }
    let currency: String = db
        .query_row("SELECT currency FROM companies WHERE id=1", [], |r| {
            r.get(0)
        })
        .map_err(unavailable)?;
    for link in storage::validate_history(control)? {
        let side = if link.side == "left" {
            &link.disposition.left
        } else {
            &link.disposition.right
        };
        if side.company_slug != slug {
            continue;
        }
        let e = storage::journal_entry(db, link.journal_entry_id)?
            .ok_or_else(|| unavailable("historical journal missing"))?;
        if e.entry_no != link.journal_entry_no
            || e.entry_hash != link.journal_entry_hash
            || e.transaction_date != link.disposition.economic_date
            || currency != link.disposition.currency
            || e.status != "posted"
            || e.reversal_of_entry_id.is_some()
        {
            return Err(unavailable("historical journal identity differs"));
        }
        let expected = link.disposition.amount_money()?;
        if !e.lines.iter().any(|l| {
            if side.expected_side == "receivable" {
                l.debit == expected
            } else {
                l.credit == expected
            }
        }) {
            return Err(unavailable("historical journal amount differs"));
        }
        if let Some(hash) = link.ledger_head_hash {
            let prefix: bool = db
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM journal_entries WHERE entry_hash=?1 AND id>=?2)",
                    params![hash, e.id],
                    |r| r.get(0),
                )
                .map_err(unavailable)?;
            if !prefix {
                return Err(unavailable("historical head is not a real journal prefix"));
            }
        } else {
            return Err(unavailable(
                "historical linked journal requires an actual head",
            ));
        }
    }
    Ok(())
}
#[cfg(all(test, target_os = "macos"))]
mod tests;
